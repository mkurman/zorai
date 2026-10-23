#[cfg(test)]
use super::*;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex as StdMutex};
use tempfile::tempdir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::time::{timeout, Duration};
use zorai_shared::providers::PROVIDER_ID_OPENAI;

async fn spawn_recording_heartbeat_server(
    recorded_bodies: Arc<StdMutex<VecDeque<String>>>,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind recording heartbeat server");
    let addr = listener.local_addr().expect("heartbeat server local addr");

    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let recorded_bodies = recorded_bodies.clone();
            tokio::spawn(async move {
                let body = read_http_request_body(&mut socket)
                    .await
                    .expect("read heartbeat request");
                recorded_bodies
                    .lock()
                    .expect("lock recorded heartbeat requests")
                    .push_back(body);

                let response = concat!(
                    "HTTP/1.1 200 OK\r\n",
                    "content-type: text/event-stream\r\n",
                    "cache-control: no-cache\r\n",
                    "connection: close\r\n",
                    "\r\n",
                    "data: {\"choices\":[{\"delta\":{\"content\":\"ACTIONABLE: false\\nDIGEST: All systems normal.\\nITEMS:\\n\"}}]}\n\n",
                    "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":3},\"content\":\"\"}\n\n",
                    "data: [DONE]\n\n"
                );
                socket
                    .write_all(response.as_bytes())
                    .await
                    .expect("write heartbeat response");
            });
        }
    });

    format!("http://{addr}/v1")
}

async fn read_http_request_body(socket: &mut tokio::net::TcpStream) -> std::io::Result<String> {
    let mut buffer = Vec::with_capacity(65536);
    let mut temp = [0u8; 4096];
    let headers_end = loop {
        let read = socket.read(&mut temp).await?;
        if read == 0 {
            return Ok(String::new());
        }
        buffer.extend_from_slice(&temp[..read]);
        if let Some(index) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
            break index + 4;
        }
    };

    let headers = String::from_utf8_lossy(&buffer[..headers_end]);
    let content_length = headers
        .lines()
        .find_map(|line| {
            let mut parts = line.splitn(2, ':');
            let name = parts.next()?.trim();
            let value = parts.next()?.trim();
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0);

    while buffer.len().saturating_sub(headers_end) < content_length {
        let read = socket.read(&mut temp).await?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&temp[..read]);
    }

    let available = buffer.len().saturating_sub(headers_end).min(content_length);
    Ok(String::from_utf8_lossy(&buffer[headers_end..headers_end + available]).to_string())
}

#[test]
fn quiet_hours_within_midnight_wrap_window() {
    assert!(check_quiet_window(23, Some(22), Some(6), false));
}

#[test]
fn quiet_hours_outside_midnight_wrap_window() {
    assert!(!check_quiet_window(12, Some(22), Some(6), false));
}

#[test]
fn quiet_hours_midnight_wrap_early_morning() {
    assert!(check_quiet_window(3, Some(22), Some(6), false));
}

#[test]
fn quiet_hours_midnight_wrap_boundary_end() {
    assert!(!check_quiet_window(6, Some(22), Some(6), false));
}

#[test]
fn quiet_hours_midnight_wrap_boundary_start() {
    assert!(check_quiet_window(22, Some(22), Some(6), false));
}

#[test]
fn dnd_enabled_overrides_everything() {
    assert!(check_quiet_window(12, None, None, true));
    assert!(check_quiet_window(12, Some(22), Some(6), true));
    assert!(check_quiet_window(0, Some(9), Some(17), true));
}

#[test]
fn no_quiet_hours_configured_and_no_dnd() {
    assert!(!check_quiet_window(12, None, None, false));
    assert!(!check_quiet_window(0, None, None, false));
    assert!(!check_quiet_window(23, None, None, false));
}

#[test]
fn same_day_range_inside() {
    assert!(check_quiet_window(12, Some(9), Some(17), false));
}

#[test]
fn same_day_range_outside() {
    assert!(!check_quiet_window(20, Some(9), Some(17), false));
}

#[test]
fn partial_config_only_start_set() {
    assert!(!check_quiet_window(23, Some(22), None, false));
}

#[test]
fn partial_config_only_end_set() {
    assert!(!check_quiet_window(3, None, Some(6), false));
}

