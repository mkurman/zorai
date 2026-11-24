use super::tests_part2_support::*;
use super::*;
use crate::agent::types::SubAgentDefinition;
#[tokio::test]
async fn gateway_send_results_complete_waiters_and_update_last_response_state() {
    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    let mut conn = spawn_test_connection_with_config(config).await;
    let correlation_id = register_gateway(&mut conn).await;
    acknowledge_gateway_bootstrap(&mut conn, correlation_id).await;
    conn.agent.init_gateway().await;

    let agent = conn.agent.clone();
    let send_task = tokio::spawn(async move {
        agent
            .request_gateway_send(GatewaySendRequest {
                correlation_id: "send-1".to_string(),
                platform: "slack".to_string(),
                channel_id: "C123".to_string(),
                thread_id: Some("1712345678.000100".to_string()),
                content: "hello".to_string(),
            })
            .await
    });

    let request = match conn.recv().await {
        DaemonMessage::GatewaySendRequest { request } => request,
        other => panic!("expected GatewaySendRequest, got {other:?}"),
    };
    assert_eq!(request.correlation_id, "send-1");
    assert_eq!(request.channel_id, "C123");

    conn.framed
        .send(ClientMessage::GatewaySendResult {
            result: zorai_protocol::GatewaySendResult {
                correlation_id: request.correlation_id.clone(),
                platform: "slack".to_string(),
                channel_id: "C123".to_string(),
                requested_channel_id: Some("C123".to_string()),
                delivery_id: Some("1712345678.000200".to_string()),
                ok: true,
                error: None,
                completed_at_ms: 1234,
            },
        })
        .await
        .expect("send gateway result");

    let result = send_task
        .await
        .expect("join send task")
        .expect("gateway send should complete");
    assert!(result.ok);

    let gw_guard = conn.agent.gateway_state.lock().await;
    let gw = gw_guard.as_ref().expect("gateway state should exist");
    assert!(gw.last_response_at.contains_key("Slack:C123"));
    drop(gw_guard);

    conn.shutdown().await;
}

#[tokio::test]
async fn agent_set_sub_agent_returns_canonical_weles_payload_after_update() {
    let mut conn = spawn_test_connection().await;

    let weles = conn
        .agent
        .list_sub_agents()
        .await
        .into_iter()
        .find(|entry| entry.id == "weles_builtin")
        .expect("missing builtin weles entry");
    let mut updated = weles.clone();
    updated.reasoning_effort = Some("high".to_string());

    conn.framed
        .send(ClientMessage::AgentSetSubAgent {
            sub_agent_json: serde_json::to_string(&updated).expect("serialize subagent"),
        })
        .await
        .expect("send set subagent");

    match conn.recv_with_timeout(Duration::from_secs(2)).await {
        DaemonMessage::OperationAccepted { kind, .. } => {
            assert_eq!(kind, "set_sub_agent");
        }
        other => panic!("expected set-subagent operation acceptance, got {other:?}"),
    }

    match conn.recv().await {
        DaemonMessage::AgentSubAgentUpdated { sub_agent_json } => {
            let returned: SubAgentDefinition =
                serde_json::from_str(&sub_agent_json).expect("parse returned subagent json");
            assert_eq!(returned.id, "weles_builtin");
            assert_eq!(returned.name, "WELES");
            assert!(returned.builtin);
            assert!(returned.immutable_identity);
            assert!(!returned.disable_allowed);
            assert!(!returned.delete_allowed);
            assert_eq!(returned.reasoning_effort.as_deref(), Some("high"));

            let effective = conn
                .agent
                .list_sub_agents()
                .await
                .into_iter()
                .find(|entry| entry.id == "weles_builtin")
                .expect("missing persisted builtin weles entry");
            assert_eq!(returned.id, effective.id);
            assert_eq!(returned.name, effective.name);
            assert_eq!(returned.provider, effective.provider);
            assert_eq!(returned.model, effective.model);
            assert_eq!(returned.system_prompt, effective.system_prompt);
            assert_eq!(returned.reasoning_effort, effective.reasoning_effort);
        }
        other => panic!("expected AgentSubAgentUpdated, got {other:?}"),
    }

    conn.shutdown().await;
}