#[tokio::test]
async fn heartbeat_running_goal_targets_use_persisted_running_goals_after_live_queue_clear() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let goal_run = engine
        .start_goal_run(
            "Persisted heartbeat target goal".to_string(),
            Some("Persisted heartbeat target goal".to_string()),
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await;
    {
        let mut goal_runs = engine.goal_runs.lock().await;
        let goal = goal_runs
            .iter_mut()
            .find(|goal| goal.id == goal_run.id)
            .expect("goal should be live before persistence");
        goal.status = GoalRunStatus::Running;
    }
    engine.persist_goal_runs().await;
    engine.goal_runs.lock().await.clear();

    let targets = engine.running_goal_trajectory_targets().await;

    assert_eq!(
        targets,
        vec![(goal_run.id, "Persisted heartbeat target goal".to_string())]
    );
}

#[test]
fn resolve_cron_prefers_explicit_cron() {
    let config = AgentConfig {
        heartbeat_cron: Some("0 * * * *".to_string()),
        heartbeat_interval_mins: 15,
        ..AgentConfig::default()
    };
    assert_eq!(resolve_cron_from_config(&config), "0 * * * *");
}

#[test]
fn resolve_cron_falls_back_to_interval_mins() {
    let config = AgentConfig {
        heartbeat_cron: None,
        heartbeat_interval_mins: 15,
        ..AgentConfig::default()
    };
    assert_eq!(resolve_cron_from_config(&config), "*/15 * * * *");
}

#[test]
fn resolve_cron_with_hourly_interval() {
    let config = AgentConfig {
        heartbeat_cron: None,
        heartbeat_interval_mins: 60,
        ..AgentConfig::default()
    };
    assert_eq!(resolve_cron_from_config(&config), "0 * * * *");
}

#[test]
fn resolve_cron_explicit_overrides_interval() {
    let config = AgentConfig {
        heartbeat_cron: Some("30 2 * * *".to_string()),
        heartbeat_interval_mins: 60,
        ..AgentConfig::default()
    };
    assert_eq!(resolve_cron_from_config(&config), "30 2 * * *");
}

#[test]
fn broadcast_when_actionable_true_and_items_present() {
    let items = vec![HeartbeatDigestItem {
        priority: 1,
        check_type: HeartbeatCheckType::StaleTodos,
        title: "Stale todo".into(),
        suggestion: "Review it".into(),
    }];
    assert!(should_broadcast(true, &items));
}

#[test]
fn broadcast_when_actionable_true_but_no_items() {
    assert!(should_broadcast(true, &[]));
}

#[test]
fn broadcast_when_not_actionable_but_items_present() {
    let items = vec![HeartbeatDigestItem {
        priority: 3,
        check_type: HeartbeatCheckType::RepoChanges,
        title: "Repo change".into(),
        suggestion: "Check it".into(),
    }];
    assert!(should_broadcast(false, &items));
}

#[test]
fn no_broadcast_when_not_actionable_and_no_items() {
    assert!(!should_broadcast(false, &[]));
}

#[test]
fn persistence_status_completed_when_synthesis_present() {
    assert_eq!(
        heartbeat_persistence_status(Some("LLM response text")),
        "completed"
    );
}

#[test]
fn persistence_status_failed_when_synthesis_none() {
    assert_eq!(heartbeat_persistence_status(None), "synthesis_failed");
}

#[test]
fn custom_item_due_when_never_run() {
    assert!(is_custom_item_due(100_000_000, None, 15, 30));
}

#[test]
fn custom_item_due_when_interval_elapsed() {
    let now = 100_000_000;
    let last = now - (16 * 60 * 1000);
    assert!(is_custom_item_due(now, Some(last), 15, 30));
}

#[test]
fn custom_item_not_due_when_interval_not_elapsed() {
    let now = 100_000_000;
    let last = now - (10 * 60 * 1000);
    assert!(!is_custom_item_due(now, Some(last), 15, 30));
}

#[test]
fn custom_item_uses_global_interval_when_item_interval_zero() {
    let now = 100_000_000;
    let last = now - (31 * 60 * 1000);
    assert!(is_custom_item_due(now, Some(last), 0, 30));
}

#[test]
fn custom_item_not_due_with_global_interval() {
    let now = 100_000_000;
    let last = now - (20 * 60 * 1000);
    assert!(!is_custom_item_due(now, Some(last), 0, 30));
}

#[test]
fn all_checks_enabled_by_default() {
    let config = HeartbeatChecksConfig::default();
    let checks = enabled_checks(&config);
    assert_eq!(checks.len(), 5);
    assert!(checks.contains(&HeartbeatCheckType::StaleTodos));
    assert!(checks.contains(&HeartbeatCheckType::StuckGoalRuns));
    assert!(checks.contains(&HeartbeatCheckType::UnrepliedGatewayMessages));
    assert!(checks.contains(&HeartbeatCheckType::RepoChanges));
    assert!(checks.contains(&HeartbeatCheckType::PluginAuth));
}

#[test]
fn only_enabled_checks_are_included() {
    let config = HeartbeatChecksConfig {
        stale_todos_enabled: true,
        stuck_goals_enabled: false,
        unreplied_messages_enabled: false,
        repo_changes_enabled: true,
        plugin_auth_enabled: false,
        ..HeartbeatChecksConfig::default()
    };
    let checks = enabled_checks(&config);
    assert_eq!(checks.len(), 2);
    assert!(checks.contains(&HeartbeatCheckType::StaleTodos));
    assert!(checks.contains(&HeartbeatCheckType::RepoChanges));
    assert!(!checks.contains(&HeartbeatCheckType::StuckGoalRuns));
    assert!(!checks.contains(&HeartbeatCheckType::UnrepliedGatewayMessages));
}

#[test]
fn no_checks_when_all_disabled() {
    let config = HeartbeatChecksConfig {
        stale_todos_enabled: false,
        stuck_goals_enabled: false,
        unreplied_messages_enabled: false,
        repo_changes_enabled: false,
        plugin_auth_enabled: false,
        ..HeartbeatChecksConfig::default()
    };
    let checks = enabled_checks(&config);
    assert!(checks.is_empty());
}

#[test]
fn parse_digest_items_from_valid_response() {
    let response = "\
ACTIONABLE: true
DIGEST: 2 items need attention
ITEMS:
- PRIORITY:1 TYPE:stale_todos TITLE:Stale todo found SUGGESTION:Review pending items
- PRIORITY:3 TYPE:repo_changes TITLE:Uncommitted changes SUGGESTION:Commit or stash";

    let items = parse_digest_items(response);
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].priority, 1);
    assert_eq!(items[0].check_type, HeartbeatCheckType::StaleTodos);
    assert_eq!(items[0].title, "Stale todo found");
    assert_eq!(items[0].suggestion, "Review pending items");
    assert_eq!(items[1].priority, 3);
    assert_eq!(items[1].check_type, HeartbeatCheckType::RepoChanges);
}

#[test]
fn parse_digest_items_empty_when_no_items_section() {
    let response = "ACTIONABLE: false\nDIGEST: All systems normal.";
    let items = parse_digest_items(response);
    assert!(items.is_empty());
}

#[test]
fn parse_digest_items_handles_camelcase_types() {
    let response = "- PRIORITY:2 TYPE:StuckGoalRuns TITLE:Goal stuck SUGGESTION:Cancel it";
    let items = parse_digest_items(response);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].check_type, HeartbeatCheckType::StuckGoalRuns);
}

#[test]
fn format_anticipatory_items_surfaces_proactive_suppression_as_low_priority_transparency() {
    let output = super::helpers::format_anticipatory_items_for_heartbeat(&[AnticipatoryItem {
        id: "proactive_suppression_thread-1".to_string(),
        kind: "proactive_suppression".to_string(),
        title: "Proactive Surfacing Tightened".to_string(),
        summary: "Optional proactive suggestions were suppressed to reduce noise.".to_string(),
        bullets: vec![
            "suppressed_kinds=intent_prediction,morning_brief".to_string(),
            "approval latency increased; optional proactive surfacing is tightened".to_string(),
        ],
        intent_prediction: None,
        confidence: 0.72,
        goal_run_id: None,
        thread_id: Some("thread-1".to_string()),
        preferred_client_surface: Some("conversation".to_string()),
        preferred_attention_surface: Some("conversation:chat".to_string()),
        created_at: 1,
        updated_at: 1,
    }]);

    assert!(output.contains("proactive_suppression"));
    assert!(output.contains("LOW-PRIORITY INFORMATIONAL"));
    assert!(output.contains("suppressed_kinds=intent_prediction,morning_brief"));
    assert!(output.contains("reduce noise"));
}