#[tokio::test]
async fn agent_set_sub_agent_async_request_returns_operation_acceptance() {
    let mut conn = spawn_test_connection().await;
    declare_async_command_capability(&mut conn).await;

    let weles = conn
        .agent
        .list_sub_agents()
        .await
        .into_iter()
        .find(|entry| entry.id == "weles_builtin")
        .expect("missing builtin weles entry");
    let mut updated = weles;
    updated.reasoning_effort = Some("high".to_string());

    conn.framed
        .send(ClientMessage::AgentSetSubAgent {
            sub_agent_json: serde_json::to_string(&updated).expect("serialize subagent"),
        })
        .await
        .expect("send set subagent");

    let operation_id = match conn.recv_with_timeout(Duration::from_millis(250)).await {
        DaemonMessage::OperationAccepted {
            operation_id, kind, ..
        } => {
            assert_eq!(kind, "set_sub_agent");
            operation_id
        }
        other => panic!("expected set-subagent operation acceptance, got {other:?}"),
    };

    conn.framed
        .send(ClientMessage::Ping)
        .await
        .expect("send ping while set-subagent is active");

    let pong_received = timeout(Duration::from_millis(250), async {
        loop {
            match conn.recv_with_timeout(Duration::from_millis(250)).await {
                DaemonMessage::Pong => return true,
                DaemonMessage::AgentSubAgentUpdated { .. } => continue,
                other => {
                    panic!("expected Pong while set-subagent runs in background, got {other:?}")
                }
            }
        }
    })
    .await
    .unwrap_or(false);

    assert!(
        pong_received,
        "ping should not be blocked behind set-subagent"
    );

    conn.framed
        .send(ClientMessage::AgentGetOperationStatus { operation_id })
        .await
        .expect("query set-subagent operation status");

    match conn.recv_with_timeout(Duration::from_secs(1)).await {
        DaemonMessage::OperationStatus { snapshot } => {
            assert_eq!(snapshot.kind, "set_sub_agent");
            assert!(matches!(
                snapshot.state,
                zorai_protocol::OperationLifecycleState::Accepted
                    | zorai_protocol::OperationLifecycleState::Started
                    | zorai_protocol::OperationLifecycleState::Completed
            ));
        }
        other => panic!("expected set-subagent status snapshot, got {other:?}"),
    }

    match conn.recv_with_timeout(Duration::from_secs(1)).await {
        DaemonMessage::AgentSubAgentUpdated { sub_agent_json } => {
            let returned: SubAgentDefinition =
                serde_json::from_str(&sub_agent_json).expect("parse returned subagent json");
            assert_eq!(returned.id, "weles_builtin");
            assert_eq!(returned.reasoning_effort.as_deref(), Some("high"));
        }
        other => panic!("expected set-subagent result, got {other:?}"),
    }

    conn.shutdown().await;
}

#[tokio::test]
async fn agent_remove_sub_agent_async_request_returns_operation_acceptance() {
    let mut conn = spawn_test_connection().await;
    declare_async_command_capability(&mut conn).await;

    let removable = test_user_sub_agent("reviewer_async_remove", "Reviewer Async Remove");
    conn.agent
        .set_sub_agent(removable)
        .await
        .expect("seed removable subagent");

    conn.framed
        .send(ClientMessage::AgentRemoveSubAgent {
            sub_agent_id: "reviewer_async_remove".to_string(),
        })
        .await
        .expect("send remove subagent");

    let operation_id = match conn.recv_with_timeout(Duration::from_millis(250)).await {
        DaemonMessage::OperationAccepted {
            operation_id, kind, ..
        } => {
            assert_eq!(kind, "remove_sub_agent");
            operation_id
        }
        other => panic!("expected remove-subagent operation acceptance, got {other:?}"),
    };

    conn.framed
        .send(ClientMessage::Ping)
        .await
        .expect("send ping while remove-subagent is active");

    let pong_received = timeout(Duration::from_millis(250), async {
        loop {
            match conn.recv_with_timeout(Duration::from_millis(250)).await {
                DaemonMessage::Pong => return true,
                DaemonMessage::AgentSubAgentRemoved { .. } => continue,
                other => {
                    panic!("expected Pong while remove-subagent runs in background, got {other:?}")
                }
            }
        }
    })
    .await
    .unwrap_or(false);

    assert!(
        pong_received,
        "ping should not be blocked behind remove-subagent"
    );

    conn.framed
        .send(ClientMessage::AgentGetOperationStatus { operation_id })
        .await
        .expect("query remove-subagent operation status");

    match conn.recv_with_timeout(Duration::from_secs(1)).await {
        DaemonMessage::OperationStatus { snapshot } => {
            assert_eq!(snapshot.kind, "remove_sub_agent");
            assert!(matches!(
                snapshot.state,
                zorai_protocol::OperationLifecycleState::Accepted
                    | zorai_protocol::OperationLifecycleState::Started
                    | zorai_protocol::OperationLifecycleState::Completed
            ));
        }
        other => panic!("expected remove-subagent status snapshot, got {other:?}"),
    }

    match conn.recv_with_timeout(Duration::from_secs(1)).await {
        DaemonMessage::AgentSubAgentRemoved { sub_agent_id } => {
            assert_eq!(sub_agent_id, "reviewer_async_remove");
        }
        other => panic!("expected remove-subagent result, got {other:?}"),
    }

    assert!(conn
        .agent
        .get_sub_agent("reviewer_async_remove")
        .await
        .is_none());

    conn.shutdown().await;
}