#[test]
fn format_anticipatory_items_highlights_system_outcome_foresight_for_operator() {
    let output = super::helpers::format_anticipatory_items_for_heartbeat(&[
        AnticipatoryItem {
            id: "system_outcome_foresight_thread-1".to_string(),
            kind: "system_outcome_foresight".to_string(),
            title: "System Outcome Foresight".to_string(),
            summary: "Predicted stale context: hydration-needed risk is elevated".to_string(),
            bullets: vec![
                "prediction_type=stale_context".to_string(),
                "hydration age=16m exceeded session rhythm window=10m".to_string(),
                "semantic alignment degraded across recent thread messages".to_string(),
            ],
            intent_prediction: None,
            confidence: 0.76,
            goal_run_id: None,
            thread_id: Some("thread-1".to_string()),
            preferred_client_surface: Some("conversation".to_string()),
            preferred_attention_surface: Some("conversation:chat".to_string()),
            created_at: 1,
            updated_at: 1,
        },
        AnticipatoryItem {
            id: "system_outcome_foresight_thread-2".to_string(),
            kind: "system_outcome_foresight".to_string(),
            title: "System Outcome Foresight".to_string(),
            summary: "Predicted build/test/risk: build/test failure risk is elevated".to_string(),
            bullets: vec![
                "prediction_type=build_test_risk".to_string(),
                "dirty repo state: modified=2 staged=0 untracked=0".to_string(),
            ],
            intent_prediction: None,
            confidence: 0.78,
            goal_run_id: None,
            thread_id: Some("thread-2".to_string()),
            preferred_client_surface: Some("conversation".to_string()),
            preferred_attention_surface: Some("conversation:chat".to_string()),
            created_at: 2,
            updated_at: 2,
        },
    ]);

    assert!(output.contains("system_outcome_foresight"));
    assert!(output.contains("OPERATOR-VISIBLE FORESIGHT"));
    assert!(output.contains("trigger=stale_context"));
    assert!(output.contains("trigger=build_test_risk"));
    assert!(output.contains("stale context"));
    assert!(output.contains("build/test failure risk"));
    assert!(output.contains("prediction_type=stale_context"));
    assert!(output.contains("prediction_type=build_test_risk"));
}

#[test]
fn format_speculative_summary_for_heartbeat_surfaces_low_noise_counts() {
    let now = 10_000_u64;
    let output = super::helpers::format_speculative_summary_for_heartbeat(
        &[SpeculativeOpportunity {
            id: "spec-q-1".to_string(),
            thread_id: Some("thread-spec-1".to_string()),
            source_kind: "intent_prediction".to_string(),
            action_kind: "repo_context_refresh".to_string(),
            confidence: 0.91,
            created_at_ms: now,
            expires_at_ms: now + 30_000,
            status: SpeculativeOpportunityStatus::Queued,
            summary: "prefetch repo context".to_string(),
        }],
        &[SpeculativeResult {
            opportunity_id: "spec-q-1".to_string(),
            action_kind: "repo_context_refresh".to_string(),
            thread_id: Some("thread-spec-1".to_string()),
            summary: "branch main; dirty=true".to_string(),
            artifact: serde_json::json!({"summary": "branch main; dirty=true"}),
            completed_at_ms: now - 500,
            expires_at_ms: now + 60_000,
            used_at_ms: None,
            precomputation_id: Some(7),
        }],
        now,
    )
    .expect("speculative summary should be present");

    assert!(output.contains("[speculative_execution]"));
    assert!(output.contains("LOW-PRIORITY INFORMATIONAL"));
    assert!(output.contains("queue_depth=1"));
    assert!(output.contains("cached_results=1"));
    assert!(output.contains("precomputed=1"));
    assert!(output.contains("threads=thread-spec-1"));
}

#[test]
fn format_speculative_summary_for_heartbeat_stays_low_noise_and_tracks_usage() {
    let now = 20_000_u64;
    let output = super::helpers::format_speculative_summary_for_heartbeat(
        &[
            SpeculativeOpportunity {
                id: "spec-q-1".to_string(),
                thread_id: Some("thread-a".to_string()),
                source_kind: "intent_prediction".to_string(),
                action_kind: "repo_context_refresh".to_string(),
                confidence: 0.91,
                created_at_ms: now,
                expires_at_ms: now + 30_000,
                status: SpeculativeOpportunityStatus::Queued,
                summary: "prefetch repo context".to_string(),
            },
            SpeculativeOpportunity {
                id: "spec-q-2".to_string(),
                thread_id: Some("thread-b".to_string()),
                source_kind: "system_outcome_foresight".to_string(),
                action_kind: "repo_context_refresh".to_string(),
                confidence: 0.88,
                created_at_ms: now,
                expires_at_ms: now + 30_000,
                status: SpeculativeOpportunityStatus::Running,
                summary: "prefetch second repo context".to_string(),
            },
        ],
        &[SpeculativeResult {
            opportunity_id: "spec-q-1".to_string(),
            action_kind: "repo_context_refresh".to_string(),
            thread_id: Some("thread-a".to_string()),
            summary: "branch main; dirty=true; speculative".to_string(),
            artifact: serde_json::json!({
                "summary": "branch main; dirty=true; speculative",
                "details": "long raw payload that should not be dumped into heartbeat"
            }),
            completed_at_ms: now - 100,
            expires_at_ms: now + 60_000,
            used_at_ms: Some(now - 50),
            precomputation_id: Some(11),
        }],
        now,
    )
    .expect("speculative summary should be present");

    assert!(output.contains("queue_depth=2"));
    assert!(output.contains("cached_results=1"));
    assert!(output.contains("used=1"));
    assert!(!output.contains("long raw payload"));
    assert!(!output.contains("artifact"));
}

#[test]
fn format_speculative_summary_for_heartbeat_returns_none_when_no_active_state() {
    let now = 30_000_u64;
    let output = super::helpers::format_speculative_summary_for_heartbeat(
        &[SpeculativeOpportunity {
            id: "expired-opportunity".to_string(),
            thread_id: Some("thread-old".to_string()),
            source_kind: "intent_prediction".to_string(),
            action_kind: "repo_context_refresh".to_string(),
            confidence: 0.9,
            created_at_ms: now - 10_000,
            expires_at_ms: now - 1,
            status: SpeculativeOpportunityStatus::Expired,
            summary: "expired".to_string(),
        }],
        &[SpeculativeResult {
            opportunity_id: "expired-opportunity".to_string(),
            action_kind: "repo_context_refresh".to_string(),
            thread_id: Some("thread-old".to_string()),
            summary: "expired cached result".to_string(),
            artifact: serde_json::json!({"summary": "expired cached result"}),
            completed_at_ms: now - 5_000,
            expires_at_ms: now - 1,
            used_at_ms: None,
            precomputation_id: None,
        }],
        now,
    );

    assert!(output.is_none());
}

#[test]
fn format_consolidation_forge_summary_surfaces_strategy_learning() {
    let output = super::helpers::format_consolidation_forge_summary(&ConsolidationResult {
        forge_ran: true,
        forge_traces_analyzed: 17,
        forge_patterns_detected: 3,
        forge_hints_generated: 2,
        forge_hints_auto_applied: 1,
        ..Default::default()
    })
    .expect("forge summary should be present when forge ran");

    assert!(output.contains("forge learned from 17 traces"));
    assert!(output.contains("3 pattern(s)"));
    assert!(output.contains("2 hint(s) generated"));
    assert!(output.contains("1 auto-applied"));
}

#[test]
fn format_consolidation_forge_summary_surfaces_logged_only_hints() {
    let output = super::helpers::format_consolidation_forge_summary(&ConsolidationResult {
        forge_ran: true,
        forge_traces_analyzed: 11,
        forge_patterns_detected: 2,
        forge_hints_generated: 3,
        forge_hints_auto_applied: 1,
        forge_hints_logged_only: 2,
        ..Default::default()
    })
    .expect("forge summary should be present when forge ran");

    assert!(
        output.contains("2 logged-only"),
        "unexpected forge summary: {output}"
    );
}

#[test]
fn format_consolidation_dream_summary_surfaces_what_the_system_considered_while_idle() {
    let output = super::helpers::format_consolidation_dream_summary(&ConsolidationResult {
        distillation_ran: true,
        distillation_threads_analyzed: 4,
        distillation_auto_applied: 2,
        forge_ran: true,
        forge_patterns_detected: 3,
        forge_hints_auto_applied: 1,
        facts_refined: 2,
        skills_promoted: 1,
        ..Default::default()
    })
    .expect("dream summary should be present when consolidation learned something");

    assert!(output.contains("what the system considered while idle"));
    assert!(output.contains("where better strategies might have changed outcomes"));
    assert!(output.contains("4 thread(s)"));
    assert!(output.contains("2 memory update(s)"));
    assert!(output.contains("3 recurring pattern(s)"));
}