#[tokio::test]
async fn agent_set_sub_agent_config_accepts_minimal_weles_payload_via_server_canonicalization() {
    let mut conn = spawn_test_connection().await;

    let mut config = conn.agent.get_config().await;
    config.provider = "openai".to_string();
    config.model = "gpt-5.4-mini".to_string();
    config.system_prompt = "Main prompt".to_string();
    conn.agent.set_config(config).await;

    let minimal = SubAgentDefinition {
        generation_settings: None,
        base_url: None,
        claude_permission_mode: None,
        id: "weles_builtin".to_string(),
        name: "WELES".to_string(),
        provider: "anthropic".to_string(),
        model: "claude-sonnet".to_string(),
        role: Some("governance".to_string()),
        system_prompt: Some("Escalated WELES prompt".to_string()),
        tool_whitelist: None,
        tool_blacklist: None,
        context_budget_tokens: None,
        context_window_tokens: None,
        max_duration_secs: None,
        supervisor_config: None,
        enabled: true,
        builtin: false,
        immutable_identity: false,
        disable_allowed: true,
        delete_allowed: true,
        protected_reason: None,
        reasoning_effort: Some("high".to_string()),
        api_transport: None,
        openrouter_provider_order: Vec::new(),
        openrouter_provider_ignore: Vec::new(),
        openrouter_allow_fallbacks: None,
        huggingface_provider: None,
        created_at: 0,
    };

    conn.framed
        .send(ClientMessage::AgentSetSubAgent {
            sub_agent_json: serde_json::to_string(&minimal).expect("serialize subagent"),
        })
        .await
        .expect("send set subagent");

    match conn.recv_with_timeout(Duration::from_secs(2)).await {
        DaemonMessage::OperationAccepted { kind, .. } => {
            assert_eq!(kind, "set_sub_agent");
        }
        other => panic!("expected set-subagent operation acceptance, got {other:?}"),
    }

    match conn.recv().await {
        DaemonMessage::AgentSubAgentUpdated { sub_agent_json } => {
            let returned: SubAgentDefinition =
                serde_json::from_str(&sub_agent_json).expect("parse returned subagent json");
            assert_eq!(returned.id, "weles_builtin");
            assert_eq!(returned.name, "WELES");
            assert!(returned.builtin);
            assert!(returned.immutable_identity);
            assert!(!returned.disable_allowed);
            assert!(!returned.delete_allowed);
            assert_eq!(
                returned.protected_reason.as_deref(),
                Some("Daemon-owned WELES registry entry")
            );
            assert_eq!(returned.provider, "anthropic");
            assert_eq!(returned.model, "claude-sonnet");
            assert_eq!(
                returned.system_prompt.as_deref(),
                Some("Escalated WELES prompt")
            );
            assert_eq!(returned.reasoning_effort.as_deref(), Some("high"));
        }
        other => panic!("expected AgentSubAgentUpdated, got {other:?}"),
    }

    let stored = conn.agent.get_config().await;
    assert_eq!(
        stored.builtin_sub_agents.weles.provider.as_deref(),
        Some("anthropic")
    );
    assert_eq!(
        stored.builtin_sub_agents.weles.model.as_deref(),
        Some("claude-sonnet")
    );
    assert_eq!(
        stored.builtin_sub_agents.weles.system_prompt.as_deref(),
        Some("Escalated WELES prompt")
    );
    assert_eq!(
        stored.builtin_sub_agents.weles.reasoning_effort.as_deref(),
        Some("high")
    );

    conn.shutdown().await;
}