#[test]
fn format_consolidation_dream_summary_surfaces_distillation_review_queue() {
    let output = super::helpers::format_consolidation_dream_summary(&ConsolidationResult {
        distillation_ran: true,
        distillation_threads_analyzed: 3,
        distillation_auto_applied: 1,
        distillation_queued_for_review: 2,
        ..Default::default()
    })
    .expect("dream summary should be present when distillation ran");

    assert!(
        output.contains("2 queued for review"),
        "unexpected dream summary: {output}"
    );
}

#[test]
fn format_consolidation_dream_summary_surfaces_counterfactual_strategy_hints() {
    let output = super::helpers::format_consolidation_dream_summary(&ConsolidationResult {
        distillation_ran: true,
        distillation_threads_analyzed: 2,
        distillation_auto_applied: 1,
        forge_ran: true,
        forge_hints_generated: 3,
        forge_hints_auto_applied: 1,
        forge_hints_logged_only: 2,
        ..Default::default()
    })
    .expect("dream summary should be present when forge generated strategy hints");

    assert!(
        output.contains("counterfactual strategy hint(s)"),
        "unexpected dream summary: {output}"
    );
    assert!(
        output.contains("3 counterfactual strategy hint(s) generated")
            && output.contains("1 auto-applied"),
        "unexpected dream summary: {output}"
    );
}

#[tokio::test]
async fn structured_heartbeat_keeps_consolidation_carryover_quiet_by_default() {
    let recorded_bodies = Arc::new(StdMutex::new(VecDeque::new()));
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.enabled = true;
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = "http://127.0.0.1:1/v1".to_string();
    config.model = "svarog-model".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.consolidation.enabled = true;
    config.consolidation.idle_threshold_secs = 0;
    config.heartbeat_checks.stale_todos_enabled = false;
    config.heartbeat_checks.stuck_goals_enabled = false;
    config.heartbeat_checks.unreplied_messages_enabled = false;
    config.heartbeat_checks.repo_changes_enabled = false;
    config.heartbeat_checks.plugin_auth_enabled = false;
    config.providers.insert(
        "custom-weles".to_string(),
        ProviderConfig {
            base_url: spawn_recording_heartbeat_server(recorded_bodies.clone()).await,
            model: "weles-model".to_string(),
            api_key: "test-key".to_string(),
            assistant_id: String::new(),
            auth_source: AuthSource::ApiKey,
            api_transport: ApiTransport::ChatCompletions,
            reasoning_effort: "medium".to_string(),
            context_window_tokens: 0,
            response_schema: None,
            stop_sequences: None,
            temperature: None,
            top_p: None,
            top_k: None,
            metadata: None,
            service_tier: None,
            container: None,
            inference_geo: None,
            cache_control: None,
            max_tokens: None,
            anthropic_tool_choice: None,
            output_effort: None,
            openrouter_provider_order: Vec::new(),
            openrouter_provider_ignore: Vec::new(),
            openrouter_allow_fallbacks: None,
            openrouter_response_cache_enabled: false,
            huggingface_provider: None,
        },
    );
    config.builtin_sub_agents.weles.provider = Some("custom-weles".to_string());
    config.builtin_sub_agents.weles.model = Some("weles-model".to_string());
    config.builtin_sub_agents.weles.reasoning_effort = Some("medium".to_string());

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let mut events = engine.subscribe();

    let weles_scope = crate::agent::agent_identity::WELES_AGENT_ID;
    crate::agent::ensure_memory_files_for_scope(engine.data_dir.as_path(), weles_scope)
        .await
        .expect("seed memory files for weles scope");
    crate::agent::ensure_memory_files_for_scope(
        engine.data_dir.as_path(),
        crate::agent::agent_identity::MAIN_AGENT_ID,
    )
    .await
    .expect("seed memory files for main scope");
    let weles_memory_path =
        crate::agent::task_prompt::memory_paths_for_scope(engine.data_dir.as_path(), weles_scope)
            .memory_path;
    let main_memory_path = crate::agent::task_prompt::memory_paths_for_scope(
        engine.data_dir.as_path(),
        crate::agent::agent_identity::MAIN_AGENT_ID,
    )
    .memory_path;
    tokio::fs::write(&weles_memory_path, "# Memory\n")
        .await
        .expect("shrink weles memory file for deterministic idle-learning test");
    tokio::fs::write(&main_memory_path, "# Memory\n")
        .await
        .expect("shrink main memory file for deterministic idle-learning test");
    let now = now_millis();
    let metrics_json = serde_json::json!({
        "total_duration_ms": 45_000,
        "step_count": 2,
        "success_rate": 0.5,
        "operator_revisions": 1,
        "exit_code": 1,
    })
    .to_string();

    for idx in 0..3u64 {
        let started_at = now.saturating_sub(1_000 + idx);
        let completed_at = started_at + 100;
        engine
            .history
            .insert_execution_trace(
                &format!("heartbeat-dream-trace-{idx}"),
                None,
                None,
                Some(&format!("task-{idx}")),
                "coding",
                "success",
                Some(0.6),
                "[\"bash_command\",\"read_file\"]",
                &metrics_json,
                45_000,
                120,
                weles_scope,
                started_at,
                completed_at,
                completed_at,
            )
            .await
            .expect("seed execution trace");
    }

    engine
        .run_structured_heartbeat_adaptive(0)
        .await
        .expect("structured heartbeat should succeed with quiet carryover");

    let payload = engine
        .show_dreams_payload(5)
        .await
        .expect("show_dreams should remain the inspection surface for carryover");
    assert_eq!(
        payload["carryover"]["inspection_tool"].as_str(),
        Some("show_dreams")
    );
    assert!(payload["carryover"]["persisted_event_count"]
        .as_u64()
        .is_some_and(|value| value >= 1));

    let mut workflow_notice_kinds = Vec::new();
    loop {
        match timeout(Duration::from_millis(50), events.recv()).await {
            Ok(Ok(AgentEvent::WorkflowNotice { kind, .. })) => workflow_notice_kinds.push(kind),
            Ok(Ok(_)) => {}
            _ => break,
        }
    }

    assert!(
        !workflow_notice_kinds
            .iter()
            .any(|kind| kind == "dream" || kind == "forge"),
        "carryover should stay quiet by default; saw workflow notices: {workflow_notice_kinds:?}"
    );
}

#[test]
fn peak_activity_hour_in_peak_hours_list() {
    let smoothed: HashMap<u8, f64> = HashMap::new();
    assert!(is_peak_activity_hour(9, &[9, 10, 14], &smoothed, 2.0));
}

#[test]
fn peak_activity_hour_above_ema_threshold() {
    let mut smoothed: HashMap<u8, f64> = HashMap::new();
    smoothed.insert(15, 5.0);
    assert!(is_peak_activity_hour(15, &[], &smoothed, 2.0));
}

#[test]
fn peak_activity_hour_below_threshold_and_not_in_list() {
    let mut smoothed: HashMap<u8, f64> = HashMap::new();
    smoothed.insert(3, 1.0);
    assert!(!is_peak_activity_hour(3, &[9, 10], &smoothed, 2.0));
}

#[test]
fn should_run_check_weight_one_always_runs() {
    assert!(should_run_check(1.0, 0));
    assert!(should_run_check(1.0, 1));
    assert!(should_run_check(1.0, 99));
}

#[test]
fn should_run_check_weight_quarter_every_fourth_cycle() {
    assert!(should_run_check(0.25, 4));
    assert!(should_run_check(0.25, 8));
    assert!(should_run_check(0.25, 0));
}

#[test]
fn should_run_check_weight_quarter_skips_other_cycles() {
    assert!(!should_run_check(0.25, 1));
    assert!(!should_run_check(0.25, 3));
}

#[test]
fn should_run_check_weight_zero_never_runs() {
    assert!(!should_run_check(0.0, 0));
    assert!(!should_run_check(0.0, 1));
    assert!(!should_run_check(0.0, 100));
}

#[test]
fn compute_check_priority_zero_dismissals_returns_one() {
    let result = compute_check_priority(0, 0, 0, 0, 0.1, 0.1);
    assert!((result - 1.0).abs() < f64::EPSILON);
}

#[test]
fn compute_check_priority_many_dismissals_clamped_minimum() {
    let result = compute_check_priority(100, 100, 100, 0, 0.1, 0.1);
    assert!((result - 0.1).abs() < f64::EPSILON);
}

#[test]
fn compute_check_priority_recovery_partially_restores() {
    let result = compute_check_priority(5, 0, 0, 3, 0.1, 0.1);
    assert!((result - 0.8).abs() < f64::EPSILON);
}