#[tokio::test]
async fn agent_set_sub_agent_request_without_declared_capability_still_returns_operation_acceptance(
) {
    let mut conn = spawn_test_connection().await;

    let weles = conn
        .agent
        .list_sub_agents()
        .await
        .into_iter()
        .find(|entry| entry.id == "weles_builtin")
        .expect("missing builtin weles entry");
    let mut updated = weles;
    updated.reasoning_effort = Some("high".to_string());

    conn.framed
        .send(ClientMessage::AgentSetSubAgent {
            sub_agent_json: serde_json::to_string(&updated).expect("serialize subagent"),
        })
        .await
        .expect("send set subagent");

    let operation_id = match conn.recv_with_timeout(Duration::from_secs(2)).await {
        DaemonMessage::OperationAccepted {
            operation_id, kind, ..
        } => {
            assert_eq!(kind, "set_sub_agent");
            operation_id
        }
        other => panic!("expected set-subagent operation acceptance, got {other:?}"),
    };

    match conn.recv_with_timeout(Duration::from_secs(2)).await {
        DaemonMessage::AgentSubAgentUpdated { sub_agent_json } => {
            let returned: SubAgentDefinition =
                serde_json::from_str(&sub_agent_json).expect("parse returned subagent json");
            assert_eq!(returned.id, "weles_builtin");
            assert_eq!(returned.reasoning_effort.as_deref(), Some("high"));
        }
        other => panic!("expected set-subagent result, got {other:?}"),
    }

    conn.framed
        .send(ClientMessage::AgentGetOperationStatus { operation_id })
        .await
        .expect("query set-subagent operation status");

    match conn.recv_with_timeout(Duration::from_secs(2)).await {
        DaemonMessage::OperationStatus { snapshot } => {
            assert_eq!(snapshot.kind, "set_sub_agent");
        }
        other => panic!("expected set-subagent status snapshot, got {other:?}"),
    }

    conn.shutdown().await;
}

#[tokio::test]
async fn agent_remove_sub_agent_request_without_declared_capability_still_returns_operation_acceptance(
) {
    let mut conn = spawn_test_connection().await;

    let removable = test_user_sub_agent("reviewer_remove_legacy", "Reviewer Remove Legacy");
    conn.agent
        .set_sub_agent(removable)
        .await
        .expect("seed removable subagent");

    conn.framed
        .send(ClientMessage::AgentRemoveSubAgent {
            sub_agent_id: "reviewer_remove_legacy".to_string(),
        })
        .await
        .expect("send remove subagent");

    let operation_id = match conn.recv_with_timeout(Duration::from_secs(2)).await {
        DaemonMessage::OperationAccepted {
            operation_id, kind, ..
        } => {
            assert_eq!(kind, "remove_sub_agent");
            operation_id
        }
        other => panic!("expected remove-subagent operation acceptance, got {other:?}"),
    };

    match conn.recv_with_timeout(Duration::from_secs(2)).await {
        DaemonMessage::AgentSubAgentRemoved { sub_agent_id } => {
            assert_eq!(sub_agent_id, "reviewer_remove_legacy");
        }
        other => panic!("expected remove-subagent result, got {other:?}"),
    }

    conn.framed
        .send(ClientMessage::AgentGetOperationStatus { operation_id })
        .await
        .expect("query remove-subagent operation status");

    match conn.recv_with_timeout(Duration::from_secs(2)).await {
        DaemonMessage::OperationStatus { snapshot } => {
            assert_eq!(snapshot.kind, "remove_sub_agent");
        }
        other => panic!("expected remove-subagent status snapshot, got {other:?}"),
    }

    conn.shutdown().await;
}