#[test]
fn priority_floor_never_below_point_one() {
    let result = compute_check_priority(1000, 1000, 1000, 0, 1.0, 0.0);
    assert!((result - 0.1).abs() < f64::EPSILON);
}

#[tokio::test]
async fn heartbeat_weles_health_marks_review_unavailable_as_degraded() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.extra.insert(
        "weles_review_available".to_string(),
        serde_json::Value::Bool(false),
    );

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let mut events = engine.subscribe();

    let status = engine.refresh_weles_health_from_heartbeat(1_234).await;

    assert_eq!(status.state, WelesHealthState::Degraded);
    assert_eq!(status.checked_at, 1_234);
    assert!(status
        .reason
        .as_deref()
        .is_some_and(|reason| reason.contains("review unavailable")));

    let event = timeout(Duration::from_millis(250), events.recv())
        .await
        .expect("weles health event should arrive")
        .expect("weles health event should deserialize");
    match event {
        AgentEvent::WelesHealthUpdate {
            state,
            reason,
            checked_at,
        } => {
            assert_eq!(state, WelesHealthState::Degraded);
            assert_eq!(checked_at, 1_234);
            assert!(reason
                .as_deref()
                .is_some_and(|value| value.contains("review unavailable")));
        }
        other => panic!("expected weles health update, got {other:?}"),
    }
}

#[tokio::test]
async fn structured_heartbeat_synthesis_uses_weles_model_without_tools_or_agent_runtime() {
    let recorded_bodies = Arc::new(StdMutex::new(VecDeque::new()));
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = "http://127.0.0.1:1/v1".to_string();
    config.model = "svarog-model".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.consolidation.enabled = false;
    config.heartbeat_checks.stale_todos_enabled = false;
    config.heartbeat_checks.stuck_goals_enabled = false;
    config.heartbeat_checks.unreplied_messages_enabled = false;
    config.heartbeat_checks.repo_changes_enabled = false;
    config.heartbeat_checks.plugin_auth_enabled = false;
    config.providers.insert(
        "custom-weles".to_string(),
        ProviderConfig {
            base_url: spawn_recording_heartbeat_server(recorded_bodies.clone()).await,
            model: "weles-model".to_string(),
            api_key: "test-key".to_string(),
            assistant_id: String::new(),
            auth_source: AuthSource::ApiKey,
            api_transport: ApiTransport::ChatCompletions,
            reasoning_effort: "medium".to_string(),
            context_window_tokens: 0,
            response_schema: None,
            stop_sequences: None,
            temperature: None,
            top_p: None,
            top_k: None,
            metadata: None,
            service_tier: None,
            container: None,
            inference_geo: None,
            cache_control: None,
            max_tokens: None,
            anthropic_tool_choice: None,
            output_effort: None,
            openrouter_provider_order: Vec::new(),
            openrouter_provider_ignore: Vec::new(),
            openrouter_allow_fallbacks: None,
            openrouter_response_cache_enabled: false,
            huggingface_provider: None,
        },
    );
    config.builtin_sub_agents.weles.provider = Some("custom-weles".to_string());
    config.builtin_sub_agents.weles.model = Some("weles-model".to_string());
    config.builtin_sub_agents.weles.reasoning_effort = Some("medium".to_string());

    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    engine
        .run_structured_heartbeat_adaptive(0)
        .await
        .expect("structured heartbeat synthesis should succeed");

    let recorded = recorded_bodies
        .lock()
        .expect("lock recorded heartbeat requests");
    let body = recorded
        .front()
        .expect("expected heartbeat synthesis request to hit Weles provider");
    assert!(
        body.contains("\"model\":\"weles-model\""),
        "expected heartbeat synthesis to use Weles model, body was: {body}"
    );
    assert!(
        !body.contains("svarog-model"),
        "heartbeat synthesis should not fall back to main-agent model, body was: {body}"
    );
    assert!(
        !body.contains("\"tools\""),
        "heartbeat synthesis must not offer tools, or the model starts acting on the data instead of summarizing it; body was: {body}"
    );
    assert!(
        !body.contains("You are Weles in zorai."),
        "heartbeat synthesis must not run as a full Weles agent turn, body was: {body}"
    );
    assert_eq!(
        recorded.len(),
        1,
        "one heartbeat is exactly one model request"
    );
    assert!(
        engine.list_threads().await.is_empty(),
        "heartbeat synthesis must not create a thread"
    );
}
