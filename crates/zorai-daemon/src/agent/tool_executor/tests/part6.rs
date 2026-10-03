use super::super::*;
use super::super::{execute_gateway_message, execute_tool};
use crate::agent::{
    types::{AgentConfig, AgentEvent, ToolCall, ToolFunction},
    AgentEngine, StreamCancellationEntry, StreamProgressKind,
};
use crate::session_manager::SessionManager;
use std::sync::{Arc, Mutex};
use tempfile::tempdir;
use tokio::sync::broadcast;
use tokio::time::{timeout, Duration};
use tokio_util::sync::CancellationToken;
use zorai_protocol::tool_names;
use zorai_protocol::{DaemonMessage, GatewaySendResult};

#[cfg(windows)]
use std::os::windows::process::ExitStatusExt;

#[tokio::test]
async fn send_discord_message_uses_canonical_dm_reply_context_for_user_targets() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    engine.init_gateway().await;
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    engine.set_gateway_ipc_sender(Some(tx)).await;

    {
        let mut gw_guard = engine.gateway_state.lock().await;
        let gw = gw_guard.as_mut().expect("gateway state should exist");
        gw.discord_dm_channels_by_user
            .insert("user:123456789".to_string(), "DM123".to_string());
        gw.reply_contexts.insert(
            "Discord:DM123".to_string(),
            crate::agent::gateway::ThreadContext {
                discord_message_id: Some("987654321".to_string()),
                ..Default::default()
            },
        );
    }

    let send_engine = engine.clone();
    let send_task = tokio::spawn(async move {
        execute_gateway_message(
            "send_discord_message",
            &serde_json::json!({
                "user_id": "123456789",
                "message": "discord reply"
            }),
            &send_engine,
            &reqwest::Client::new(),
        )
        .await
    });

    let request = match timeout(Duration::from_millis(250), rx.recv())
        .await
        .expect("gateway send request should be emitted")
        .expect("gateway send request should exist")
    {
        DaemonMessage::GatewaySendRequest { request } => request,
        other => panic!("expected GatewaySendRequest, got {other:?}"),
    };
    assert_eq!(request.platform, "discord");
    assert_eq!(request.channel_id, "user:123456789");
    assert_eq!(request.thread_id.as_deref(), Some("987654321"));

    engine
        .complete_gateway_send_result(GatewaySendResult {
            correlation_id: request.correlation_id.clone(),
            platform: "discord".to_string(),
            channel_id: "DM123".to_string(),
            requested_channel_id: Some("user:123456789".to_string()),
            delivery_id: Some("delivery-2".to_string()),
            ok: true,
            error: None,
            completed_at_ms: 1,
        })
        .await;

    let result = send_task
        .await
        .expect("send task should join")
        .expect("send should succeed");
    assert_eq!(result, "Discord message sent to user:123456789");
}

async fn spawn_scripted_tool_call_server_for_tool_executor(
    script: Vec<Option<(String, String)>>,
) -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind scripted tool call server");
    let addr = listener
        .local_addr()
        .expect("scripted tool call server local addr");
    let script = Arc::new(script);
    let next_response = Arc::new(std::sync::atomic::AtomicUsize::new(0));

    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let script = Arc::clone(&script);
            let next_response = Arc::clone(&next_response);
            tokio::spawn(async move {
                let mut buffer = vec![0u8; 65536];
                let read = socket
                    .read(&mut buffer)
                    .await
                    .expect("read scripted tool call request");
                let request = String::from_utf8_lossy(&buffer[..read]).to_string();
                let body = request
                    .split("\r\n\r\n")
                    .nth(1)
                    .unwrap_or_default()
                    .to_string();
                let response = if body.contains("internal_hidden") {
                    concat!(
	                        "HTTP/1.1 200 OK\r\n",
	                        "content-type: text/event-stream\r\n",
	                        "cache-control: no-cache\r\n",
	                        "connection: close\r\n",
	                        "\r\n",
	                        "data: {\"choices\":[{\"delta\":{\"content\":\"Acknowledged.\"}}]}\n\n",
	                        "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":3}}\n\n",
	                        "data: [DONE]\n\n"
	                    )
	                    .to_string()
                } else {
                    let index = next_response.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    match script.get(index).and_then(|entry| entry.as_ref()) {
	                        Some((tool_name, arguments)) => {
	                            let chunk = serde_json::json!({
	                                "choices": [{
	                                    "delta": {
	                                        "tool_calls": [{
	                                            "index": 0,
	                                            "id": format!("call_scripted_{index}"),
	                                            "function": {
	                                                "name": tool_name,
	                                                "arguments": arguments,
	                                            }
	                                        }]
	                                    }
	                                }],
	                                "usage": {
	                                    "prompt_tokens": 7,
	                                    "completion_tokens": 3
	                                }
	                            })
	                            .to_string();
	                            format!(
	                                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncache-control: no-cache\r\nconnection: close\r\n\r\ndata: {chunk}\n\ndata: [DONE]\n\n",
	                                chunk = chunk
	                            )
	                        }
	                        None => concat!(
	                            "HTTP/1.1 200 OK\r\n",
	                            "content-type: text/event-stream\r\n",
	                            "cache-control: no-cache\r\n",
	                            "connection: close\r\n",
	                            "\r\n",
	                            "data: {\"choices\":[{\"delta\":{\"content\":\"Acknowledged.\"}}]}\n\n",
	                            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":3}}\n\n",
	                            "data: [DONE]\n\n"
	                        )
	                        .to_string(),
	                    }
                };
                socket
                    .write_all(response.as_bytes())
                    .await
                    .expect("write scripted tool call response");
            });
        }
    });

    format!("http://{addr}/v1")
}

#[test]
fn visible_thread_continuation_chain_does_not_overflow_small_worker_stack() {
    let runtime_stack_size = 8 * 1024 * 1024;
    let join = std::thread::Builder::new()
        .name("continuation-stack-regression".to_string())
        .stack_size(runtime_stack_size)
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .enable_all()
                .thread_stack_size(runtime_stack_size)
                .build()
                .expect("build runtime");
            runtime.block_on(async {
                let chain_len = 96usize;
                let mut script = Vec::with_capacity((chain_len - 1) * 2);
                for index in 0..(chain_len - 1) {
                    let target = if index % 2 == 0 { "svarog" } else { "weles" };
                    script.push(Some((
                        "message_agent".to_string(),
                        serde_json::json!({
                            "target": target,
                            "message": format!("continue visible chain step {}", index + 1),
                            "request_visible_thread_continuation": true
                        })
                        .to_string(),
                    )));
                    script.push(None);
                }

                let root = tempdir().expect("tempdir should succeed");
                let manager = SessionManager::new_test(root.path()).await;
                let mut config = AgentConfig::default();
                config.provider = "openai".to_string();
                config.base_url = spawn_scripted_tool_call_server_for_tool_executor(script).await;
                config.model = "gpt-4o-mini".to_string();
                config.api_key = "test-key".to_string();
                config.api_transport = crate::agent::types::ApiTransport::ChatCompletions;
                config.auto_retry = false;
                config.max_retries = 0;
                config.max_tool_loops = 1;
                let engine = AgentEngine::new_test(manager.clone(), config, root.path()).await;
                let (event_tx, _) = broadcast::channel(8);
                let thread_id = "thread-visible-continuation-recursion";

                {
                    let mut threads = engine.threads.write().await;
                    threads.insert(
                        thread_id.to_string(),
                        crate::agent::types::AgentThread {
                            id: thread_id.to_string(),
                            agent_name: Some(
                                crate::agent::agent_identity::MAIN_AGENT_NAME.to_string(),
                            ),
                            title: "Visible continuation recursion".to_string(),
                            messages: vec![crate::agent::types::AgentMessage::user(
                                "Keep the visible thread moving forward",
                                1,
                            )],
                            pinned: false,
                            upstream_thread_id: None,
                            upstream_transport: None,
                            upstream_provider: None,
                            upstream_model: None,
                            upstream_assistant_id: None,
                            total_input_tokens: 0,
                            total_output_tokens: 0,
                            created_at: 1,
                            updated_at: 1,
                        },
                    );
                }

                let tool_call = ToolCall::with_default_weles_review(
                    "tool-message-agent-visible-recursion".to_string(),
                    ToolFunction {
                        name: "message_agent".to_string(),
                        arguments: serde_json::json!({
                            "target": "weles",
                            "message": "Start the visible continuation chain.",
                            "request_visible_thread_continuation": true
                        })
                        .to_string(),
                    },
                );

                let result = execute_tool(
                    &tool_call,
                    &engine,
                    thread_id,
                    None,
                    &manager,
                    None,
                    &event_tx,
                    root.path(),
                    &engine.http_client,
                    None,
                )
                .await;

                assert!(
                    !result.is_error,
                    "initial visible continuation request should succeed: {}",
                    result.content
                );

                engine
                    .flush_deferred_visible_thread_continuations(thread_id)
                    .await
                    .expect("visible continuation chain should flush");

                let assistant_count = engine
                    .threads
                    .read()
                    .await
                    .get(thread_id)
                    .expect("visible thread should exist")
                    .messages
                    .iter()
                    .filter(|message| message.role == crate::agent::MessageRole::Assistant)
                    .count();
                assert!(
                    assistant_count >= 1,
                    "expected at least one visible assistant turn, got {assistant_count}"
                );
            });
        })
        .expect("spawn regression thread");

    join.join()
        .expect("continuation chain should complete without stack overflow");
}

#[tokio::test]
async fn compaction_trims_participant_playground_threads_without_dropping_latest_artifact() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.max_context_messages = 4;
    config.keep_recent_on_compact = 2;
    let assistant_message = |content: &str, timestamp: u64| crate::agent::types::AgentMessage {
        id: crate::agent::types::generate_message_id(),
        role: crate::agent::MessageRole::Assistant,
        content: content.to_string(),
        content_blocks: Vec::new(),
        tool_calls: None,
        tool_call_id: None,
        tool_name: None,
        tool_arguments: None,
        tool_status: None,
        weles_review: None,
        input_tokens: 0,
        output_tokens: 0,
        cost: None,
        provider: None,
        model: None,
        api_transport: None,
        response_id: None,
        upstream_message: None,
        provider_final_result: None,
        author_agent_id: None,
        author_agent_name: None,
        reasoning: None,
        message_kind: crate::agent::types::AgentMessageKind::Normal,
        compaction_strategy: None,
        compaction_payload: None,
        offloaded_payload_id: None,
        tool_output_preview_path: None,
        structural_refs: Vec::new(),
        pinned_for_compaction: false,
        timestamp,
        feedback: None,
    };
    let compaction_artifact_message =
        |content: &str, timestamp: u64| crate::agent::types::AgentMessage {
            id: crate::agent::types::generate_message_id(),
            role: crate::agent::MessageRole::Assistant,
            content: content.to_string(),
            content_blocks: Vec::new(),
            tool_calls: None,
            tool_call_id: None,
            tool_name: None,
            tool_arguments: None,
            tool_status: None,
            weles_review: None,
            input_tokens: 0,
            output_tokens: 0,
            cost: None,
            provider: None,
            model: None,
            api_transport: None,
            response_id: None,
            upstream_message: None,
            provider_final_result: None,
            author_agent_id: None,
            author_agent_name: None,
            reasoning: None,
            message_kind: crate::agent::types::AgentMessageKind::CompactionArtifact,
            compaction_strategy: Some(crate::agent::types::CompactionStrategy::Heuristic),
            compaction_payload: Some(content.to_string()),
            offloaded_payload_id: None,
            tool_output_preview_path: None,
            structural_refs: Vec::new(),
            pinned_for_compaction: false,
            timestamp,
            feedback: None,
        };
    let provider_config = {
        let engine = AgentEngine::new_test(manager.clone(), config.clone(), root.path()).await;
        engine
            .resolve_provider_config(&config)
            .expect("provider config")
    };
    let engine = AgentEngine::new_test(manager, config.clone(), root.path()).await;
    let thread_id = "thread-participant-compaction-reset";
    let playground_thread_id =
        crate::agent::agent_identity::participant_playground_thread_id(thread_id, "weles");

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            crate::agent::types::AgentThread {
                id: thread_id.to_string(),
                agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
                title: "Participant compaction reset".to_string(),
                messages: vec![
                    crate::agent::types::AgentMessage::user("hello", 1),
                    assistant_message("step 1", 2),
                    assistant_message("step 2", 3),
                    assistant_message("step 3", 4),
                    assistant_message("step 4", 5),
                ],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: 1,
                updated_at: 5,
            },
        );
        threads.insert(
            playground_thread_id.clone(),
            crate::agent::types::AgentThread {
                id: playground_thread_id.clone(),
                agent_name: Some("Weles".to_string()),
                title: "Participant Playground".to_string(),
                messages: vec![
                    compaction_artifact_message("[Compacted earlier context] playground", 1),
                    crate::agent::types::AgentMessage::user("draft prompt 2", 2),
                    assistant_message("draft reply 2", 3),
                    crate::agent::types::AgentMessage::user("draft prompt 3", 4),
                    assistant_message("draft reply 3", 5),
                    crate::agent::types::AgentMessage::user("draft prompt 4", 6),
                ],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 11,
                total_output_tokens: 7,
                created_at: 1,
                updated_at: 6,
            },
        );
    }

    let compacted = engine
        .maybe_persist_compaction_artifact(thread_id, None, &config, &provider_config)
        .await
        .expect("compaction should succeed");
    assert!(compacted, "expected compaction artifact to be persisted");

    let threads = engine.threads.read().await;
    let playground = threads
        .get(&playground_thread_id)
        .expect("playground thread should still exist after compaction");
    assert_eq!(
        playground.messages.len(),
        4,
        "compaction should retain a bounded recent playground tail instead of clearing hidden context"
    );
    assert_eq!(
        playground.messages[0].message_kind,
        crate::agent::types::AgentMessageKind::CompactionArtifact,
        "compaction should preserve the latest playground artifact boundary"
    );
    assert_eq!(playground.messages[1].content, "draft prompt 3");
    assert_eq!(playground.messages[3].content, "draft prompt 4");
}

#[tokio::test]
async fn execute_managed_command_auto_approves_learned_git_category() {
    let recorded_bodies = Arc::new(Mutex::new(std::collections::VecDeque::new()));
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.operator_model.enabled = true;
    config.operator_model.allow_approval_learning = true;
    config.provider = "openai".to_string();
    config.base_url = super::part4::spawn_stub_assistant_server_for_tool_executor(
        recorded_bodies,
        serde_json::json!({
            "verdict": "allow",
            "reasons": ["runtime approved managed command test"],
            "audit_id": "audit-weles-managed-approve"
        })
        .to_string(),
    )
    .await;
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = crate::agent::types::ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    let engine = AgentEngine::new_test(manager.clone(), config, root.path()).await;
    let (event_tx, _) = broadcast::channel(8);
    let repo_dir = root.path().join("git-repo");
    std::fs::create_dir_all(&repo_dir).expect("create repo dir");
    std::process::Command::new("git")
        .arg("init")
        .current_dir(&repo_dir)
        .output()
        .expect("git init should succeed");
    let (session_id, _rx) = manager
        .spawn(Some("/bin/bash".to_string()), None, None, None, 80, 24)
        .await
        .expect("spawn session");

    {
        let mut model = engine.operator_model.write().await;
        model.risk_fingerprint.approvals = 3;
        model
            .risk_fingerprint
            .category_requests
            .insert("git".to_string(), 3);
        model
            .risk_fingerprint
            .category_approvals
            .insert("git".to_string(), 3);
        model.risk_fingerprint.auto_approve_categories = vec!["git".to_string()];
    }

    let tool_call = ToolCall::with_default_weles_review(
        "tool-managed-auto-approve".to_string(),
        ToolFunction {
            name: "execute_managed_command".to_string(),
            arguments: serde_json::json!({
                "session": session_id.to_string(),
                "command": "git status --short",
                "rationale": "Check repo status",
                "cwd": repo_dir,
                "allow_network": true,
                "timeout_seconds": 5
            })
            .to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        "thread-managed-auto-approve",
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(
        !result.is_error,
        "learned auto-approve should execute successfully: {}",
        result.content
    );
    assert!(result.pending_approval.is_none());
    assert!(result.content.contains("Managed command finished"));

    let model = engine.operator_model.read().await;
    assert_eq!(model.risk_fingerprint.approval_requests, 1);
    assert_eq!(model.risk_fingerprint.approvals, 4);
    assert_eq!(
        model.risk_fingerprint.auto_approve_categories,
        vec!["git".to_string()]
    );
}

#[tokio::test]
async fn execute_managed_command_auto_approves_saved_rule() {
    let recorded_bodies = Arc::new(Mutex::new(std::collections::VecDeque::new()));
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.operator_model.enabled = false;
    config.provider = "openai".to_string();
    config.base_url = super::part4::spawn_stub_assistant_server_for_tool_executor(
        recorded_bodies,
        serde_json::json!({
            "verdict": "allow",
            "reasons": ["runtime approved managed command test"],
            "audit_id": "audit-weles-managed-saved-rule"
        })
        .to_string(),
    )
    .await;
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = crate::agent::types::ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    let engine = AgentEngine::new_test(manager.clone(), config, root.path()).await;
    let (event_tx, _) = broadcast::channel(8);
    let repo_dir = root.path().join("git-repo");
    std::fs::create_dir_all(&repo_dir).expect("create repo dir");
    std::process::Command::new("git")
        .arg("init")
        .current_dir(&repo_dir)
        .output()
        .expect("git init should succeed");
    let (session_id, _rx) = manager
        .spawn(Some("/bin/bash".to_string()), None, None, None, 80, 24)
        .await
        .expect("spawn session");

    engine
        .task_approval_rules
        .write()
        .await
        .push(zorai_protocol::TaskApprovalRule {
            id: "rule-managed-git".to_string(),
            command: "git status --short".to_string(),
            created_at: 1,
            last_used_at: None,
            use_count: 0,
        });

    let tool_call = ToolCall::with_default_weles_review(
        "tool-managed-saved-rule".to_string(),
        ToolFunction {
            name: "execute_managed_command".to_string(),
            arguments: serde_json::json!({
                "session": session_id.to_string(),
                "command": "git status --short",
                "rationale": "Check repo status",
                "cwd": repo_dir,
                "allow_network": true,
                "timeout_seconds": 5
            })
            .to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        "thread-managed-saved-rule",
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(
        !result.is_error,
        "saved-rule auto-approve should execute successfully: {}",
        result.content
    );
    assert!(result.pending_approval.is_none());
    assert!(result.content.contains("Managed command finished"));

    let rule = engine
        .list_task_approval_rules()
        .await
        .into_iter()
        .find(|rule| rule.id == "rule-managed-git")
        .expect("saved rule should remain");
    assert_eq!(rule.use_count, 1);
    assert!(rule.last_used_at.is_some());
}

#[tokio::test]
async fn message_agent_rejects_self_target_for_active_responder() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);
    let thread_id = "thread-self-message";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            crate::agent::types::AgentThread {
                id: thread_id.to_string(),
                agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
                title: "Sticky Svarog".to_string(),
                messages: vec![crate::agent::types::AgentMessage::user(
                    "Operator wants Svarog",
                    1,
                )],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: 1,
                updated_at: 1,
            },
        );
    }
    engine
        .set_thread_handoff_state(
            thread_id,
            crate::agent::ThreadHandoffState {
                origin_agent_id: crate::agent::agent_identity::MAIN_AGENT_ID.to_string(),
                active_agent_id: crate::agent::agent_identity::MAIN_AGENT_ID.to_string(),
                responder_stack: vec![crate::agent::ThreadResponderFrame {
                    agent_id: crate::agent::agent_identity::MAIN_AGENT_ID.to_string(),
                    agent_name: crate::agent::agent_identity::MAIN_AGENT_NAME.to_string(),
                    entered_at: 1,
                    entered_via_handoff_event_id: None,
                    linked_thread_id: None,
                }],
                events: Vec::new(),
                pending_approval_id: None,
            },
        )
        .await;

    let tool_call = ToolCall::with_default_weles_review(
        "tool-message-self".to_string(),
        ToolFunction {
            name: "message_agent".to_string(),
            arguments: serde_json::json!({
                "target": "svarog",
                "message": "Talk to yourself"
            })
            .to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        thread_id,
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(
        result.is_error,
        "self-targeted message_agent should be rejected"
    );
    assert!(
        result
            .content
            .contains("cannot target the current active responder")
            || result.content.contains("cannot target the same agent"),
        "unexpected self-target error: {}",
        result.content
    );
}

async fn spawn_hanging_assistant_server_for_tool_executor() -> String {
    use tokio::io::AsyncReadExt;
    use tokio::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind hanging assistant server");
    let addr = listener.local_addr().expect("hanging assistant local addr");
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                let mut buffer = vec![0u8; 1024];
                let _ = socket.read(&mut buffer).await;
                tokio::time::sleep(Duration::from_secs(60)).await;
            });
        }
    });
    format!("http://{addr}/v1")
}

#[tokio::test]
async fn message_agent_internal_dm_returns_without_waiting_for_recipient_turn() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = "openai".to_string();
    config.base_url = spawn_hanging_assistant_server_for_tool_executor().await;
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = crate::agent::types::ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    let engine = AgentEngine::new_test(manager.clone(), config, root.path()).await;
    let (event_tx, _) = broadcast::channel(8);
    let thread_id = "thread-message-agent-async-dm";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            crate::agent::types::AgentThread {
                id: thread_id.to_string(),
                agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
                title: "Async internal DM".to_string(),
                messages: vec![crate::agent::types::AgentMessage::user(
                    "Ask Weles privately",
                    1,
                )],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: 1,
                updated_at: 1,
            },
        );
    }

    let tool_call = ToolCall::with_default_weles_review(
        "tool-message-agent-async-dm".to_string(),
        ToolFunction {
            name: "message_agent".to_string(),
            arguments: serde_json::json!({
                "target": "weles",
                "message": "Need a private coordination check."
            })
            .to_string(),
        },
    );

    let result = timeout(
        Duration::from_secs(2),
        execute_tool(
            &tool_call,
            &engine,
            thread_id,
            None,
            &manager,
            None,
            &event_tx,
            root.path(),
            &engine.http_client,
            None,
        ),
    )
    .await
    .expect("message_agent should return before the recipient turn finishes");

    assert!(
        !result.is_error,
        "async internal DM delivery should succeed: {}",
        result.content
    );
    assert!(
        result.content.contains("\"delivered\": true"),
        "tool result should report async delivery: {}",
        result.content
    );
    assert!(
        result
            .content
            .contains("\"visible_thread_continuation_requested\": false"),
        "internal DM path should not request visible-thread continuation: {}",
        result.content
    );
    let dm_thread_id = crate::agent::agent_identity::internal_dm_thread_id(
        crate::agent::agent_identity::MAIN_AGENT_ID,
        crate::agent::agent_identity::WELES_AGENT_ID,
    );
    assert!(
        engine.threads.read().await.contains_key(&dm_thread_id),
        "internal DM thread should exist as soon as the message is queued"
    );
}

#[tokio::test]
async fn message_agent_can_request_visible_thread_continuation() {
    let recorded_bodies = Arc::new(Mutex::new(std::collections::VecDeque::new()));
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = "openai".to_string();
    config.base_url = super::part4::spawn_stub_assistant_server_for_tool_executor(
        recorded_bodies.clone(),
        serde_json::json!({
            "verdict": "allow",
            "reasons": ["unused for visible thread continuation test"],
            "audit_id": "audit-message-agent-visible-thread"
        })
        .to_string(),
    )
    .await;
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = crate::agent::types::ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    let engine = AgentEngine::new_test(manager.clone(), config, root.path()).await;
    let (event_tx, _) = broadcast::channel(8);
    let thread_id = "thread-message-agent-visible-thread";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            crate::agent::types::AgentThread {
                id: thread_id.to_string(),
                agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
                title: "Thread-scoped internal DM".to_string(),
                messages: vec![crate::agent::types::AgentMessage::user(
                    "Please continue this work",
                    1,
                )],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: 1,
                updated_at: 1,
            },
        );
    }

    let tool_call = ToolCall::with_default_weles_review(
        "tool-message-agent-visible-thread".to_string(),
        ToolFunction {
            name: "message_agent".to_string(),
            arguments: serde_json::json!({
                "target": "weles",
                "message": "Please pick up the work from governance scope.",
                "request_visible_thread_continuation": true
            })
            .to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        thread_id,
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(
        !result.is_error,
        "thread continuation via message_agent should succeed: {}",
        result.content
    );
    assert!(
        result
            .content
            .contains("\"visible_thread_continuation_requested\": true"),
        "tool result should report that visible-thread continuation was requested: {}",
        result.content
    );
    engine
        .flush_deferred_visible_thread_continuations(thread_id)
        .await
        .expect("deferred visible-thread continuation should flush");

    let dm_thread_id = crate::agent::agent_identity::internal_dm_thread_id(
        crate::agent::agent_identity::MAIN_AGENT_ID,
        crate::agent::agent_identity::WELES_AGENT_ID,
    );
    let threads = engine.threads.read().await;
    assert!(
        threads
            .get(&dm_thread_id)
            .expect("message_agent should still use internal DM for coordination")
            .messages
            .iter()
            .any(|message| message.role == crate::agent::MessageRole::Assistant),
        "internal DM thread should contain the discussion reply"
    );
    let visible_reply = threads
        .get(thread_id)
        .expect("visible thread should remain present")
        .messages
        .iter()
        .rev()
        .find(|message| {
            message.role == crate::agent::MessageRole::Assistant
                && message.author_agent_id.as_deref() == Some("weles")
        })
        .expect("visible thread should receive the Weles continuation");
    assert!(
        !visible_reply.content.trim().is_empty(),
        "visible-thread continuation should append a non-empty assistant reply"
    );
}

#[tokio::test]
async fn message_agent_to_active_thread_participant_defaults_to_visible_thread_continuation() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);
    let thread_id = "thread-message-agent-participant-default-continuation";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            crate::agent::types::AgentThread {
                id: thread_id.to_string(),
                agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
                title: "Participant default continuation".to_string(),
                messages: vec![crate::agent::types::AgentMessage::user(
                    "Please keep this work visible",
                    1,
                )],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: 1,
                updated_at: 1,
            },
        );
    }
    engine
        .upsert_thread_participant(thread_id, "weles", "verify and keep momentum")
        .await
        .expect("participant should register");

    let tool_call = ToolCall::with_default_weles_review(
        "tool-message-agent-participant-default-continuation".to_string(),
        ToolFunction {
            name: "message_agent".to_string(),
            arguments: serde_json::json!({
                "target": "weles",
                "message": "Please continue this work from your participant role."
            })
            .to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        thread_id,
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(
        !result.is_error,
        "message_agent to active participant should queue visible continuation: {}",
        result.content
    );
    assert!(
        result
            .content
            .contains("\"visible_thread_continuation_requested\": true"),
        "active participant message_agent should default to visible-thread continuation: {}",
        result.content
    );
    assert_eq!(
        engine
            .deferred_visible_thread_continuations_for(thread_id)
            .await
            .len(),
        1,
        "active participant message_agent should queue a visible-thread continuation"
    );
}

#[tokio::test]
async fn execute_managed_command_auto_denies_learned_destructive_category() {
    let recorded_bodies = Arc::new(Mutex::new(std::collections::VecDeque::new()));
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.operator_model.enabled = true;
    config.operator_model.allow_approval_learning = true;
    config.provider = "openai".to_string();
    config.base_url = super::part4::spawn_stub_assistant_server_for_tool_executor(
        recorded_bodies,
        serde_json::json!({
            "verdict": "allow",
            "reasons": ["runtime approved managed command test"],
            "audit_id": "audit-weles-managed-deny"
        })
        .to_string(),
    )
    .await;
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = crate::agent::types::ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    let engine = AgentEngine::new_test(manager.clone(), config, root.path()).await;
    let (event_tx, _) = broadcast::channel(8);
    let delete_dir = root.path().join("delete-me");
    std::fs::create_dir_all(&delete_dir).expect("create delete target");
    let (session_id, _rx) = manager
        .spawn(Some("/bin/bash".to_string()), None, None, None, 80, 24)
        .await
        .expect("spawn session");

    {
        let mut model = engine.operator_model.write().await;
        model.risk_fingerprint.denials = 3;
        model
            .risk_fingerprint
            .category_requests
            .insert("destructive_delete".to_string(), 3);
        model.risk_fingerprint.auto_deny_categories = vec!["destructive_delete".to_string()];
    }

    let tool_call = ToolCall::with_default_weles_review(
        "tool-managed-auto-deny".to_string(),
        ToolFunction {
            name: "execute_managed_command".to_string(),
            arguments: serde_json::json!({
                "session": session_id.to_string(),
                "command": format!("rm -rf {}", delete_dir.display()),
                "rationale": "Clean up generated scratch directory",
                "timeout_seconds": 5
            })
            .to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        "thread-managed-auto-deny",
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(
        !result.is_error,
        "learned auto-deny should return a policy message: {}",
        result.content
    );
    assert!(result.pending_approval.is_none());
    assert!(result.content.contains("auto-denied"));
    assert!(
        delete_dir.exists(),
        "the destructive command should not execute after learned denial"
    );

    let model = engine.operator_model.read().await;
    assert_eq!(model.risk_fingerprint.approval_requests, 1);
    assert_eq!(model.risk_fingerprint.denials, 4);
    assert_eq!(
        model.risk_fingerprint.auto_deny_categories,
        vec!["destructive_delete".to_string()]
    );
}

#[tokio::test]
async fn auto_backgrounded_bash_command_returns_handle_and_status() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);
    let (session_id, _rx) = manager
        .spawn(Some("/bin/bash".to_string()), None, None, None, 80, 24)
        .await
        .expect("spawn session");

    let tool_call = ToolCall::with_default_weles_review(
        "tool-bash-auto-background".to_string(),
        ToolFunction {
            name: "bash_command".to_string(),
            arguments: serde_json::json!({
                "session": session_id.to_string(),
                "command": "sleep 0.2 && printf done",
                "timeout_seconds": 1200,
                "sandbox_enabled": true,
                "allow_network": false,
                "security_level": "moderate"
            })
            .to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        "thread-bash-auto-background",
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(
        !result.is_error,
        "auto-backgrounded bash command should queue cleanly: {}",
        result.content
    );

    let background_task_id = result
        .content
        .lines()
        .find_map(|line| line.strip_prefix("background_task_id: "))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .expect("auto-background response should expose a background_task_id handle");

    let status_call = ToolCall::with_default_weles_review(
        "tool-background-status".to_string(),
        ToolFunction {
            name: "get_background_task_status".to_string(),
            arguments: serde_json::json!({
                "background_task_id": background_task_id,
            })
            .to_string(),
        },
    );

    let mut payload = serde_json::Value::Null;
    let mut completed = false;
    for _ in 0..20 {
        let status = execute_tool(
            &status_call,
            &engine,
            "thread-bash-auto-background",
            None,
            &manager,
            None,
            &event_tx,
            root.path(),
            &engine.http_client,
            None,
        )
        .await;

        assert!(
            !status.is_error,
            "background task status lookup should succeed: {}",
            status.content
        );

        payload =
            serde_json::from_str(&status.content).expect("status payload should be valid JSON");
        if payload["state"] == "completed" {
            completed = true;
            break;
        }

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    assert!(
        completed,
        "background task should complete within the polling window: {}",
        payload
    );
    assert_eq!(payload["background_task_id"], background_task_id);
    assert_eq!(payload["state"], "completed");
    assert_eq!(payload["exit_code"], 0);
    assert_eq!(payload["command"], "sleep 0.2 && printf done");
}

#[tokio::test]
async fn tui_bash_command_wait_false_returns_immediate_operation_handle() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);
    let marker = root.path().join("tui-headless-background.txt");
    let thread_id = "thread-tui-bash-wait-false";
    engine
        .set_thread_client_surface(thread_id, zorai_protocol::ClientSurface::Tui)
        .await;

    let command = format!(
            "python3 -c \"import pathlib, time; time.sleep(1); pathlib.Path(r'{}').write_text('done')\"",
            marker.display()
        );
    let tool_call = ToolCall::with_default_weles_review(
        "tool-tui-bash-wait-false".to_string(),
        ToolFunction {
            name: "bash_command".to_string(),
            arguments: serde_json::json!({
                "command": command,
                "wait_for_completion": false,
            })
            .to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        thread_id,
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(
        !result.is_error,
        "non-blocking TUI bash command should return immediately: {}",
        result.content
    );
    assert!(
        result.content.contains("operation_id: "),
        "non-blocking TUI bash command should return an operation handle: {}",
        result.content
    );
    assert!(
        !marker.exists(),
        "backgrounded headless TUI command should not have finished before returning"
    );

    let operation_id = result
        .content
        .lines()
        .find_map(|line| line.strip_prefix("operation_id: "))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .expect("backgrounded TUI bash command should expose an operation_id");

    let status_call = ToolCall::with_default_weles_review(
        "tool-tui-bash-operation-status".to_string(),
        ToolFunction {
            name: "get_operation_status".to_string(),
            arguments: serde_json::json!({
                "operation_id": operation_id,
            })
            .to_string(),
        },
    );

    let mut payload = serde_json::Value::Null;
    let mut completed = false;
    for _ in 0..20 {
        let status = execute_tool(
            &status_call,
            &engine,
            thread_id,
            None,
            &manager,
            None,
            &event_tx,
            root.path(),
            &engine.http_client,
            None,
        )
        .await;

        assert!(
            !status.is_error,
            "operation status lookup should succeed: {}",
            status.content
        );

        payload = serde_json::from_str(&status.content)
            .expect("operation status payload should be valid JSON");
        if payload["state"] == "completed" {
            completed = true;
            break;
        }

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    assert!(
        completed,
        "backgrounded headless TUI command should complete within the polling window: {}",
        payload
    );
    assert_eq!(payload["operation_id"], operation_id);
    assert_eq!(payload["state"], "completed");
    assert!(
        marker.exists(),
        "backgrounded headless TUI command should finish after polling"
    );
}

#[tokio::test]
async fn get_operation_status_resolves_mistyped_id_by_unique_first_segment() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let record = crate::server::operation_registry()
        .accept_operation(zorai_protocol::tool_names::BASH_COMMAND, None);
    let first_segment = record
        .operation_id
        .split('-')
        .next()
        .expect("uuid should have a first segment");
    let mangled = format!("{first_segment}-1111-4111-8111-111111111111");

    let args = serde_json::json!({ "operation_id": mangled });
    let result = execute_get_operation_status(&args, &manager, None, None, None)
        .await
        .expect("a mistyped id with a unique correct leading segment should still resolve");
    let payload: serde_json::Value =
        serde_json::from_str(&result).expect("status payload should be valid JSON");

    assert_eq!(
        payload["operation_id"], record.operation_id,
        "payload should report the real registered operation id"
    );
    assert_eq!(
        payload["requested_operation_id"], mangled,
        "payload should surface the id the caller actually asked for"
    );
    assert!(
        payload["status_note"]
            .as_str()
            .is_some_and(|note| note.contains(&record.operation_id)),
        "status note should tell the agent the exact id to use next time: {payload}"
    );
}

#[tokio::test]
async fn tui_bash_command_wait_true_detaches_long_running_headless_command() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);
    let marker = root.path().join("tui-headless-wait-true-detached.txt");
    let thread_id = "thread-tui-bash-wait-true-detach";
    engine
        .set_thread_client_surface(thread_id, zorai_protocol::ClientSurface::Tui)
        .await;

    let command = format!(
        "python3 -c \"import pathlib, time; time.sleep(2); pathlib.Path(r'{}').write_text('done')\"",
        marker.display()
    );
    let tool_call = ToolCall::with_default_weles_review(
        "tool-tui-bash-wait-true-detach".to_string(),
        ToolFunction {
            name: "bash_command".to_string(),
            arguments: serde_json::json!({
                "command": command,
                "wait_for_completion": true,
                "timeout_seconds": 30,
            })
            .to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        thread_id,
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(
        !result.is_error,
        "long-running TUI wait=true bash command should detach cleanly: {}",
        result.content
    );
    assert!(
        result.content.contains("operation_id: "),
        "detached TUI wait=true bash command should return an operation handle: {}",
        result.content
    );
    assert!(
        !marker.exists(),
        "detached TUI wait=true command should return before the subprocess finishes"
    );

    let operation_id = result
        .content
        .lines()
        .find_map(|line| line.strip_prefix("operation_id: "))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .expect("detached TUI wait=true command should expose an operation_id");

    let status_call = ToolCall::with_default_weles_review(
        "tool-tui-bash-wait-true-detach-status".to_string(),
        ToolFunction {
            name: "get_operation_status".to_string(),
            arguments: serde_json::json!({
                "operation_id": operation_id,
            })
            .to_string(),
        },
    );

    let mut payload = serde_json::Value::Null;
    let mut completed = false;
    for _ in 0..30 {
        let status = execute_tool(
            &status_call,
            &engine,
            thread_id,
            None,
            &manager,
            None,
            &event_tx,
            root.path(),
            &engine.http_client,
            None,
        )
        .await;

        assert!(
            !status.is_error,
            "operation status lookup should succeed: {}",
            status.content
        );

        payload = serde_json::from_str(&status.content)
            .expect("operation status payload should be valid JSON");
        if payload["state"] == "completed" {
            completed = true;
            break;
        }

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    assert!(
        completed,
        "detached TUI wait=true command should complete within the polling window: {}",
        payload
    );
    assert_eq!(payload["operation_id"], operation_id);
    assert_eq!(payload["state"], "completed");
    assert!(
        marker.exists(),
        "detached TUI wait=true command should eventually finish"
    );
}

#[tokio::test]
async fn tui_python_execute_wait_for_response_detaches_long_running_command() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);
    let marker = root
        .path()
        .join("tui-python-execute-wait-true-detached.txt");
    let thread_id = "thread-tui-python-execute-wait-true-detach";
    engine
        .set_thread_client_surface(thread_id, zorai_protocol::ClientSurface::Tui)
        .await;

    let tool_call = ToolCall::with_default_weles_review(
        "tool-tui-python-execute-wait-true-detach".to_string(),
        ToolFunction {
            name: "python_execute".to_string(),
            arguments: serde_json::json!({
                "code": format!(
                    "import pathlib\nimport time\ntime.sleep(2)\npathlib.Path(r'{}').write_text('done')\nprint('finished')",
                    marker.display()
                ),
                "wait_for_response": true,
                "timeout_seconds": 30,
            })
            .to_string(),
        },
    );

    let result = timeout(
        Duration::from_millis(1500),
        execute_tool(
            &tool_call,
            &engine,
            thread_id,
            None,
            &manager,
            None,
            &event_tx,
            root.path(),
            &engine.http_client,
            None,
        ),
    )
    .await
    .expect("long-running TUI python_execute should detach within the foreground grace window");

    assert!(
        !result.is_error,
        "long-running TUI wait=true python_execute should detach cleanly: {}",
        result.content
    );
    assert!(
        result.content.contains("operation_id: "),
        "detached TUI wait=true python_execute should return an operation handle: {}",
        result.content
    );
    assert!(
        !marker.exists(),
        "detached TUI wait=true python_execute should return before the subprocess finishes"
    );

    let operation_id = result
        .content
        .lines()
        .find_map(|line| line.strip_prefix("operation_id: "))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .expect("detached TUI wait=true python_execute should expose an operation_id");

    let status_call = ToolCall::with_default_weles_review(
        "tool-tui-python-execute-wait-true-detach-status".to_string(),
        ToolFunction {
            name: "get_operation_status".to_string(),
            arguments: serde_json::json!({
                "operation_id": operation_id,
            })
            .to_string(),
        },
    );

    let mut payload = serde_json::Value::Null;
    let mut completed = false;
    for _ in 0..30 {
        let status = execute_tool(
            &status_call,
            &engine,
            thread_id,
            None,
            &manager,
            None,
            &event_tx,
            root.path(),
            &engine.http_client,
            None,
        )
        .await;

        assert!(
            !status.is_error,
            "python_execute operation status lookup should succeed: {}",
            status.content
        );

        payload = serde_json::from_str(&status.content)
            .expect("python_execute operation status payload should be valid JSON");
        if payload["state"] == "completed" {
            completed = true;
            break;
        }

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    assert!(
        completed,
        "detached TUI wait=true python_execute should complete within the polling window: {}",
        payload
    );
    assert_eq!(payload["operation_id"], operation_id);
    assert_eq!(payload["state"], "completed");
    assert!(
        marker.exists(),
        "detached TUI wait=true python_execute should eventually finish"
    );
}

#[tokio::test]
async fn bash_command_wait_true_waits_for_non_quick_headless_command() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);
    let marker = root.path().join("headless-non-quick-backgrounded.txt");
    let thread_id = "thread-bash-non-quick-background";

    let command = format!(
        "python3 -c \"import pathlib, time; time.sleep(1); pathlib.Path(r'{}').write_text('done')\"",
        marker.display()
    );
    let tool_call = ToolCall::with_default_weles_review(
        "tool-bash-non-quick-background".to_string(),
        ToolFunction {
            name: "bash_command".to_string(),
            arguments: serde_json::json!({
                "command": command,
                "wait_for_completion": true,
                "timeout_seconds": 30,
            })
            .to_string(),
        },
    );

    let result = timeout(
        Duration::from_secs(10),
        execute_tool(
            &tool_call,
            &engine,
            thread_id,
            None,
            &manager,
            None,
            &event_tx,
            root.path(),
            &engine.http_client,
            None,
        ),
    )
    .await
    .expect("non-quick bash_command should honor wait_for_completion=true in the foreground");

    assert!(
        !result.is_error,
        "wait=true non-quick bash command should complete synchronously: {}",
        result.content
    );
    assert!(
        !result.content.contains("background_task_id: "),
        "explicit wait_for_completion=true must not be silently rewritten to a background dispatch: {}",
        result.content
    );
    assert!(
        marker.exists(),
        "wait=true command should finish before the tool call returns"
    );
}

#[tokio::test]
async fn tui_bash_command_falls_back_to_goal_run_surface_when_thread_surface_is_missing() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);
    let thread_id = "thread-tui-goal-surface-fallback";
    let task_id = "task-tui-goal-surface-fallback";

    let goal_run = engine
        .start_goal_run_with_surface(
            "Verify TUI shell routing".to_string(),
            None,
            Some(thread_id.to_string()),
            None,
            None,
            None,
            None,
            Some(zorai_protocol::ClientSurface::Tui),
            None,
        )
        .await;
    engine.clear_thread_client_surface(thread_id).await;

    {
        let mut tasks = engine.tasks.lock().await;
        tasks.push_back(crate::agent::types::AgentTask {
            id: task_id.to_string(),
            title: "Run routed shell command".to_string(),
            description: "Exercise TUI fallback shell routing".to_string(),
            status: crate::agent::types::TaskStatus::InProgress,
            priority: crate::agent::types::TaskPriority::Normal,
            progress: 0,
            created_at: 0,
            started_at: None,
            completed_at: None,
            error: None,
            result: None,
            thread_id: Some(thread_id.to_string()),
            source: "test".to_string(),
            notify_on_complete: false,
            notify_channels: Vec::new(),
            dependencies: Vec::new(),
            command: None,
            session_id: None,
            goal_run_id: Some(goal_run.id.clone()),
            goal_run_title: Some(goal_run.title.clone()),
            goal_step_id: None,
            goal_step_title: None,
            parent_task_id: None,
            parent_thread_id: None,
            runtime: "foreground".to_string(),
            retry_count: 0,
            max_retries: 0,
            next_retry_at: None,
            scheduled_at: None,
            blocked_reason: None,
            awaiting_approval_id: None,
            policy_fingerprint: None,
            approval_expires_at: None,
            containment_scope: None,
            compensation_status: None,
            compensation_summary: None,
            lane_id: None,
            last_error: None,
            logs: Vec::new(),
            completion_contract: None,
            tool_whitelist: None,
            tool_blacklist: None,
            context_budget_tokens: None,
            context_overflow_action: None,
            termination_conditions: None,
            success_criteria: None,
            max_duration_secs: None,
            supervisor_config: None,
            override_provider: None,
            override_model: None,
            override_api_transport: None,
            override_system_prompt: None,
            sub_agent_def_id: None,
        });
    }

    let plan_call = ToolCall::with_default_weles_review(
        "tool-tui-goal-surface-plan".to_string(),
        ToolFunction {
            name: tool_names::UPDATE_TODO.to_string(),
            arguments: serde_json::json!({
                "todos": [
                    { "content": "Verify shell routing stays headless", "status": "in_progress" }
                ]
            })
            .to_string(),
        },
    );

    let plan_result = execute_tool(
        &plan_call,
        &engine,
        thread_id,
        Some(task_id),
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(
        !plan_result.is_error,
        "update_todo should establish plan mode before shell execution: {}",
        plan_result.content
    );

    let tool_call = ToolCall::with_default_weles_review(
        "tool-tui-goal-surface-fallback".to_string(),
        ToolFunction {
            name: "bash_command".to_string(),
            arguments: serde_json::json!({
                "command": "printf routed-via-goal-surface",
                "sandbox_enabled": true,
            })
            .to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        thread_id,
        Some(task_id),
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(
        !result.is_error,
        "TUI goal-run surface should keep bash_command headless: {}",
        result.content
    );
    assert!(
        result.content.contains("routed-via-goal-surface"),
        "headless execution should return command output: {}",
        result.content
    );
}

#[tokio::test]
async fn bash_command_with_sandbox_hint_runs_headless_without_terminal_session() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);
    let thread_id = "thread-sandbox-hint-headless";

    let tool_call = ToolCall::with_default_weles_review(
        "tool-sandbox-hint-headless".to_string(),
        ToolFunction {
            name: "bash_command".to_string(),
            arguments: serde_json::json!({
                "command": "pwd && printf '\\nheadless-ok'",
                "cwd": root.path(),
                "allow_network": false,
                "sandbox_enabled": true,
                "security_level": "moderate",
                "wait_for_completion": true,
                "timeout_seconds": 30,
            })
            .to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        thread_id,
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(
        !result.is_error,
        "sandbox hints on a simple one-shot command should not require a live terminal session: {}",
        result.content
    );
    assert!(
        result.content.contains("headless-ok"),
        "headless execution should return command output: {}",
        result.content
    );
}

#[tokio::test]
async fn tui_bash_command_wait_false_exposes_failure_payload_via_operation_status() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);
    let thread_id = "thread-tui-bash-wait-false-failure";
    engine
        .set_thread_client_surface(thread_id, zorai_protocol::ClientSurface::Tui)
        .await;

    let tool_call = ToolCall::with_default_weles_review(
            "tool-tui-bash-wait-false-failure".to_string(),
            ToolFunction {
                name: "bash_command".to_string(),
                arguments: serde_json::json!({
                    "command": "python3 -c \"import sys, time; print('stdout-line'); print('stderr-line', file=sys.stderr); time.sleep(0.2); sys.exit(7)\"",
                    "wait_for_completion": false,
                })
                .to_string(),
            },
        );

    let result = execute_tool(
        &tool_call,
        &engine,
        thread_id,
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(
        !result.is_error,
        "non-blocking TUI bash failure should still return an operation handle: {}",
        result.content
    );

    let operation_id = result
        .content
        .lines()
        .find_map(|line| line.strip_prefix("operation_id: "))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .expect("backgrounded TUI bash failure should expose an operation_id");

    let status_call = ToolCall::with_default_weles_review(
        "tool-tui-bash-operation-status-failure".to_string(),
        ToolFunction {
            name: "get_operation_status".to_string(),
            arguments: serde_json::json!({
                "operation_id": operation_id,
            })
            .to_string(),
        },
    );

    let mut payload = serde_json::Value::Null;
    let mut failed = false;
    for _ in 0..20 {
        let status = execute_tool(
            &status_call,
            &engine,
            thread_id,
            None,
            &manager,
            None,
            &event_tx,
            root.path(),
            &engine.http_client,
            None,
        )
        .await;

        assert!(
            !status.is_error,
            "operation status lookup should succeed: {}",
            status.content
        );

        payload = serde_json::from_str(&status.content)
            .expect("operation status payload should be valid JSON");
        if payload["state"] == "failed" {
            failed = true;
            break;
        }

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    assert!(
        failed,
        "backgrounded headless TUI failure should reach failed state within the polling window: {}",
        payload
    );
    assert_eq!(payload["operation_id"], operation_id);
    assert_eq!(payload["state"], "failed");
    assert_eq!(payload["exit_code"], 7);
    assert!(
        payload["terminal_result"]["stdout"]
            .as_str()
            .is_some_and(|value| value.contains("stdout-line")),
        "failed background status should include stdout payload: {}",
        payload
    );
    assert!(
        payload["terminal_result"]["stderr"]
            .as_str()
            .is_some_and(|value| value.contains("stderr-line")),
        "failed background status should include stderr payload: {}",
        payload
    );
}

#[tokio::test]
async fn tui_bash_command_wait_false_completes_when_descendant_keeps_pipes_open() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);
    let thread_id = "thread-tui-bash-descendant-pipes";
    engine
        .set_thread_client_surface(thread_id, zorai_protocol::ClientSurface::Tui)
        .await;

    let tool_call = ToolCall::with_default_weles_review(
        "tool-tui-bash-descendant-pipes".to_string(),
        ToolFunction {
            name: "bash_command".to_string(),
            arguments: serde_json::json!({
                "command": "python3 -c \"import subprocess, sys; subprocess.Popen(['sleep', '5'], stdout=sys.stdout, stderr=sys.stderr); print('parent-done')\"",
                "wait_for_completion": false,
            })
            .to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        thread_id,
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(
        !result.is_error,
        "non-blocking TUI bash should return an operation handle: {}",
        result.content
    );

    let operation_id = result
        .content
        .lines()
        .find_map(|line| line.strip_prefix("operation_id: "))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .expect("backgrounded TUI bash should expose an operation_id");

    let status_call = ToolCall::with_default_weles_review(
        "tool-tui-bash-descendant-pipes-status".to_string(),
        ToolFunction {
            name: "get_operation_status".to_string(),
            arguments: serde_json::json!({
                "operation_id": operation_id,
            })
            .to_string(),
        },
    );

    let mut payload = serde_json::Value::Null;
    let mut completed = false;
    // Poll up to ~4s under parallel test load (python3 startup can exceed the
    // old 1s window on a busy machine); the descendant invariant is still
    // enforced below via duration_ms, which must stay well under the 5s sleep.
    for _ in 0..40 {
        let status = execute_tool(
            &status_call,
            &engine,
            thread_id,
            None,
            &manager,
            None,
            &event_tx,
            root.path(),
            &engine.http_client,
            None,
        )
        .await;

        assert!(
            !status.is_error,
            "operation status lookup should succeed: {}",
            status.content
        );

        payload = serde_json::from_str(&status.content)
            .expect("operation status payload should be valid JSON");
        if payload["state"] == "completed" {
            completed = true;
            break;
        }

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    assert!(
        completed,
        "operation should complete after the primary shell exits, without waiting for descendant-held pipes to close: {}",
        payload
    );
    assert_eq!(payload["operation_id"], operation_id);
    assert_eq!(payload["state"], "completed");
    assert_eq!(payload["exit_code"], 0);
    assert!(
        payload["terminal_result"]["duration_ms"]
            .as_u64()
            .is_some_and(|value| value < 4500),
        "operation must complete after the primary shell exits, not when the 5s descendant releases the pipes: {}",
        payload
    );
    assert!(
        payload["terminal_result"]["stdout"]
            .as_str()
            .is_some_and(|value| value.contains("parent-done")),
        "completed background status should include available stdout payload: {}",
        payload
    );
}

#[tokio::test]
async fn get_operation_status_returns_server_operation_snapshot() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);

    let record = crate::server::operation_registry()
        .accept_operation("skill_publish", Some("tool-test:skill-publish".to_string()));
    crate::server::operation_registry().mark_started(&record.operation_id);
    crate::server::operation_registry().mark_completed(&record.operation_id);

    let tool_call = ToolCall::with_default_weles_review(
        "tool-operation-status".to_string(),
        ToolFunction {
            name: "get_operation_status".to_string(),
            arguments: serde_json::json!({
                "operation_id": record.operation_id,
            })
            .to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        "thread-operation-status",
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(
        !result.is_error,
        "get_operation_status should return server operation snapshots: {}",
        result.content
    );

    let payload: serde_json::Value =
        serde_json::from_str(&result.content).expect("status payload should be valid JSON");
    assert_eq!(payload["operation_id"], record.operation_id);
    assert_eq!(payload["kind"], "skill_publish");
    assert_eq!(payload["state"], "completed");
}

#[tokio::test]
async fn get_operation_status_wait_blocks_until_terminal_state() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);

    let record = crate::server::operation_registry()
        .accept_operation("skill_publish", Some("tool-test:wait-complete".to_string()));
    crate::server::operation_registry().mark_started(&record.operation_id);
    engine
        .register_operation_wakeup(
            "thread-operation-status-wait",
            "skill_publish",
            &record.operation_id,
            true,
        )
        .await;

    let operation_id = record.operation_id.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(150)).await;
        crate::server::operation_registry().mark_completed(&operation_id);
    });

    let tool_call = ToolCall::with_default_weles_review(
        "tool-operation-status-wait".to_string(),
        ToolFunction {
            name: "get_operation_status".to_string(),
            arguments: serde_json::json!({
                "operation_id": record.operation_id,
                "wait": true,
                "timeout_seconds": 5,
            })
            .to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        "thread-operation-status-wait",
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(
        !result.is_error,
        "wait=true should return the completed operation: {}",
        result.content
    );
    let payload: serde_json::Value =
        serde_json::from_str(&result.content).expect("status payload should be valid JSON");
    assert_eq!(payload["state"], "completed");
    assert_eq!(payload["waited"], true);
    assert_eq!(engine.pending_operation_wakeup_count().await, 0);
}

#[tokio::test]
async fn get_operation_status_wait_times_out_without_polling_hint_loop() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let record = crate::server::operation_registry()
        .accept_operation("skill_publish", Some("tool-test:wait-timeout".to_string()));
    crate::server::operation_registry().mark_started(&record.operation_id);

    let started = std::time::Instant::now();
    let result = execute_get_operation_status(
        &serde_json::json!({
            "operation_id": record.operation_id,
            "wait": true,
            "timeout_seconds": 1,
        }),
        &manager,
        None,
        None,
        None,
    )
    .await
    .expect("wait timeout should still return a snapshot");
    assert!(
        started.elapsed() >= Duration::from_millis(900),
        "wait=true should block in the daemon instead of returning immediately"
    );

    let payload: serde_json::Value =
        serde_json::from_str(&result).expect("status payload should be valid JSON");
    assert_eq!(payload["state"], "started");
    assert_eq!(payload["wait_timed_out"], true);
    assert!(
        payload["next_step"]
            .as_str()
            .is_some_and(|value| value.contains("wait=true")),
        "timed-out wait should tell the agent to wait again or continue, not poll: {}",
        payload
    );
}

#[tokio::test]
async fn get_operation_status_snapshot_tells_agent_not_to_poll() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let record = crate::server::operation_registry()
        .accept_operation("skill_publish", Some("tool-test:no-poll".to_string()));
    crate::server::operation_registry().mark_started(&record.operation_id);

    let result = execute_get_operation_status(
        &serde_json::json!({ "operation_id": record.operation_id }),
        &manager,
        None,
        None,
        None,
    )
    .await
    .expect("snapshot lookup should succeed");
    let payload: serde_json::Value =
        serde_json::from_str(&result).expect("status payload should be valid JSON");
    assert_eq!(payload["state"], "started");
    assert!(
        payload["next_step"]
            .as_str()
            .is_some_and(|value| value.contains("Do not poll")),
        "non-terminal snapshot should forbid polling: {}",
        payload
    );
}

#[tokio::test]
async fn send_telegram_message_emits_gateway_ipc_request() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    engine.set_gateway_ipc_sender(Some(tx)).await;

    let send_engine = engine.clone();
    let send_task = tokio::spawn(async move {
        execute_gateway_message(
            "send_telegram_message",
            &serde_json::json!({
                "chat_id": "777",
                "message": "telegram reply",
                "reply_to_message_id": 42
            }),
            &send_engine,
            &reqwest::Client::new(),
        )
        .await
    });

    let request = match timeout(Duration::from_millis(250), rx.recv())
        .await
        .expect("gateway send request should be emitted")
        .expect("gateway send request should exist")
    {
        DaemonMessage::GatewaySendRequest { request } => request,
        other => panic!("expected GatewaySendRequest, got {other:?}"),
    };
    assert_eq!(request.platform, "telegram");
    assert_eq!(request.channel_id, "777");
    assert_eq!(request.thread_id.as_deref(), Some("42"));
    assert_eq!(request.content, "telegram reply");

    engine
        .complete_gateway_send_result(GatewaySendResult {
            correlation_id: request.correlation_id.clone(),
            platform: "telegram".to_string(),
            channel_id: "777".to_string(),
            requested_channel_id: Some("777".to_string()),
            delivery_id: Some("99".to_string()),
            ok: true,
            error: None,
            completed_at_ms: 1,
        })
        .await;

    let result = send_task
        .await
        .expect("send task should join")
        .expect("send should succeed");
    assert_eq!(result, "Telegram message sent to 777");
}

#[tokio::test]
async fn send_telegram_message_uses_auto_injected_reply_context_for_chat() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    engine.init_gateway().await;
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    engine.set_gateway_ipc_sender(Some(tx)).await;

    {
        let mut gw_guard = engine.gateway_state.lock().await;
        let gw = gw_guard.as_mut().expect("gateway state should exist");
        gw.reply_contexts.insert(
            "Telegram:777".to_string(),
            crate::agent::gateway::ThreadContext {
                telegram_message_id: Some(42),
                ..Default::default()
            },
        );
    }

    let send_engine = engine.clone();
    let send_task = tokio::spawn(async move {
        execute_gateway_message(
            "send_telegram_message",
            &serde_json::json!({
                "chat_id": "777",
                "message": "telegram reply"
            }),
            &send_engine,
            &reqwest::Client::new(),
        )
        .await
    });

    let request = match timeout(Duration::from_millis(250), rx.recv())
        .await
        .expect("gateway send request should be emitted")
        .expect("gateway send request should exist")
    {
        DaemonMessage::GatewaySendRequest { request } => request,
        other => panic!("expected GatewaySendRequest, got {other:?}"),
    };
    assert_eq!(request.platform, "telegram");
    assert_eq!(request.channel_id, "777");
    assert_eq!(request.thread_id.as_deref(), Some("42"));

    engine
        .complete_gateway_send_result(GatewaySendResult {
            correlation_id: request.correlation_id.clone(),
            platform: "telegram".to_string(),
            channel_id: "777".to_string(),
            requested_channel_id: Some("777".to_string()),
            delivery_id: Some("99".to_string()),
            ok: true,
            error: None,
            completed_at_ms: 1,
        })
        .await;

    let result = send_task
        .await
        .expect("send task should join")
        .expect("send should succeed");
    assert_eq!(result, "Telegram message sent to 777");
}

use super::{classify_freshness, classify_source_authority, format_result_with_authority};

#[test]
fn classify_source_authority_official_rust_docs() {
    assert_eq!(
        classify_source_authority("https://docs.rust-lang.org/book/"),
        "official"
    );
}

#[test]
fn classify_source_authority_community_stackoverflow() {
    assert_eq!(
        classify_source_authority("https://stackoverflow.com/questions/123"),
        "community"
    );
}

#[test]
fn classify_source_authority_unknown_random_site() {
    assert_eq!(
        classify_source_authority("https://random-site.example.com"),
        "unknown"
    );
}

#[test]
fn classify_source_authority_official_mdn() {
    assert_eq!(
        classify_source_authority("https://developer.mozilla.org/en-US/docs"),
        "official"
    );
}

#[test]
fn classify_source_authority_community_reddit() {
    assert_eq!(
        classify_source_authority("https://reddit.com/r/rust"),
        "community"
    );
}

#[test]
fn classify_source_authority_community_medium() {
    assert_eq!(
        classify_source_authority("https://medium.com/@author/article"),
        "community"
    );
}

#[test]
fn classify_source_authority_official_cppreference() {
    assert_eq!(
        classify_source_authority("https://cppreference.com/w/cpp"),
        "official"
    );
}

#[test]
fn classify_source_authority_empty_string_no_panic() {
    assert_eq!(classify_source_authority(""), "unknown");
}

#[test]
fn format_result_with_authority_prepends_official_tag() {
    let result = format_result_with_authority(
        "Rust Book",
        "https://docs.rust-lang.org/book/",
        "The Rust Programming Language",
    );
    assert!(result.starts_with("- [official]"));
    assert!(result.contains("**Rust Book**"));
    assert!(result.contains("https://docs.rust-lang.org/book/"));
    assert!(result.contains("The Rust Programming Language"));
    assert!(
        result.contains("freshness:"),
        "research result formatting should expose freshness alongside source authority"
    );
}

#[test]
fn classify_freshness_labels_recent_stale_and_old_dates() {
    let today = chrono::Utc::now().date_naive();
    let recent = (today - chrono::Days::new(7))
        .format("%Y-%m-%d")
        .to_string();
    let stale = (today - chrono::Days::new(90))
        .format("%Y-%m-%d")
        .to_string();
    let old = (today - chrono::Days::new(500))
        .format("%Y-%m-%d")
        .to_string();

    assert_eq!(classify_freshness(Some(&recent)), "recent");
    assert_eq!(classify_freshness(Some(&stale)), "stale");
    assert_eq!(classify_freshness(Some(&old)), "old");
    assert_eq!(classify_freshness(Some("not-a-date")), "unknown");
    assert_eq!(classify_freshness(None), "unknown");
}

#[tokio::test]
async fn spawn_subagent_does_not_require_todo_bootstrap_for_chat_threads() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);
    let thread_id = "thread-planned-subagents";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
                thread_id.to_string(),
                crate::agent::types::AgentThread {
                    id: thread_id.to_string(),
                    agent_name: None,
                    title: "Parallel skill work".to_string(),
                    messages: vec![crate::agent::types::AgentMessage::user(
                        "Investigate the failing tests, then update the parser, and finally rerun the suite.",
                        1,
                    )],
                    pinned: false,
                    upstream_thread_id: None,
                    upstream_transport: None,
                    upstream_provider: None,
                    upstream_model: None,
                    upstream_assistant_id: None,
                    total_input_tokens: 0,
                    total_output_tokens: 0,
                    created_at: 1,
                    updated_at: 1,
                },
            );
    }

    let tool_call = ToolCall::with_default_weles_review(
            "tool-spawn-subagent-bootstrap".to_string(),
            ToolFunction {
                name: "spawn_subagent".to_string(),
                arguments: serde_json::json!({
                    "title": "Write foundational skill files",
                    "description": "Create the foundational skill files in parallel so the parent can integrate the batches."
                })
                .to_string(),
            },
        );

    let result = execute_tool(
        &tool_call,
        &engine,
        thread_id,
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(
        !result.is_error,
        "spawn_subagent should run from a chat thread without forcing plan bootstrap: {}",
        result.content
    );
    assert!(result.content.contains("Spawned subagent"));

    let todos = engine.get_todos(thread_id).await;
    assert!(
        todos.is_empty(),
        "chat-thread subagent spawning should not auto-bootstrap todos"
    );

    let tasks = engine.list_tasks().await;
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].source, "subagent");
    assert_eq!(tasks[0].parent_thread_id.as_deref(), Some(thread_id));
}

#[tokio::test]
async fn start_goal_run_tool_creates_goal_and_list_goal_runs_returns_it() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);
    let thread_id = "thread-start-goal-run-tool";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            crate::agent::types::AgentThread {
                id: thread_id.to_string(),
                agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
                title: "Goal launch thread".to_string(),
                messages: vec![crate::agent::types::AgentMessage::user(
                    "Launch a real durable goal run here.",
                    1,
                )],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: 1,
                updated_at: 1,
            },
        );
    }

    let start_call = ToolCall::with_default_weles_review(
        "tool-start-goal-run".to_string(),
        ToolFunction {
            name: "start_goal_run".to_string(),
            arguments: serde_json::json!({
                "goal": "Ship the release with full verification",
                "title": "Release Goal",
                "priority": "high"
            })
            .to_string(),
        },
    );

    let start_result = execute_tool(
        &start_call,
        &engine,
        thread_id,
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(
        !start_result.is_error,
        "start_goal_run should succeed: {}",
        start_result.content
    );
    let started_goal: serde_json::Value =
        serde_json::from_str(&start_result.content).expect("parse started goal run");
    let goal_run_id = started_goal
        .get("id")
        .and_then(|value| value.as_str())
        .expect("started goal run should expose id");
    let goal_thread_id = started_goal
        .get("thread_id")
        .and_then(|value| value.as_str())
        .expect("started goal run should expose its dedicated goal thread");
    assert_ne!(
        goal_thread_id, thread_id,
        "goals started from a tool call should isolate themselves onto a fresh thread"
    );

    let goal_thread = engine
        .get_thread(goal_thread_id)
        .await
        .expect("goal thread should exist");
    assert_eq!(
        goal_thread.upstream_thread_id.as_deref(),
        Some(thread_id),
        "goal thread should keep lineage to the spawning thread"
    );
    assert!(
        goal_thread.messages.iter().any(|message| {
            message.role == crate::agent::types::MessageRole::System
                && message
                    .content
                    .contains("Ship the release with full verification")
                && message.content.contains(thread_id)
        }),
        "goal thread should be seeded with source-thread and goal-context guidance"
    );

    let list_call = ToolCall::with_default_weles_review(
        "tool-list-goal-runs".to_string(),
        ToolFunction {
            name: "list_goal_runs".to_string(),
            arguments: serde_json::json!({}).to_string(),
        },
    );

    let list_result = execute_tool(
        &list_call,
        &engine,
        thread_id,
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(
        !list_result.is_error,
        "list_goal_runs should succeed: {}",
        list_result.content
    );
    let goal_runs_page: serde_json::Value =
        serde_json::from_str(&list_result.content).expect("parse goal run list page");
    assert_eq!(goal_runs_page["offset"].as_u64(), Some(0));
    assert_eq!(goal_runs_page["returned"].as_u64(), Some(1));
    assert_eq!(goal_runs_page["total"].as_u64(), Some(1));
    let goal_runs = goal_runs_page["items"]
        .as_array()
        .expect("goal run page should expose items");
    assert!(
        goal_runs.iter().any(
            |goal_run| goal_run.get("id").and_then(|value| value.as_str()) == Some(goal_run_id)
        ),
        "list_goal_runs should include the started goal run"
    );
}

#[tokio::test]
async fn list_goal_runs_tool_returns_paged_result_with_total_count() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);
    let thread_id = "thread-list-goal-runs-page";

    for index in 0..3 {
        let call = ToolCall::with_default_weles_review(
            format!("tool-start-goal-run-{index}"),
            ToolFunction {
                name: "start_goal_run".to_string(),
                arguments: serde_json::json!({
                    "goal": format!("Goal {index}"),
                    "title": format!("Goal {index}")
                })
                .to_string(),
            },
        );
        let result = execute_tool(
            &call,
            &engine,
            thread_id,
            None,
            &manager,
            None,
            &event_tx,
            root.path(),
            &engine.http_client,
            None,
        )
        .await;
        assert!(
            !result.is_error,
            "start_goal_run should succeed: {}",
            result.content
        );
    }

    let list_call = ToolCall::with_default_weles_review(
        "tool-list-goal-runs-paged".to_string(),
        ToolFunction {
            name: "list_goal_runs".to_string(),
            arguments: serde_json::json!({ "limit": 1, "offset": 1 }).to_string(),
        },
    );
    let result = execute_tool(
        &list_call,
        &engine,
        thread_id,
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(
        !result.is_error,
        "list_goal_runs should succeed: {}",
        result.content
    );
    let page: serde_json::Value =
        serde_json::from_str(&result.content).expect("list_goal_runs should return page JSON");
    assert_eq!(page["total"].as_u64(), Some(3));
    assert_eq!(page["limit"].as_u64(), Some(1));
    assert_eq!(page["offset"].as_u64(), Some(1));
    assert_eq!(page["returned"].as_u64(), Some(1));
    assert_eq!(page["has_more"].as_bool(), Some(true));
    assert_eq!(page["next_offset"].as_u64(), Some(2));
    assert_eq!(
        page["items"].as_array().map(Vec::len),
        Some(1),
        "only the requested page should be returned"
    );
}

#[tokio::test]
async fn start_goal_run_tool_inherits_client_surface_from_source_thread() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);
    let thread_id = "thread-start-goal-run-tool-tui-surface";
    engine
        .set_thread_client_surface(thread_id, zorai_protocol::ClientSurface::Tui)
        .await;

    let start_call = ToolCall::with_default_weles_review(
        "tool-start-goal-run-tui-surface".to_string(),
        ToolFunction {
            name: "start_goal_run".to_string(),
            arguments: serde_json::json!({
                "goal": "Keep TUI shell routing for a delegated durable goal",
                "title": "TUI Routed Goal"
            })
            .to_string(),
        },
    );

    let start_result = execute_tool(
        &start_call,
        &engine,
        thread_id,
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(
        !start_result.is_error,
        "start_goal_run should succeed: {}",
        start_result.content
    );
    let started_goal: serde_json::Value =
        serde_json::from_str(&start_result.content).expect("parse started goal run");
    let goal_run_id = started_goal
        .get("id")
        .and_then(|value| value.as_str())
        .expect("started goal run should expose id");
    let goal_thread_id = started_goal
        .get("thread_id")
        .and_then(|value| value.as_str())
        .expect("started goal run should expose dedicated goal thread");

    assert_eq!(
        engine.get_thread_client_surface(goal_thread_id).await,
        Some(zorai_protocol::ClientSurface::Tui),
        "dedicated goal thread should inherit the source thread client surface"
    );
    assert_eq!(
        engine.get_goal_run_client_surface(goal_run_id).await,
        Some(zorai_protocol::ClientSurface::Tui),
        "goal run should retain the source client surface for task fallback routing"
    );
}

#[tokio::test]
async fn start_goal_run_tool_defaults_to_agent_authored_auto_approval() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);
    let thread_id = "thread-start-goal-run-tool-auto-approval";

    let start_call = ToolCall::with_default_weles_review(
        "tool-start-goal-run-auto-approval".to_string(),
        ToolFunction {
            name: "start_goal_run".to_string(),
            arguments: serde_json::json!({
                "goal": "Let the responsible agent finish this independently",
                "title": "Agent-owned Goal"
            })
            .to_string(),
        },
    );

    let start_result = execute_tool(
        &start_call,
        &engine,
        thread_id,
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(
        !start_result.is_error,
        "start_goal_run should succeed: {}",
        start_result.content
    );
    let started_goal: serde_json::Value =
        serde_json::from_str(&start_result.content).expect("parse started goal run");
    assert_eq!(
        started_goal["authorship_tag"], "agent",
        "tool-created goals should default to agent authorship for auto-approval"
    );
}

#[tokio::test]
async fn start_goal_run_tool_forwards_launch_assignments_to_goal_snapshot() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);
    let thread_id = "thread-start-goal-run-tool-assignments";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            crate::agent::types::AgentThread {
                id: thread_id.to_string(),
                agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
                title: "Goal launch thread".to_string(),
                messages: vec![crate::agent::types::AgentMessage::user(
                    "Launch a durable goal with a researcher.",
                    1,
                )],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: 1,
                updated_at: 1,
            },
        );
    }

    let start_call = ToolCall::with_default_weles_review(
        "tool-start-goal-run-assignments".to_string(),
        ToolFunction {
            name: "start_goal_run".to_string(),
            arguments: serde_json::json!({
                "goal": "Research and implement the release checklist",
                "title": "Release Research Goal",
                "launch_assignments": [
                    {
                        "role_id": "swarog",
                        "enabled": true,
                        "provider": "openai",
                        "model": "gpt-5.4",
                        "reasoning_effort": "medium",
                        "inherit_from_main": false
                    },
                    {
                        "role_id": "researcher",
                        "enabled": true,
                        "provider": "alibaba-coding-plan",
                        "model": "qwen3.6-plus",
                        "reasoning_effort": "high",
                        "inherit_from_main": false
                    }
                ]
            })
            .to_string(),
        },
    );

    let start_result = execute_tool(
        &start_call,
        &engine,
        thread_id,
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(
        !start_result.is_error,
        "start_goal_run should accept launch assignments: {}",
        start_result.content
    );
    let started_goal: serde_json::Value =
        serde_json::from_str(&start_result.content).expect("parse started goal run");
    let assignments = started_goal["launch_assignment_snapshot"]
        .as_array()
        .expect("started goal should include launch assignment snapshot");
    let researcher = assignments
        .iter()
        .find(|assignment| assignment["role_id"] == "researcher")
        .expect("tool-provided researcher assignment should be visible in the launch snapshot");
    assert_eq!(researcher["provider"], "alibaba-coding-plan");
    assert_eq!(researcher["model"], "qwen3.6-plus");
    assert!(
        assignments
            .iter()
            .any(|assignment| assignment["role_id"] == "reviewer"),
        "reviewer should still be injected when missing"
    );
}

#[tokio::test]
async fn spawn_subagent_reserves_thread_id_immediately() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);
    let thread_id = "thread-parent";
    let parent_task = engine
        .enqueue_task(
            "Parent goal task".to_string(),
            "Coordinate child work".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "goal_run",
            Some("goal-spawn-1".to_string()),
            None,
            None,
            Some("daemon".to_string()),
        )
        .await;

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            crate::agent::types::AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: "Parent".to_string(),
                messages: vec![crate::agent::types::AgentMessage::user(
                    "Delegate this to a child agent.",
                    1,
                )],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: 1,
                updated_at: 1,
            },
        );
    }

    let result = super::execute_spawn_subagent(
        &serde_json::json!({
            "title": "Reserved child thread",
            "description": "Reserve the child thread immediately."
        }),
        &engine,
        thread_id,
        Some(&parent_task.id),
        &manager,
        None,
        &event_tx,
    )
    .await
    .expect("spawn_subagent should succeed");

    let tasks = engine.list_tasks().await;
    assert_eq!(tasks.len(), 2);
    let spawned_task = tasks
        .iter()
        .find(|task| task.id != parent_task.id)
        .expect("spawned subagent task should exist");
    let reserved_thread_id = spawned_task
        .thread_id
        .clone()
        .expect("spawn_subagent should reserve a thread id immediately");
    assert!(
        reserved_thread_id.starts_with("thread_"),
        "reserved thread id should use the standard thread prefix"
    );
    assert!(
        result.contains(&reserved_thread_id),
        "spawn_subagent result should surface the reserved thread id"
    );
    assert_eq!(spawned_task.parent_thread_id.as_deref(), Some(thread_id));
    assert_eq!(
        spawned_task.parent_task_id.as_deref(),
        Some(parent_task.id.as_str())
    );
    assert_eq!(spawned_task.goal_run_id.as_deref(), Some("goal-spawn-1"));

    let persisted_thread = engine
        .history
        .get_thread(&reserved_thread_id)
        .await
        .expect("read persisted child thread")
        .expect("reserved child thread should be persisted immediately");
    let metadata: serde_json::Value = serde_json::from_str(
        persisted_thread
            .metadata_json
            .as_deref()
            .expect("reserved child thread should persist metadata"),
    )
    .expect("reserved child thread metadata should be valid JSON");
    assert_eq!(metadata["thread_id"], serde_json::json!(reserved_thread_id));
    assert_eq!(metadata["parent_thread_id"], serde_json::json!(thread_id));
    assert_eq!(
        metadata["parent_task_id"],
        serde_json::json!(parent_task.id)
    );
    assert_eq!(metadata["goal_run_id"], serde_json::json!("goal-spawn-1"));
    assert_eq!(metadata["identity"]["thread_id"], metadata["thread_id"]);
    assert_eq!(
        metadata["identity"]["parent_thread_id"],
        metadata["parent_thread_id"]
    );
    assert_eq!(metadata["identity"]["goal_run_id"], metadata["goal_run_id"]);
}

#[tokio::test]
async fn spawn_subagent_inherits_goal_from_persisted_parent_task() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);
    let thread_id = "thread-persisted-parent-subagent";
    let parent_task = engine
        .enqueue_task(
            "Persisted parent goal task".to_string(),
            "Coordinate persisted child work".to_string(),
            "normal",
            None,
            Some("session-parent".to_string()),
            Vec::new(),
            None,
            "goal_run",
            Some("goal-persisted-spawn-1".to_string()),
            None,
            Some(thread_id.to_string()),
            Some("daemon".to_string()),
        )
        .await;
    engine.persist_tasks().await;
    engine.tasks.lock().await.clear();

    let result = super::execute_spawn_subagent(
        &serde_json::json!({
            "title": "Persisted parent child",
            "description": "Inherit goal metadata from a parent task that only exists in history."
        }),
        &engine,
        thread_id,
        Some(&parent_task.id),
        &manager,
        None,
        &event_tx,
    )
    .await
    .expect("spawn_subagent should resolve the persisted parent task");

    assert!(result.contains("Spawned subagent"));
    let spawned = engine
        .list_tasks_filtered(&crate::history::AgentTaskListQuery {
            id: None,
            status: None,
            statuses: Vec::new(),
            source: Some("subagent".to_string()),
            thread_id: None,
            thread_ids: Vec::new(),
            goal_run_id: None,
            parent_task_id: Some(parent_task.id.clone()),
            awaiting_approval_id: None,
            supervisor_config_present: false,
            exclude_terminal_statuses: false,
            order_by_recent_activity_desc: false,
            limit: Some(1),
            ids: Vec::new(),
            parent_task_ids: Vec::new(),
        })
        .await
        .into_iter()
        .next()
        .expect("spawned subagent should be persisted with the parent task id");
    assert_eq!(
        spawned.goal_run_id.as_deref(),
        Some("goal-persisted-spawn-1"),
        "spawned subagent should inherit goal metadata from the persisted parent"
    );
}

#[tokio::test]
async fn spawn_subagent_reserved_thread_inherits_parent_client_surface() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);
    let thread_id = "thread-parent-tui-subagent";
    engine
        .set_thread_client_surface(thread_id, zorai_protocol::ClientSurface::Tui)
        .await;

    let result = super::execute_spawn_subagent(
        &serde_json::json!({
            "title": "TUI child thread",
            "description": "Reserve a child thread that keeps the parent client surface."
        }),
        &engine,
        thread_id,
        None,
        &manager,
        None,
        &event_tx,
    )
    .await
    .expect("spawn_subagent should succeed");

    let tasks = engine.list_tasks().await;
    let reserved_thread_id = tasks[0]
        .thread_id
        .clone()
        .expect("spawn_subagent should reserve a thread id immediately");
    assert!(
        result.contains(&reserved_thread_id),
        "spawn_subagent result should surface the reserved thread id"
    );
    assert_eq!(
        engine.get_thread_client_surface(&reserved_thread_id).await,
        Some(zorai_protocol::ClientSurface::Tui),
        "spawned subagent thread should inherit the parent thread client surface"
    );
}

#[tokio::test]
async fn spawn_subagent_seeds_reserved_thread_with_spawned_persona_identity() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);

    let result = super::execute_spawn_subagent(
        &serde_json::json!({
            "title": "Persona seeded thread",
            "description": "Reserve a child thread with the spawned persona identity."
        }),
        &engine,
        "thread-parent",
        None,
        &manager,
        None,
        &event_tx,
    )
    .await
    .expect("spawn_subagent should succeed");

    let task = engine
        .list_tasks()
        .await
        .into_iter()
        .find(|task| result.contains(&task.id))
        .expect("spawned subagent should exist");
    let reserved_thread_id = task
        .thread_id
        .clone()
        .expect("spawned subagent should reserve a thread id");
    let persona_prompt = task
        .override_system_prompt
        .as_deref()
        .expect("spawned subagent should carry a persona prompt");
    let persona_name = persona_prompt
        .lines()
        .find_map(|line| {
            line.trim()
                .strip_prefix("Agent persona:")
                .map(str::trim)
                .map(str::to_string)
        })
        .expect("persona prompt should expose the persona name");
    let persona_id = persona_prompt
        .lines()
        .find_map(|line| {
            line.trim()
                .strip_prefix("Agent persona id:")
                .map(str::trim)
                .map(str::to_string)
        })
        .expect("persona prompt should expose the persona id");

    let thread = engine
        .get_thread(&reserved_thread_id)
        .await
        .expect("reserved child thread should be created immediately");
    assert_eq!(thread.agent_name.as_deref(), Some(persona_name.as_str()));

    let handoff_state = engine
        .thread_handoff_state(&reserved_thread_id)
        .await
        .expect("reserved child thread should seed handoff state");
    assert_eq!(handoff_state.active_agent_id, persona_id);
    assert_eq!(handoff_state.responder_stack.len(), 1);
    assert_eq!(
        handoff_state.responder_stack[0].agent_name, persona_name,
        "reserved child thread should keep the seeded responder name"
    );
}

#[tokio::test]
async fn spawn_subagent_assigns_unique_persona_per_parent_scope() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);
    let thread_id = "thread-persona-unique";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            crate::agent::types::AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: "Parent".to_string(),
                messages: vec![crate::agent::types::AgentMessage::user(
                    "Spawn two children.",
                    1,
                )],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: 1,
                updated_at: 1,
            },
        );
    }

    let first = super::execute_spawn_subagent(
        &serde_json::json!({
            "title": "First worker",
            "description": "First child task."
        }),
        &engine,
        thread_id,
        None,
        &manager,
        None,
        &event_tx,
    )
    .await
    .expect("first spawn should succeed");
    let second = super::execute_spawn_subagent(
        &serde_json::json!({
            "title": "Second worker",
            "description": "Second child task."
        }),
        &engine,
        thread_id,
        None,
        &manager,
        None,
        &event_tx,
    )
    .await
    .expect("second spawn should succeed");

    let persona_id_for = |result: String| {
        let engine = engine.clone();
        async move {
            let tasks = engine.list_tasks().await;
            let task = tasks
                .iter()
                .find(|task| result.contains(&task.id))
                .expect("spawned subagent should exist");
            let prompt = task
                .override_system_prompt
                .as_deref()
                .expect("spawned subagent should carry a persona prompt");
            prompt
                .lines()
                .find_map(|line: &str| {
                    line.trim()
                        .strip_prefix("Agent persona id:")
                        .map(str::trim)
                        .map(str::to_string)
                })
                .expect("persona prompt should expose the persona id")
        }
    };

    let first_persona = persona_id_for(first.clone()).await;
    let second_persona = persona_id_for(second.clone()).await;
    assert_ne!(
        first_persona, second_persona,
        "two concurrent subagents under one parent must never share a persona name"
    );
}

#[tokio::test]
async fn message_agent_routes_sibling_dm_to_active_task_thread() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);
    let thread_id = "thread-sibling-dm";

    let parent = engine
        .enqueue_task(
            "Parent coordinator".to_string(),
            "Coordinate the child work".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "user",
            None,
            None,
            Some(thread_id.to_string()),
            Some("daemon".to_string()),
        )
        .await;

    let spawned = super::execute_spawn_subagent(
        &serde_json::json!({
            "title": "Sibling worker",
            "description": "Child task that will receive a sibling DM."
        }),
        &engine,
        thread_id,
        Some(&parent.id),
        &manager,
        None,
        &event_tx,
    )
    .await
    .expect("spawn should succeed");
    let sibling_task = engine
        .list_tasks()
        .await
        .into_iter()
        .find(|task| spawned.contains(&task.id))
        .expect("spawned sibling task should exist");
    let sibling_thread_id = sibling_task
        .thread_id
        .clone()
        .expect("sibling task should have a reserved thread");
    let sibling_persona_id = crate::agent::agent_identity::extract_persona_id(
        sibling_task.override_system_prompt.as_deref(),
    )
    .expect("sibling task should carry a persona id");

    // Simulate the sibling actively working: a live (non-cancelled) stream on
    // its reserved thread makes the post-DM flush defer the continuation so
    // it stays observable in the queue (it would otherwise drain into an
    // immediate LLM turn attempt).
    {
        let mut streams = engine.stream_cancellations.lock().await;
        streams.insert(
            sibling_thread_id.clone(),
            StreamCancellationEntry {
                generation: 1,
                token: CancellationToken::new(),
                retry_now: Arc::new(tokio::sync::Notify::new()),
                started_at: 1,
                last_progress_at: 1,
                last_progress_kind: StreamProgressKind::Started,
                last_progress_excerpt: String::new(),
            },
        );
    }

    let tool_call = ToolCall::with_default_weles_review(
        "tool-sibling-dm".to_string(),
        ToolFunction {
            name: "message_agent".to_string(),
            arguments: serde_json::json!({
                "target": sibling_persona_id,
                "message": "Here are my results; integrate them into your work."
            })
            .to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        thread_id,
        Some(&parent.id),
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(
        !result.is_error,
        "sibling DM should route to the active task thread: {}",
        result.content
    );
    let payload: serde_json::Value =
        serde_json::from_str(&result.content).expect("sibling DM result should be JSON");
    assert_eq!(
        payload["sibling_task_id"],
        serde_json::json!(sibling_task.id)
    );
    assert_eq!(payload["thread_id"], serde_json::json!(sibling_thread_id));
    assert!(
        !engine
            .threads
            .read()
            .await
            .contains_key(&format!("dm:{}", sibling_persona_id)),
        "no context-free internal-dm thread should be created for sibling routing"
    );
    let queued = engine
        .deferred_visible_thread_continuations_for(&sibling_thread_id)
        .await;
    assert!(
        !queued.is_empty(),
        "sibling DM should queue a continuation on the sibling task thread"
    );
}

#[tokio::test]
async fn message_agent_rejects_spawned_subagent_dm_to_unused_global_persona() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);
    let thread_id = "thread-sibling-dm-reject";

    let parent = engine
        .enqueue_task(
            "Parent coordinator".to_string(),
            "Coordinate the child work".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "user",
            None,
            None,
            Some(thread_id.to_string()),
            Some("daemon".to_string()),
        )
        .await;
    let child = engine
        .enqueue_task(
            "Child sender".to_string(),
            "Child task that will try a bad DM.".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            Some(parent.id.clone()),
            Some(thread_id.to_string()),
            Some("daemon".to_string()),
        )
        .await;

    let tool_call = ToolCall::with_default_weles_review(
        "tool-sibling-dm-reject".to_string(),
        ToolFunction {
            name: "message_agent".to_string(),
            arguments: serde_json::json!({
                "target": "swietowit",
                "message": "Random global persona ping."
            })
            .to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        thread_id,
        Some(&child.id),
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(
        result.is_error,
        "spawned subagent DM to a persona with no active sibling should be rejected: {}",
        result.content
    );
    assert!(
        result.content.contains("no active sibling subagent named")
            || result
                .content
                .contains("only internal-DM their active siblings"),
        "rejection should explain scope rules: {}",
        result.content
    );
}

#[tokio::test]
async fn read_peer_memory_honors_explicit_parent_task_id_without_current_task() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.collaboration.enabled = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let parent = engine
        .enqueue_task(
            "Parent coordinator".to_string(),
            "Coordinate the child work".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "user",
            None,
            None,
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    let child_a = engine
        .enqueue_task(
            "Research child".to_string(),
            "Inspect deployment risks".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            Some(parent.id.clone()),
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    let child_b = engine
        .enqueue_task(
            "Review child".to_string(),
            "Cross-check deployment risks".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            Some(parent.id.clone()),
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;

    engine
        .register_subagent_collaboration(&parent.id, &child_a)
        .await;
    engine
        .register_subagent_collaboration(&parent.id, &child_b)
        .await;
    engine
        .record_collaboration_contribution(
            &parent.id,
            &child_a.id,
            "deploy",
            "recommend staged rollout",
            vec!["smoke tests are stable".to_string()],
            0.82,
        )
        .await
        .expect("seed contribution should succeed");

    let result = super::execute_read_peer_memory(
        &serde_json::json!({ "parent_task_id": parent.id }),
        &engine,
        None,
    )
    .await
    .expect("explicit parent scope should work without a current task");

    let report: serde_json::Value =
        serde_json::from_str(&result).expect("result should be valid json");
    assert_eq!(
        report["shared_context"]
            .as_array()
            .expect("shared_context should be an array")
            .len(),
        1
    );
    assert_eq!(
        report["shared_context"][0]["task_id"],
        serde_json::Value::String(child_a.id.clone())
    );
    assert_eq!(
        report["peers"]
            .as_array()
            .expect("peers should be an array")
            .len(),
        2
    );
}

#[tokio::test]
async fn broadcast_contribution_honors_explicit_parent_task_id_without_current_task() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.collaboration.enabled = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let parent = engine
        .enqueue_task(
            "Parent coordinator".to_string(),
            "Coordinate the child work".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "user",
            None,
            None,
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    let child = engine
        .enqueue_task(
            "Research child".to_string(),
            "Inspect deployment risks".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            Some(parent.id.clone()),
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;

    engine
        .register_subagent_collaboration(&parent.id, &child)
        .await;

    let result = super::execute_broadcast_contribution(
        &serde_json::json!({
            "parent_task_id": parent.id,
            "topic": "deploy",
            "position": "hold until the migration completes",
            "evidence": ["operator wants to preserve retrieval visibility"],
            "confidence": 0.95
        }),
        &engine,
        "thread-parent",
        None,
    )
    .await
    .expect("explicit parent scope should allow parent-context contributions");

    let report: serde_json::Value =
        serde_json::from_str(&result).expect("result should be valid json");
    assert_eq!(report["contribution"]["task_id"], "operator");
    assert_eq!(report["contribution"]["topic"], "deploy");

    let session = engine
        .collaboration_sessions_json(Some(&parent.id))
        .await
        .expect("session lookup should succeed");
    assert_eq!(
        session["contributions"]
            .as_array()
            .unwrap_or(&Vec::new())
            .len(),
        1
    );
}

#[tokio::test]
async fn broadcast_contribution_uses_persisted_current_task_parent_after_live_queue_clear() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.collaboration.enabled = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let parent = engine
        .enqueue_task(
            "Parent coordinator".to_string(),
            "Coordinate the child work".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "user",
            None,
            None,
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    let child = engine
        .enqueue_task(
            "Research child".to_string(),
            "Inspect deployment risks".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            Some(parent.id.clone()),
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;

    engine
        .register_subagent_collaboration(&parent.id, &child)
        .await;
    engine.persist_tasks().await;
    engine.tasks.lock().await.clear();

    let result = super::execute_broadcast_contribution(
        &serde_json::json!({
            "topic": "deploy",
            "position": "ship after smoke tests",
            "evidence": ["child task was persisted"],
            "confidence": 0.91
        }),
        &engine,
        "thread-parent",
        Some(&child.id),
    )
    .await
    .expect("persisted current task should provide parent context");

    let report: serde_json::Value =
        serde_json::from_str(&result).expect("result should be valid json");
    assert_eq!(report["contribution"]["task_id"], child.id);
    assert_eq!(report["contribution"]["topic"], "deploy");
}

#[tokio::test]
async fn dispatch_via_bid_protocol_tool_routes_through_collaboration_runtime() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.collaboration.enabled = true;
    let engine = AgentEngine::new_test(manager.clone(), config, root.path()).await;
    let (event_tx, _) = broadcast::channel(8);

    let parent = engine
        .enqueue_task(
            "Parent coordinator".to_string(),
            "Choose the best owner for the next workstream".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "user",
            None,
            None,
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    let child_a = engine
        .enqueue_task(
            "Research child".to_string(),
            "Prepare a bid for implementation ownership".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            Some(parent.id.clone()),
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    let child_b = engine
        .enqueue_task(
            "Review child".to_string(),
            "Prepare a bid for review ownership".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            Some(parent.id.clone()),
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;

    engine
        .register_subagent_collaboration(&parent.id, &child_a)
        .await;
    engine
        .register_subagent_collaboration(&parent.id, &child_b)
        .await;

    let tool_call = ToolCall::with_default_weles_review(
        "tool-dispatch-via-bid-protocol".to_string(),
        ToolFunction {
            name: "dispatch_via_bid_protocol".to_string(),
            arguments: serde_json::json!({
                "parent_task_id": parent.id,
                "bids": [
                    {
                        "task_id": child_b.id,
                        "confidence": 0.93,
                        "availability": "busy"
                    },
                    {
                        "task_id": child_a.id,
                        "confidence": 0.74,
                        "availability": "available"
                    }
                ]
            })
            .to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        "thread-parent",
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(
        !result.is_error,
        "dispatch_via_bid_protocol tool should succeed: {}",
        result.content
    );
    let payload: serde_json::Value =
        serde_json::from_str(&result.content).expect("tool result should be valid json");
    assert_eq!(payload["primary_task_id"], child_a.id);
    assert_eq!(payload["reviewer_task_id"], child_b.id);

    let session = engine
        .collaboration_sessions_json(Some(&parent.id))
        .await
        .expect("session lookup should succeed");
    assert_eq!(session["role_assignment"]["primary_task_id"], child_a.id);
    assert_eq!(session["role_assignment"]["reviewer_task_id"], child_b.id);
}

#[tokio::test]
async fn dispatch_via_bid_protocol_tool_bootstraps_collaboration_agents_before_role_assignment() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.collaboration.enabled = true;
    let engine = AgentEngine::new_test(manager.clone(), config, root.path()).await;
    let (event_tx, _) = broadcast::channel(8);

    let parent = engine
        .enqueue_task(
            "Parent coordinator".to_string(),
            "Choose the best owner for the next workstream".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "user",
            None,
            None,
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    let child_a = engine
        .enqueue_task(
            "Research child".to_string(),
            "Prepare a bid for implementation ownership".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            Some(parent.id.clone()),
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    let child_b = engine
        .enqueue_task(
            "Review child".to_string(),
            "Prepare a bid for review ownership".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            Some(parent.id.clone()),
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;

    engine.tasks.lock().await.clear();
    let persisted_parent = engine
        .history
        .list_agent_tasks_filtered(&crate::history::AgentTaskListQuery {
            id: Some(parent.id.clone()),
            limit: Some(1),
            ..Default::default()
        })
        .await
        .expect("parent task query should succeed");
    assert_eq!(
        persisted_parent.len(),
        1,
        "fixture must persist the parent task before clearing live queue"
    );

    let tool_call = ToolCall::with_default_weles_review(
        "tool-dispatch-via-bid-protocol-bootstrap".to_string(),
        ToolFunction {
            name: "dispatch_via_bid_protocol".to_string(),
            arguments: serde_json::json!({
                "parent_task_id": parent.id,
                "bids": [
                    {
                        "task_id": child_b.id,
                        "confidence": 0.63,
                        "availability": "available"
                    },
                    {
                        "task_id": child_a.id,
                        "confidence": 0.88,
                        "availability": "available"
                    }
                ]
            })
            .to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        "thread-parent",
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(
        !result.is_error,
        "dispatch_via_bid_protocol should bootstrap collaboration agents: {}",
        result.content
    );

    let payload: serde_json::Value =
        serde_json::from_str(&result.content).expect("tool result should be valid json");
    assert_eq!(payload["primary_task_id"], child_a.id);
    assert_eq!(payload["reviewer_task_id"], child_b.id);

    let session = engine
        .collaboration_sessions_json(Some(&parent.id))
        .await
        .expect("session lookup should succeed");
    assert_eq!(
        session["agents"].as_array().map(|items| items.len()),
        Some(2),
        "eligible subagents should be bootstrapped into the collaboration session"
    );
    assert_eq!(session["role_assignment"]["primary_task_id"], child_a.id);
    assert_eq!(session["role_assignment"]["reviewer_task_id"], child_b.id);
}

async fn spawn_model_fetch_server(models_body: serde_json::Value) -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind model fetch server");
    let address = listener
        .local_addr()
        .expect("read model fetch listener address");
    let response_body = serde_json::to_string(&models_body).expect("serialize models body");

    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            let body = response_body.clone();
            tokio::spawn(async move {
                let mut buffer = [0u8; 2048];
                let _ = stream.read(&mut buffer).await;
                let response = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                let _ = stream.write_all(response.as_bytes()).await;
                let _ = stream.shutdown().await;
            });
        }
    });

    format!("http://{}", address)
}

#[tokio::test]
async fn fetch_authenticated_providers_returns_only_authenticated_entries() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = zorai_shared::providers::PROVIDER_ID_OPENAI.to_string();
    config.api_key = "test-key".to_string();
    config.base_url = "https://api.openai.example/v1".to_string();
    let engine = AgentEngine::new_test(manager.clone(), config, root.path()).await;
    let (event_tx, _) = broadcast::channel(8);

    let tool_call = ToolCall::with_default_weles_review(
        "tool-fetch-authenticated-providers".to_string(),
        ToolFunction {
            name: "fetch_authenticated_providers".to_string(),
            arguments: serde_json::json!({}).to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        "thread-provider-discovery",
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(
        !result.is_error,
        "provider discovery should succeed: {}",
        result.content
    );
    let providers: Vec<crate::agent::types::ProviderAuthState> =
        serde_json::from_str(&result.content).expect("parse authenticated provider list");
    assert!(providers.iter().all(|provider| provider.authenticated));
    assert!(providers
        .iter()
        .any(|provider| { provider.provider_id == zorai_shared::providers::PROVIDER_ID_OPENAI }));
}

#[tokio::test]
async fn fetch_provider_models_uses_authenticated_provider_config() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = zorai_shared::providers::PROVIDER_ID_OPENAI.to_string();
    config.api_key = "test-key".to_string();
    config.base_url = spawn_model_fetch_server(serde_json::json!({
        "data": [
            {"id": "gpt-4.1", "name": "GPT-4.1", "context_length": 1048576},
            {"id": "gpt-5.4", "name": "GPT-5.4", "context_length": 1048576}
        ]
    }))
    .await;
    let engine = AgentEngine::new_test(manager.clone(), config, root.path()).await;
    let (event_tx, _) = broadcast::channel(8);

    let tool_call = ToolCall::with_default_weles_review(
        "tool-fetch-provider-models".to_string(),
        ToolFunction {
            name: "fetch_provider_models".to_string(),
            arguments: serde_json::json!({
                "provider": zorai_shared::providers::PROVIDER_ID_OPENAI,
            })
            .to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        "thread-provider-models",
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(
        !result.is_error,
        "model discovery should succeed: {}",
        result.content
    );
    let models: Vec<crate::agent::llm_client::FetchedModel> =
        serde_json::from_str(&result.content).expect("parse fetched models");
    assert_eq!(models.len(), 2);
    assert_eq!(models[0].id, "gpt-4.1");
    assert_eq!(models[1].id, "gpt-5.4");
}

#[tokio::test]
async fn list_providers_returns_auth_state_rows() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = zorai_shared::providers::PROVIDER_ID_OPENAI.to_string();
    config.api_key = "test-key".to_string();
    config.base_url = "https://api.openai.example/v1".to_string();
    let engine = AgentEngine::new_test(manager.clone(), config, root.path()).await;
    let (event_tx, _) = broadcast::channel(8);

    let tool_call = ToolCall::with_default_weles_review(
        "tool-list-providers".to_string(),
        ToolFunction {
            name: "list_providers".to_string(),
            arguments: serde_json::json!({}).to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        "thread-list-providers",
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(
        !result.is_error,
        "list_providers should succeed: {}",
        result.content
    );
    let providers: Vec<crate::agent::types::ProviderAuthState> =
        serde_json::from_str(&result.content).expect("parse provider auth states");
    assert!(providers.iter().any(|provider| {
        provider.provider_id == zorai_shared::providers::PROVIDER_ID_OPENAI
            && provider.authenticated
    }));
}

#[tokio::test]
async fn list_models_returns_remote_models_for_authenticated_provider() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = zorai_shared::providers::PROVIDER_ID_OPENAI.to_string();
    config.api_key = "test-key".to_string();
    config.base_url = spawn_model_fetch_server(serde_json::json!({
        "data": [
            {"id": "gpt-4.1", "name": "GPT-4.1", "context_length": 1048576},
            {"id": "gpt-5.4", "name": "GPT-5.4", "context_length": 1048576}
        ]
    }))
    .await;
    let engine = AgentEngine::new_test(manager.clone(), config, root.path()).await;
    let (event_tx, _) = broadcast::channel(8);

    let tool_call = ToolCall::with_default_weles_review(
        "tool-list-models".to_string(),
        ToolFunction {
            name: "list_models".to_string(),
            arguments: serde_json::json!({
                "provider": zorai_shared::providers::PROVIDER_ID_OPENAI,
            })
            .to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        "thread-list-models",
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(
        !result.is_error,
        "list_models should succeed: {}",
        result.content
    );
    let models: Vec<crate::agent::llm_client::FetchedModel> =
        serde_json::from_str(&result.content).expect("parse fetched models");
    assert_eq!(models.len(), 2);
    assert_eq!(models[0].id, "gpt-4.1");
    assert_eq!(models[1].id, "gpt-5.4");
}

#[tokio::test]
async fn list_models_returns_built_in_catalog_when_remote_fetch_is_unsupported() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = zorai_shared::providers::PROVIDER_ID_ALIBABA_CODING_PLAN.to_string();
    config.api_key = "test-key".to_string();
    config.base_url = "https://coding-intl.dashscope.aliyuncs.com/v1".to_string();
    let engine = AgentEngine::new_test(manager.clone(), config, root.path()).await;
    let (event_tx, _) = broadcast::channel(8);

    let tool_call = ToolCall::with_default_weles_review(
        "tool-list-models-built-in".to_string(),
        ToolFunction {
            name: "list_models".to_string(),
            arguments: serde_json::json!({
                "provider": zorai_shared::providers::PROVIDER_ID_ALIBABA_CODING_PLAN,
            })
            .to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        "thread-list-models-built-in",
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(
        !result.is_error,
        "list_models built-in fallback should succeed: {}",
        result.content
    );
    let models: Vec<crate::agent::llm_client::FetchedModel> =
        serde_json::from_str(&result.content).expect("parse fetched models");
    assert!(
        !models.is_empty(),
        "built-in fallback should not return an empty list"
    );
    assert!(models.iter().any(|model| model.id == "glm-5"));
    assert!(models.iter().any(|model| model.id == "qwen3.6-plus"));
}

#[tokio::test]
async fn list_agents_returns_effective_runtime_targets() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = zorai_shared::providers::PROVIDER_ID_OPENAI.to_string();
    config.model = "gpt-5.4-mini".to_string();
    config.reasoning_effort = "xhigh".to_string();
    config.context_window_tokens = 256_000;
    config.api_key = "test-key".to_string();
    config.concierge.provider = Some(zorai_shared::providers::PROVIDER_ID_GROQ.to_string());
    config.concierge.model = Some("llama-3.3-70b-versatile".to_string());
    config.builtin_sub_agents.weles.provider =
        Some(zorai_shared::providers::PROVIDER_ID_ANTHROPIC.to_string());
    config.builtin_sub_agents.weles.model = Some("claude-sonnet-4-20250514".to_string());
    config.builtin_sub_agents.weles.reasoning_effort = Some("medium".to_string());
    config.sub_agents.push(crate::agent::SubAgentDefinition {
        base_url: None,
        claude_permission_mode: None,
        id: "reviewer".to_string(),
        name: "Reviewer".to_string(),
        provider: zorai_shared::providers::PROVIDER_ID_OPENAI.to_string(),
        model: "gpt-5.4".to_string(),
        role: Some("review".to_string()),
        system_prompt: None,
        tool_whitelist: None,
        tool_blacklist: None,
        context_budget_tokens: None,
        context_window_tokens: Some(180_000),
        max_duration_secs: None,
        supervisor_config: None,
        enabled: true,
        builtin: false,
        immutable_identity: false,
        disable_allowed: true,
        delete_allowed: true,
        protected_reason: None,
        reasoning_effort: Some("low".to_string()),
        api_transport: None,
        openrouter_provider_order: Vec::new(),
        openrouter_provider_ignore: Vec::new(),
        openrouter_allow_fallbacks: None,
        huggingface_provider: None,
        created_at: 0,
    });
    let engine = AgentEngine::new_test(manager.clone(), config, root.path()).await;
    let (event_tx, _) = broadcast::channel(8);

    let tool_call = ToolCall::with_default_weles_review(
        "tool-list-agents".to_string(),
        ToolFunction {
            name: "list_agents".to_string(),
            arguments: serde_json::json!({}).to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        "thread-list-agents",
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(
        !result.is_error,
        "list_agents should succeed: {}",
        result.content
    );
    let agents: serde_json::Value =
        serde_json::from_str(&result.content).expect("parse list_agents payload");
    let rows = agents
        .as_array()
        .expect("list_agents should return an array");
    let svarog = rows
        .iter()
        .find(|row| row.get("agent").and_then(|value| value.as_str()) == Some("svarog"))
        .expect("svarog row should be present");
    assert_eq!(
        svarog.get("provider").and_then(|value| value.as_str()),
        Some(zorai_shared::providers::PROVIDER_ID_OPENAI)
    );
    assert_eq!(
        svarog.get("model").and_then(|value| value.as_str()),
        Some("gpt-5.4-mini")
    );
    assert_eq!(
        svarog
            .get("reasoning_effort")
            .and_then(|value| value.as_str()),
        Some("xhigh")
    );
    assert_eq!(
        svarog
            .get("context_window_tokens")
            .and_then(|value| value.as_u64()),
        Some(256_000)
    );
    let rarog = rows
        .iter()
        .find(|row| row.get("agent").and_then(|value| value.as_str()) == Some("rarog"))
        .expect("rarog row should be present");
    assert_eq!(
        rarog.get("provider").and_then(|value| value.as_str()),
        Some(zorai_shared::providers::PROVIDER_ID_GROQ)
    );
    let perun = rows
        .iter()
        .find(|row| row.get("agent").and_then(|value| value.as_str()) == Some("perun"))
        .expect("perun row should be present");
    assert_eq!(
        perun.get("name").and_then(|value| value.as_str()),
        Some("Perun")
    );
    assert_eq!(
        perun.get("kind").and_then(|value| value.as_str()),
        Some("builtin")
    );
    let mokosh = rows
        .iter()
        .find(|row| row.get("agent").and_then(|value| value.as_str()) == Some("mokosh"))
        .expect("mokosh row should be present");
    assert_eq!(
        mokosh.get("name").and_then(|value| value.as_str()),
        Some("Mokosh")
    );
    let dazhbog = rows
        .iter()
        .find(|row| row.get("agent").and_then(|value| value.as_str()) == Some("dazhbog"))
        .expect("dazhbog row should be present");
    assert_eq!(
        dazhbog.get("name").and_then(|value| value.as_str()),
        Some("Dazhbog")
    );
    let rod = rows
        .iter()
        .find(|row| row.get("agent").and_then(|value| value.as_str()) == Some("rod"))
        .expect("rod row should be present");
    assert_eq!(
        rod.get("name").and_then(|value| value.as_str()),
        Some("Rod")
    );
    let weles = rows
        .iter()
        .find(|row| row.get("agent").and_then(|value| value.as_str()) == Some("weles"))
        .expect("weles row should be present");
    assert_eq!(
        weles.get("provider").and_then(|value| value.as_str()),
        Some(zorai_shared::providers::PROVIDER_ID_ANTHROPIC)
    );
    assert_eq!(
        weles
            .get("reasoning_effort")
            .and_then(|value| value.as_str()),
        Some("medium")
    );
    assert_eq!(
        weles
            .get("context_window_tokens")
            .and_then(|value| value.as_u64()),
        Some(200_000)
    );
    assert_eq!(
        svarog.get("spawnable"),
        Some(&serde_json::Value::Bool(false))
    );
    assert_eq!(
        weles.get("spawnable"),
        Some(&serde_json::Value::Bool(false))
    );
    assert!(rows.iter().any(|row| {
        row.get("agent").and_then(|value| value.as_str()) == Some("reviewer")
            && row.get("model").and_then(|value| value.as_str()) == Some("gpt-5.4")
            && row.get("spawnable") == Some(&serde_json::Value::Bool(true))
            && row.get("role").and_then(|value| value.as_str()) == Some("review")
            && row.get("reasoning_effort").and_then(|value| value.as_str()) == Some("low")
            && row
                .get("context_window_tokens")
                .and_then(|value| value.as_u64())
                == Some(180_000)
    }));
    let perun_provider = perun.get("provider").and_then(|value| value.as_str());
    assert_eq!(
        perun_provider,
        Some(zorai_shared::providers::PROVIDER_ID_OPENAI),
        "builtin personas inherit the main provider when they have no override"
    );
}

#[tokio::test]
async fn list_participants_returns_thread_participant_rows() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);
    let thread_id = "thread-list-participants";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        crate::agent::types::AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Participant list".to_string(),
            messages: vec![crate::agent::types::AgentMessage::user(
                "Inspect participants.",
                1,
            )],
            pinned: false,
            upstream_thread_id: None,
            upstream_transport: None,
            upstream_provider: None,
            upstream_model: None,
            upstream_assistant_id: None,
            total_input_tokens: 0,
            total_output_tokens: 0,
            created_at: 1,
            updated_at: 1,
        },
    );
    engine
        .upsert_thread_participant(thread_id, "weles", "watch this thread")
        .await
        .expect("participant should register");

    let tool_call = ToolCall::with_default_weles_review(
        "tool-list-participants".to_string(),
        ToolFunction {
            name: "list_participants".to_string(),
            arguments: serde_json::json!({}).to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        thread_id,
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(
        !result.is_error,
        "list_participants should succeed: {}",
        result.content
    );
    let participants: serde_json::Value =
        serde_json::from_str(&result.content).expect("parse list_participants payload");
    let rows = participants
        .as_array()
        .expect("list_participants should return an array");
    assert!(rows.iter().any(|row| {
        row.get("agent").and_then(|value| value.as_str()) == Some("weles")
            && row.get("status").and_then(|value| value.as_str()) == Some("active")
    }));
}

#[tokio::test]
async fn switch_model_updates_targeted_agent_settings_from_svarog_scope() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = zorai_shared::providers::PROVIDER_ID_OPENAI.to_string();
    config.model = "gpt-5.4-mini".to_string();
    config.api_key = "test-key".to_string();
    config.concierge.provider = Some(zorai_shared::providers::PROVIDER_ID_OPENAI.to_string());
    config.concierge.model = Some("gpt-4.1-mini".to_string());
    config.providers.insert(
        zorai_shared::providers::PROVIDER_ID_GROQ.to_string(),
        crate::agent::types::ProviderConfig {
            base_url: String::new(),
            model: String::new(),
            api_key: "groq-key".to_string(),
            assistant_id: String::new(),
            auth_source: crate::agent::types::AuthSource::ApiKey,
            api_transport: crate::agent::types::ApiTransport::default(),
            reasoning_effort: "high".to_string(),
            context_window_tokens: 128000,
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
    config.sub_agents.push(crate::agent::SubAgentDefinition {
        base_url: None,
        claude_permission_mode: None,
        id: "reviewer".to_string(),
        name: "Reviewer".to_string(),
        provider: zorai_shared::providers::PROVIDER_ID_OPENAI.to_string(),
        model: "gpt-4.1".to_string(),
        role: None,
        system_prompt: None,
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
        reasoning_effort: None,
        api_transport: None,
        openrouter_provider_order: Vec::new(),
        openrouter_provider_ignore: Vec::new(),
        openrouter_allow_fallbacks: None,
        huggingface_provider: None,
        created_at: 0,
    });
    let engine = AgentEngine::new_test(manager.clone(), config, root.path()).await;
    let (event_tx, _) = broadcast::channel(8);

    for (agent, provider, model) in [
        (
            "svarog",
            zorai_shared::providers::PROVIDER_ID_OPENAI,
            "gpt-5.4",
        ),
        (
            "rarog",
            zorai_shared::providers::PROVIDER_ID_GROQ,
            "llama-3.3-70b-versatile",
        ),
        (
            "weles",
            zorai_shared::providers::PROVIDER_ID_OPENAI,
            "gpt-5.4-mini",
        ),
        (
            "reviewer",
            zorai_shared::providers::PROVIDER_ID_OPENAI,
            "gpt-5.4-mini",
        ),
    ] {
        let tool_call = ToolCall::with_default_weles_review(
            format!("tool-switch-model-{agent}"),
            ToolFunction {
                name: "switch_model".to_string(),
                arguments: serde_json::json!({
                    "agent": agent,
                    "provider": provider,
                    "model": model,
                })
                .to_string(),
            },
        );

        let result = crate::agent::agent_identity::run_with_agent_scope(
            crate::agent::agent_identity::MAIN_AGENT_ID.to_string(),
            async {
                execute_tool(
                    &tool_call,
                    &engine,
                    "thread-switch-model",
                    None,
                    &manager,
                    None,
                    &event_tx,
                    root.path(),
                    &engine.http_client,
                    None,
                )
                .await
            },
        )
        .await;

        assert!(
            !result.is_error,
            "switch_model should succeed for {agent}: {}",
            result.content
        );
    }

    let stored = engine.get_config().await;
    assert_eq!(stored.provider, zorai_shared::providers::PROVIDER_ID_OPENAI);
    assert_eq!(stored.model, "gpt-5.4");
    assert_eq!(
        stored.concierge.provider.as_deref(),
        Some(zorai_shared::providers::PROVIDER_ID_GROQ)
    );
    assert_eq!(
        stored.concierge.model.as_deref(),
        Some("llama-3.3-70b-versatile")
    );
    assert_eq!(
        stored.builtin_sub_agents.weles.provider.as_deref(),
        Some(zorai_shared::providers::PROVIDER_ID_OPENAI)
    );
    assert_eq!(
        stored.builtin_sub_agents.weles.model.as_deref(),
        Some("gpt-5.4-mini")
    );
    let reviewer = stored
        .sub_agents
        .iter()
        .find(|agent| agent.id == "reviewer")
        .expect("reviewer subagent should exist");
    assert_eq!(
        reviewer.provider,
        zorai_shared::providers::PROVIDER_ID_OPENAI
    );
    assert_eq!(reviewer.model, "gpt-5.4-mini");
}

#[tokio::test]
async fn switch_model_is_rejected_outside_svarog_scope() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = zorai_shared::providers::PROVIDER_ID_OPENAI.to_string();
    config.model = "gpt-5.4-mini".to_string();
    config.api_key = "test-key".to_string();
    let engine = AgentEngine::new_test(manager.clone(), config, root.path()).await;
    let (event_tx, _) = broadcast::channel(8);

    let tool_call = ToolCall::with_default_weles_review(
        "tool-switch-model-rarog-scope".to_string(),
        ToolFunction {
            name: "switch_model".to_string(),
            arguments: serde_json::json!({
                "agent": "svarog",
                "provider": zorai_shared::providers::PROVIDER_ID_OPENAI,
                "model": "gpt-5.4"
            })
            .to_string(),
        },
    );

    let result = crate::agent::agent_identity::run_with_agent_scope(
        crate::agent::agent_identity::CONCIERGE_AGENT_ID.to_string(),
        async {
            execute_tool(
                &tool_call,
                &engine,
                "thread-switch-model-rarog",
                None,
                &manager,
                None,
                &event_tx,
                root.path(),
                &engine.http_client,
                None,
            )
            .await
        },
    )
    .await;

    assert!(result.is_error, "non-svarog switch_model call should fail");
    assert!(
        result.content.contains("svarog"),
        "rejection should mention svarog-only access: {}",
        result.content
    );
}

#[tokio::test]
async fn spawn_subagent_rejects_model_without_provider_and_points_to_fetch_tools() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);

    let error = super::execute_spawn_subagent(
        &serde_json::json!({
            "title": "Write foundational skill files",
            "description": "Create the foundational skill files in parallel.",
            "model": "gpt-5.4"
        }),
        &engine,
        "thread-parent",
        None,
        &manager,
        None,
        &event_tx,
    )
    .await
    .expect_err("model-only override should be rejected");

    let message = error.to_string();
    assert!(message.contains("provider"));
    assert!(message.contains("list_providers"));
    assert!(message.contains("list_models"));
}

#[tokio::test]
async fn spawn_subagent_rejects_unauthenticated_provider_and_points_to_fetch_tools() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);

    let error = super::execute_spawn_subagent(
        &serde_json::json!({
            "title": "Write foundational skill files",
            "description": "Create the foundational skill files in parallel.",
            "provider": zorai_shared::providers::PROVIDER_ID_OPENAI
        }),
        &engine,
        "thread-parent",
        None,
        &manager,
        None,
        &event_tx,
    )
    .await
    .expect_err("unauthenticated provider override should be rejected");

    let message = error.to_string();
    assert!(message.contains("list_providers"));
    assert!(message.contains(zorai_shared::providers::PROVIDER_ID_OPENAI));
}

#[tokio::test]
async fn spawn_subagent_persists_explicit_provider_and_model_override() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = zorai_shared::providers::PROVIDER_ID_OPENAI.to_string();
    config.api_key = "test-key".to_string();
    config.base_url = spawn_model_fetch_server(serde_json::json!({
        "data": [
            {"id": "gpt-4.1", "name": "GPT-4.1", "context_length": 1048576},
            {"id": "gpt-5.4", "name": "GPT-5.4", "context_length": 1048576}
        ]
    }))
    .await;
    let engine = AgentEngine::new_test(manager.clone(), config, root.path()).await;
    let (event_tx, _) = broadcast::channel(8);

    let result = super::execute_spawn_subagent(
        &serde_json::json!({
            "title": "Write foundational skill files",
            "description": "Create the foundational skill files in parallel.",
            "provider": zorai_shared::providers::PROVIDER_ID_OPENAI,
            "model": "gpt-5.4"
        }),
        &engine,
        "thread-parent",
        None,
        &manager,
        None,
        &event_tx,
    )
    .await
    .expect("explicit provider/model override should succeed");

    let tasks = engine.list_tasks().await;
    let task = tasks
        .into_iter()
        .find(|task| result.contains(&task.id))
        .expect("spawned subagent should exist");
    assert_eq!(
        task.override_provider.as_deref(),
        Some(zorai_shared::providers::PROVIDER_ID_OPENAI)
    );
    assert_eq!(task.override_model.as_deref(), Some("gpt-5.4"));
}

#[tokio::test]
async fn spawn_subagent_derives_budget_from_effective_subagent_provider_window() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = zorai_shared::providers::PROVIDER_ID_OPENAI.to_string();
    config.model = "gpt-5.4".to_string();
    config.api_key = "test-key".to_string();
    config.context_window_tokens = 1_000_000;
    config.providers.insert(
        zorai_shared::providers::PROVIDER_ID_GROQ.to_string(),
        crate::agent::types::ProviderConfig {
            base_url: String::new(),
            model: "llama-3.3-70b-versatile".to_string(),
            api_key: "groq-key".to_string(),
            assistant_id: String::new(),
            auth_source: crate::agent::types::AuthSource::ApiKey,
            api_transport: crate::agent::types::ApiTransport::default(),
            reasoning_effort: "medium".to_string(),
            context_window_tokens: 205_000,
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
    config.sub_agents.push(crate::agent::SubAgentDefinition {
        base_url: None,
        claude_permission_mode: None,
        id: "reviewer".to_string(),
        name: "Reviewer".to_string(),
        provider: zorai_shared::providers::PROVIDER_ID_GROQ.to_string(),
        model: "llama-3.3-70b-versatile".to_string(),
        role: Some("Reviewer".to_string()),
        system_prompt: Some("Review code carefully.".to_string()),
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
        reasoning_effort: Some("medium".to_string()),
        api_transport: None,
        openrouter_provider_order: Vec::new(),
        openrouter_provider_ignore: Vec::new(),
        openrouter_allow_fallbacks: None,
        huggingface_provider: None,
        created_at: 0,
    });
    let engine = AgentEngine::new_test(manager.clone(), config, root.path()).await;
    let (event_tx, _) = broadcast::channel(8);

    let result = super::execute_spawn_subagent(
        &serde_json::json!({
            "title": "Reviewer",
            "description": "Review the diff and report problems."
        }),
        &engine,
        "thread-parent",
        None,
        &manager,
        None,
        &event_tx,
    )
    .await
    .expect("spawn_subagent should succeed");

    let task = engine
        .list_tasks()
        .await
        .into_iter()
        .find(|task| result.contains(&task.id))
        .expect("spawned subagent should exist");
    assert_eq!(
        task.context_budget_tokens,
        Some(128_000),
        "derived context budget should use the effective subagent provider/model window, not the parent config"
    );
}

#[tokio::test]
async fn spawn_subagent_reserved_thread_detail_includes_execution_profile_metadata() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.providers.insert(
        zorai_shared::providers::PROVIDER_ID_GROQ.to_string(),
        crate::agent::types::ProviderConfig {
            base_url: String::new(),
            model: "llama-3.3-70b-versatile".to_string(),
            api_key: "groq-key".to_string(),
            assistant_id: String::new(),
            auth_source: crate::agent::types::AuthSource::ApiKey,
            api_transport: crate::agent::types::ApiTransport::default(),
            reasoning_effort: "high".to_string(),
            context_window_tokens: 205_000,
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
    config.sub_agents.push(crate::agent::SubAgentDefinition {
        base_url: None,
        claude_permission_mode: None,
        id: "design-reviewer".to_string(),
        name: "Design Reviewer".to_string(),
        provider: zorai_shared::providers::PROVIDER_ID_GROQ.to_string(),
        model: "llama-3.3-70b-versatile".to_string(),
        role: Some("Designer".to_string()),
        system_prompt: Some("Refactor the design only.".to_string()),
        tool_whitelist: None,
        tool_blacklist: None,
        context_budget_tokens: None,
        context_window_tokens: Some(456_789),
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
    });
    let engine = AgentEngine::new_test(manager.clone(), config, root.path()).await;
    let (event_tx, _) = broadcast::channel(8);

    let result = super::execute_spawn_subagent(
        &serde_json::json!({
            "title": "Design Reviewer",
            "description": "Refactor docs design without semantic changes.",
            "reasoning_effort": "low"
        }),
        &engine,
        "thread-parent",
        None,
        &manager,
        None,
        &event_tx,
    )
    .await
    .expect("spawn_subagent should succeed");

    let task = engine
        .list_tasks()
        .await
        .into_iter()
        .find(|task| result.contains(&task.id))
        .expect("spawned subagent should exist");
    let reserved_thread_id = task
        .thread_id
        .clone()
        .expect("spawned subagent should reserve a thread id");

    let detail_json = engine
        .agent_thread_detail_json(&reserved_thread_id, None, None, false)
        .await;
    let detail: serde_json::Value =
        serde_json::from_str(&detail_json).expect("thread detail json should parse");

    assert_eq!(
        detail.get("agent_name").and_then(|value| value.as_str()),
        Some("Design Reviewer")
    );
    assert_eq!(
        detail
            .get("profile_provider")
            .and_then(|value| value.as_str()),
        Some(zorai_shared::providers::PROVIDER_ID_GROQ)
    );
    assert_eq!(
        detail.get("profile_model").and_then(|value| value.as_str()),
        Some("llama-3.3-70b-versatile")
    );
    assert_eq!(
        detail
            .get("profile_reasoning_effort")
            .and_then(|value| value.as_str()),
        Some("low")
    );
    assert_eq!(
        detail
            .get("profile_context_window_tokens")
            .and_then(|value| value.as_u64()),
        Some(456_789)
    );
}

#[tokio::test]
async fn spawn_subagent_bootstraps_todos_for_goal_run_tasks() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);
    let thread_id = "thread-goal-subagents";
    let task_id = "task-goal-subagents";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            crate::agent::types::AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: "Goal work".to_string(),
                messages: vec![crate::agent::types::AgentMessage::user(
                    "Start the release goal and delegate the code review.",
                    1,
                )],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: 1,
                updated_at: 1,
            },
        );
    }
    engine
        .tasks
        .lock()
        .await
        .push_back(crate::agent::types::AgentTask {
            id: task_id.to_string(),
            title: "Run release goal".to_string(),
            description: "Delegate review from the goal run.".to_string(),
            status: crate::agent::types::TaskStatus::Queued,
            priority: crate::agent::types::TaskPriority::High,
            progress: 0,
            created_at: 1,
            started_at: None,
            completed_at: None,
            error: None,
            result: None,
            thread_id: Some(thread_id.to_string()),
            source: "goal_run".to_string(),
            notify_on_complete: false,
            notify_channels: Vec::new(),
            dependencies: Vec::new(),
            command: None,
            session_id: None,
            goal_run_id: Some("goal-1".to_string()),
            goal_run_title: Some("Release the build".to_string()),
            goal_step_id: Some("step-1".to_string()),
            goal_step_title: Some("Delegate review".to_string()),
            parent_task_id: None,
            parent_thread_id: None,
            runtime: "daemon".to_string(),
            retry_count: 0,
            max_retries: 0,
            next_retry_at: None,
            scheduled_at: None,
            blocked_reason: None,
            awaiting_approval_id: None,
            policy_fingerprint: None,
            approval_expires_at: None,
            containment_scope: None,
            compensation_status: None,
            compensation_summary: None,
            lane_id: None,
            last_error: None,
            logs: Vec::new(),
            completion_contract: None,
            tool_whitelist: None,
            tool_blacklist: None,
            context_budget_tokens: None,
            context_overflow_action: None,
            termination_conditions: None,
            success_criteria: None,
            max_duration_secs: None,
            supervisor_config: None,
            override_provider: None,
            override_model: None,
            override_api_transport: None,
            override_system_prompt: None,
            sub_agent_def_id: None,
        });

    let tool_call = ToolCall::with_default_weles_review(
            "tool-goal-spawn-subagent-bootstrap".to_string(),
            ToolFunction {
                name: "spawn_subagent".to_string(),
                arguments: serde_json::json!({
                    "title": "Review release notes",
                    "description": "Check the release notes and report issues before the goal continues."
                })
                .to_string(),
            },
        );

    let result = execute_tool(
        &tool_call,
        &engine,
        thread_id,
        Some(task_id),
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(
        !result.is_error,
        "goal-run subagent spawning should bootstrap plan state instead of failing: {}",
        result.content
    );
    assert!(result.content.contains("Spawned subagent"));

    let todos = engine.get_todos(thread_id).await;
    assert_eq!(todos.len(), 1);
    assert_eq!(todos[0].status, TodoStatus::InProgress);
    assert!(
        todos[0].content.contains("Review release notes"),
        "goal-run bootstrap todo should reflect the delegated work"
    );
    let tasks = engine.tasks.lock().await;
    let spawned = tasks
        .iter()
        .find(|task| task.parent_task_id.as_deref() == Some(task_id))
        .expect("spawned goal subagent should be enqueued");
    assert!(
        spawned
            .tool_blacklist
            .as_ref()
            .is_some_and(|tools| tools.iter().any(|tool| tool == "ask_questions")),
        "goal-owned subagents should inherit the ask_questions blacklist"
    );
}

#[tokio::test]
async fn spawn_subagent_rejects_recursive_spawn_beyond_default_flat_depth() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);

    engine
        .tasks
        .lock()
        .await
        .push_back(crate::agent::types::AgentTask {
            id: "task-parent-subagent".to_string(),
            title: "Parent subagent".to_string(),
            description: "Already delegated child work".to_string(),
            status: crate::agent::types::TaskStatus::Queued,
            priority: crate::agent::types::TaskPriority::Normal,
            progress: 0,
            created_at: 1,
            started_at: None,
            completed_at: None,
            error: None,
            result: None,
            thread_id: Some("thread-parent".to_string()),
            source: "subagent".to_string(),
            notify_on_complete: false,
            notify_channels: Vec::new(),
            dependencies: Vec::new(),
            command: None,
            session_id: None,
            goal_run_id: None,
            goal_run_title: None,
            goal_step_id: None,
            goal_step_title: None,
            parent_task_id: Some("task-root".to_string()),
            parent_thread_id: Some("thread-parent".to_string()),
            runtime: "daemon".to_string(),
            retry_count: 0,
            max_retries: 0,
            next_retry_at: None,
            scheduled_at: None,
            blocked_reason: None,
            awaiting_approval_id: None,
            policy_fingerprint: None,
            approval_expires_at: None,
            containment_scope: None,
            compensation_status: None,
            compensation_summary: None,
            lane_id: None,
            last_error: None,
            logs: Vec::new(),
            completion_contract: None,
            tool_whitelist: None,
            tool_blacklist: None,
            context_budget_tokens: None,
            context_overflow_action: None,
            termination_conditions: None,
            success_criteria: None,
            max_duration_secs: None,
            supervisor_config: None,
            override_provider: None,
            override_model: None,
            override_api_transport: None,
            override_system_prompt: None,
            sub_agent_def_id: None,
        });

    let error = super::execute_spawn_subagent(
        &serde_json::json!({
            "title": "Grandchild helper",
            "description": "Try to recurse under a flat default subagent."
        }),
        &engine,
        "thread-parent",
        Some("task-parent-subagent"),
        &manager,
        None,
        &event_tx,
    )
    .await
    .expect_err("nested recursion should be rejected by default");

    assert!(error.to_string().contains("max_depth") || error.to_string().contains("depth"));
}

#[tokio::test]
async fn spawn_subagent_allows_recursive_spawn_when_parent_scope_permits_and_derives_limits() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);

    engine
        .tasks
        .lock()
        .await
        .push_back(crate::agent::types::AgentTask {
            id: "task-parent-subagent".to_string(),
            title: "Parent subagent".to_string(),
            description: "Allowed to recurse one level deeper".to_string(),
            status: crate::agent::types::TaskStatus::Queued,
            priority: crate::agent::types::TaskPriority::Normal,
            progress: 0,
            created_at: 1,
            started_at: None,
            completed_at: None,
            error: None,
            result: None,
            thread_id: Some("thread-parent".to_string()),
            source: "subagent".to_string(),
            notify_on_complete: false,
            notify_channels: Vec::new(),
            dependencies: Vec::new(),
            command: None,
            session_id: None,
            goal_run_id: None,
            goal_run_title: None,
            goal_step_id: None,
            goal_step_title: None,
            parent_task_id: Some("task-root".to_string()),
            parent_thread_id: Some("thread-parent".to_string()),
            runtime: "daemon".to_string(),
            retry_count: 0,
            max_retries: 0,
            next_retry_at: None,
            scheduled_at: None,
            blocked_reason: None,
            awaiting_approval_id: None,
            policy_fingerprint: None,
            approval_expires_at: None,
            containment_scope: Some("subagent-depth:1/2".to_string()),
            compensation_status: None,
            compensation_summary: None,
            lane_id: None,
            last_error: None,
            logs: Vec::new(),
            completion_contract: None,
            tool_whitelist: None,
            tool_blacklist: None,
            context_budget_tokens: Some(20_000),
            context_overflow_action: None,
            termination_conditions: Some("tool_call_count(10)".to_string()),
            success_criteria: None,
            max_duration_secs: Some(240),
            supervisor_config: None,
            override_provider: None,
            override_model: None,
            override_api_transport: None,
            override_system_prompt: None,
            sub_agent_def_id: None,
        });

    let result = super::execute_spawn_subagent(
        &serde_json::json!({
            "title": "Grandchild helper",
            "description": "Recurse one level deeper with inherited depth allowance."
        }),
        &engine,
        "thread-parent",
        Some("task-parent-subagent"),
        &manager,
        None,
        &event_tx,
    )
    .await
    .expect("nested recursion should succeed when parent scope permits it");

    assert!(result.contains("Delegation depth: 2/2"));

    let task = engine
        .list_tasks()
        .await
        .into_iter()
        .find(|task| result.contains(&task.id))
        .expect("spawned recursive subagent should exist");
    assert_eq!(
        task.containment_scope.as_deref(),
        Some("subagent-depth:2/2")
    );
    assert_eq!(task.context_budget_tokens, Some(20_000));
    assert_eq!(task.max_duration_secs, Some(180));
    assert!(task.termination_conditions.is_none());
    assert_eq!(
        task.context_overflow_action,
        Some(crate::agent::types::ContextOverflowAction::Error)
    );
}

#[tokio::test]
async fn spawn_subagent_strained_mode_constrains_derived_budgets() {
    let root_normal = tempdir().expect("tempdir should succeed");
    let manager_normal = SessionManager::new_test(root_normal.path()).await;
    let engine_normal = AgentEngine::new_test(
        manager_normal.clone(),
        AgentConfig::default(),
        root_normal.path(),
    )
    .await;
    let (event_tx_normal, _) = broadcast::channel(8);

    let normal_result = super::execute_spawn_subagent(
        &serde_json::json!({
            "title": "Budget baseline helper",
            "description": "Collect the unadapted default subagent budget."
        }),
        &engine_normal,
        "thread-parent",
        None,
        &manager_normal,
        None,
        &event_tx_normal,
    )
    .await
    .expect("normal spawn_subagent should succeed");

    let normal_task = engine_normal
        .list_tasks()
        .await
        .into_iter()
        .find(|task| normal_result.contains(&task.id))
        .expect("normal spawned subagent should exist");

    let root_strained = tempdir().expect("tempdir should succeed");
    let manager_strained = SessionManager::new_test(root_strained.path()).await;
    let engine_strained = AgentEngine::new_test(
        manager_strained.clone(),
        AgentConfig::default(),
        root_strained.path(),
    )
    .await;
    let (event_tx_strained, _) = broadcast::channel(8);
    {
        let mut model = engine_strained.operator_model.write().await;
        model.operator_satisfaction.label = "strained".to_string();
        model.operator_satisfaction.score = 0.18;
    }

    let strained_result = super::execute_spawn_subagent(
        &serde_json::json!({
            "title": "Budget constrained helper",
            "description": "Collect the strained-mode default subagent budget."
        }),
        &engine_strained,
        "thread-parent",
        None,
        &manager_strained,
        None,
        &event_tx_strained,
    )
    .await
    .expect("strained spawn_subagent should succeed");

    let strained_task = engine_strained
        .list_tasks()
        .await
        .into_iter()
        .find(|task| strained_result.contains(&task.id))
        .expect("strained spawned subagent should exist");

    let normal_tokens = normal_task
        .context_budget_tokens
        .expect("normal spawn should derive a context budget");
    let strained_tokens = strained_task
        .context_budget_tokens
        .expect("strained spawn should derive a context budget");
    assert_eq!(
        strained_tokens,
        ((normal_tokens as f64 * 0.6).round() as u32).max(256)
    );

    let normal_duration = normal_task
        .max_duration_secs
        .expect("normal spawn should derive a duration budget");
    let strained_duration = strained_task
        .max_duration_secs
        .expect("strained spawn should derive a duration budget");
    assert_eq!(
        strained_duration,
        ((normal_duration as f64 * 0.6).round() as u64).max(30)
    );

    assert!(normal_task.termination_conditions.is_none());
    assert!(strained_task.termination_conditions.is_none());
}

#[tokio::test]
async fn spawn_subagent_fragile_mode_constrains_inherited_max_depth_without_explicit_request() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);

    {
        let mut model = engine.operator_model.write().await;
        model.operator_satisfaction.label = "fragile".to_string();
        model.operator_satisfaction.score = 0.42;
    }

    engine
        .tasks
        .lock()
        .await
        .push_back(crate::agent::types::AgentTask {
            id: "task-root-with-recursion".to_string(),
            title: "Root task".to_string(),
            description: "Parent task exposes a broader inherited recursion allowance".to_string(),
            status: crate::agent::types::TaskStatus::Queued,
            priority: crate::agent::types::TaskPriority::Normal,
            progress: 0,
            created_at: 1,
            started_at: None,
            completed_at: None,
            error: None,
            result: None,
            thread_id: Some("thread-parent".to_string()),
            source: "user".to_string(),
            notify_on_complete: false,
            notify_channels: Vec::new(),
            dependencies: Vec::new(),
            command: None,
            session_id: None,
            goal_run_id: None,
            goal_run_title: None,
            goal_step_id: None,
            goal_step_title: None,
            parent_task_id: None,
            parent_thread_id: Some("thread-parent".to_string()),
            runtime: "daemon".to_string(),
            retry_count: 0,
            max_retries: 0,
            next_retry_at: None,
            scheduled_at: None,
            blocked_reason: None,
            awaiting_approval_id: None,
            policy_fingerprint: None,
            approval_expires_at: None,
            containment_scope: Some("subagent-depth:0/3".to_string()),
            compensation_status: None,
            compensation_summary: None,
            lane_id: None,
            last_error: None,
            logs: Vec::new(),
            completion_contract: None,
            tool_whitelist: None,
            tool_blacklist: None,
            context_budget_tokens: None,
            context_overflow_action: None,
            termination_conditions: None,
            success_criteria: None,
            max_duration_secs: None,
            supervisor_config: None,
            override_provider: None,
            override_model: None,
            override_api_transport: None,
            override_system_prompt: None,
            sub_agent_def_id: None,
        });

    let result = super::execute_spawn_subagent(
        &serde_json::json!({
            "title": "Fragile-mode helper",
            "description": "Spawn without an explicit max_depth so fragile mode can tighten inheritance."
        }),
        &engine,
        "thread-parent",
        Some("task-root-with-recursion"),
        &manager,
        None,
        &event_tx,
    )
    .await
    .expect("fragile spawn_subagent should succeed");

    assert!(result.contains("Delegation depth: 1/2"));

    let task = engine
        .list_tasks()
        .await
        .into_iter()
        .find(|task| result.contains(&task.id))
        .expect("fragile spawned subagent should exist");
    assert_eq!(
        task.containment_scope.as_deref(),
        Some("subagent-depth:1/2")
    );
}

#[tokio::test]
async fn list_subagents_includes_depth_and_budget_remaining() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;

    let parent = engine
        .enqueue_task(
            "Parent coordinator".to_string(),
            "Coordinate the child work".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "user",
            None,
            None,
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;

    let mut child = engine
        .enqueue_task(
            "Depth child".to_string(),
            "Inspect deployment risks".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            Some(parent.id.clone()),
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    child.containment_scope = Some("subagent-depth:1/2".to_string());
    child.context_budget_tokens = Some(10_000);
    child.max_duration_secs = Some(100);
    child.termination_conditions = Some("tool_call_count(15)".to_string());
    child.started_at = Some(crate::agent::now_millis().saturating_sub(20_000));
    {
        let mut tasks = engine.tasks.lock().await;
        if let Some(existing) = tasks.iter_mut().find(|task| task.id == child.id) {
            *existing = child.clone();
        }
    }

    engine
        .history
        .upsert_subagent_metrics(
            &child.id,
            Some(&parent.id),
            Some("thread-parent"),
            7,
            5,
            2,
            2500,
            Some(10_000),
            0.2,
            Some(crate::agent::now_millis().saturating_sub(5_000)),
            0.1,
            "healthy",
            1,
            crate::agent::now_millis(),
        )
        .await
        .expect("subagent metrics should persist");

    let result = super::execute_list_subagents(
        &serde_json::json!({ "parent_task_id": parent.id }),
        &engine,
        "thread-parent",
        None,
    )
    .await
    .expect("list_subagents should succeed");

    let payload: serde_json::Value = serde_json::from_str(&result).expect("valid JSON payload");
    let items = payload
        .as_array()
        .expect("list_subagents should return an array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["depth"].as_u64(), Some(1));
    assert_eq!(items[0]["max_depth"].as_u64(), Some(2));
    assert_eq!(
        items[0]["budget_remaining"]["tool_calls_remaining"].as_u64(),
        Some(8)
    );
    assert!(items[0]["budget_remaining"]["tokens_pct"]
        .as_f64()
        .is_some_and(|value| value > 0.7));
}

#[tokio::test]
async fn list_subagents_reports_exhausted_budget_limits() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;

    let parent = engine
        .enqueue_task(
            "Parent coordinator".to_string(),
            "Coordinate the child work".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "user",
            None,
            None,
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;

    let mut child = engine
        .enqueue_task(
            "Depth child".to_string(),
            "Inspect deployment risks".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            Some(parent.id.clone()),
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    child.containment_scope = Some("subagent-depth:1/2".to_string());
    child.context_budget_tokens = Some(10_000);
    child.max_duration_secs = Some(10);
    child.termination_conditions = Some("tool_call_count(3)".to_string());
    child.started_at = Some(crate::agent::now_millis().saturating_sub(30_000));
    {
        let mut tasks = engine.tasks.lock().await;
        if let Some(existing) = tasks.iter_mut().find(|task| task.id == child.id) {
            *existing = child.clone();
        }
    }

    engine
        .history
        .upsert_subagent_metrics(
            &child.id,
            Some(&parent.id),
            Some("thread-parent"),
            3,
            2,
            1,
            10_500,
            Some(10_000),
            0.0,
            Some(crate::agent::now_millis().saturating_sub(20_000)),
            0.9,
            "stalled",
            1,
            crate::agent::now_millis(),
        )
        .await
        .expect("subagent metrics should persist");

    let result = super::execute_list_subagents(
        &serde_json::json!({ "parent_task_id": parent.id }),
        &engine,
        "thread-parent",
        None,
    )
    .await
    .expect("list_subagents should succeed");

    let payload: serde_json::Value = serde_json::from_str(&result).expect("valid JSON payload");
    let items = payload
        .as_array()
        .expect("list_subagents should return an array");
    assert_eq!(items.len(), 1);
    assert_eq!(
        items[0]["budget_remaining"]["tokens_pct"].as_f64(),
        Some(0.0)
    );
    assert_eq!(items[0]["budget_remaining"]["time_pct"].as_f64(), Some(0.0));
    assert_eq!(
        items[0]["budget_remaining"]["tool_calls_remaining"].as_u64(),
        Some(0)
    );
    assert_eq!(items[0]["budget_exhausted"].as_bool(), Some(true));
    assert_eq!(
        items[0]["effective_status"].as_str(),
        Some("budget_exhausted")
    );
    assert_eq!(
        items[0]["exhausted_limits"]
            .as_array()
            .map(|items| items.len()),
        Some(3)
    );
    assert_eq!(items[0]["exhausted_limits"][0], "tokens");
    assert_eq!(items[0]["exhausted_limits"][1], "time");
    assert_eq!(items[0]["exhausted_limits"][2], "tool_calls");
}

#[tokio::test]
async fn list_subagents_includes_descendants_for_parent_task_tree() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;

    let parent = engine
        .enqueue_task(
            "Parent coordinator".to_string(),
            "Coordinate the child work".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "user",
            None,
            None,
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;

    let mut child = engine
        .enqueue_task(
            "Depth child".to_string(),
            "Inspect deployment risks".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            Some(parent.id.clone()),
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    child.containment_scope = Some("subagent-depth:1/3".to_string());
    {
        let mut tasks = engine.tasks.lock().await;
        if let Some(existing) = tasks.iter_mut().find(|task| task.id == child.id) {
            *existing = child.clone();
        }
    }

    let mut grandchild = engine
        .enqueue_task(
            "Grandchild helper".to_string(),
            "Inspect one narrow edge case".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            Some(child.id.clone()),
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    grandchild.containment_scope = Some("subagent-depth:2/3".to_string());
    {
        let mut tasks = engine.tasks.lock().await;
        if let Some(existing) = tasks.iter_mut().find(|task| task.id == grandchild.id) {
            *existing = grandchild.clone();
        }
    }

    let result = super::execute_list_subagents(
        &serde_json::json!({ "parent_task_id": parent.id }),
        &engine,
        "thread-parent",
        None,
    )
    .await
    .expect("list_subagents should succeed");

    let payload: serde_json::Value = serde_json::from_str(&result).expect("valid JSON payload");
    let items = payload
        .as_array()
        .expect("list_subagents should return an array");
    assert_eq!(
        items.len(),
        2,
        "parent task tree should include descendants"
    );

    let returned_ids = items
        .iter()
        .filter_map(|item| item.get("id").and_then(|value| value.as_str()))
        .collect::<std::collections::HashSet<_>>();
    assert!(returned_ids.contains(child.id.as_str()));
    assert!(returned_ids.contains(grandchild.id.as_str()));

    let grandchild_item = items
        .iter()
        .find(|item| {
            item.get("id").and_then(|value| value.as_str()) == Some(grandchild.id.as_str())
        })
        .expect("grandchild should be present in parent tree listing");
    assert_eq!(grandchild_item["depth"].as_u64(), Some(2));
    assert_eq!(grandchild_item["max_depth"].as_u64(), Some(3));
}

#[tokio::test]
async fn list_subagents_parent_task_tree_excludes_unrelated_same_thread_subagents() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;

    let parent = engine
        .enqueue_task(
            "Parent coordinator".to_string(),
            "Coordinate the child work".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "user",
            None,
            None,
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;

    let mut child = engine
        .enqueue_task(
            "Depth child".to_string(),
            "Inspect deployment risks".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            Some(parent.id.clone()),
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    child.containment_scope = Some("subagent-depth:1/3".to_string());

    let mut unrelated = engine
        .enqueue_task(
            "Unrelated sibling".to_string(),
            "Different subtree in the same thread".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            Some("task-other-root".to_string()),
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    unrelated.containment_scope = Some("subagent-depth:1/3".to_string());

    {
        let mut tasks = engine.tasks.lock().await;
        if let Some(existing) = tasks.iter_mut().find(|task| task.id == child.id) {
            *existing = child.clone();
        }
        if let Some(existing) = tasks.iter_mut().find(|task| task.id == unrelated.id) {
            *existing = unrelated.clone();
        }
    }

    let result = super::execute_list_subagents(
        &serde_json::json!({ "parent_task_id": parent.id }),
        &engine,
        "thread-parent",
        None,
    )
    .await
    .expect("list_subagents should succeed");

    let payload: serde_json::Value = serde_json::from_str(&result).expect("valid JSON payload");
    let items = payload
        .as_array()
        .expect("list_subagents should return an array");
    let returned_ids = items
        .iter()
        .filter_map(|item| item.get("id").and_then(|value| value.as_str()))
        .collect::<std::collections::HashSet<_>>();

    assert!(returned_ids.contains(child.id.as_str()));
    assert!(
        !returned_ids.contains(unrelated.id.as_str()),
        "parent task tree should not include unrelated same-thread subagents"
    );
}

#[tokio::test]
async fn list_subagents_from_child_task_uses_parent_scope_by_default() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;

    let parent = engine
        .enqueue_task(
            "Parent coordinator".to_string(),
            "Coordinate the child work".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "user",
            None,
            None,
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;

    let mut child_a = engine
        .enqueue_task(
            "Child A".to_string(),
            "Inspect deployment risks".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            Some(parent.id.clone()),
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;

    let mut child_b = engine
        .enqueue_task(
            "Child B".to_string(),
            "Inspect auth risks".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            Some(parent.id.clone()),
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    child_a.status = crate::agent::types::TaskStatus::InProgress;
    child_b.status = crate::agent::types::TaskStatus::InProgress;
    {
        let mut tasks = engine.tasks.lock().await;
        if let Some(existing) = tasks.iter_mut().find(|task| task.id == child_a.id) {
            *existing = child_a.clone();
        }
        if let Some(existing) = tasks.iter_mut().find(|task| task.id == child_b.id) {
            *existing = child_b.clone();
        }
    }

    let result = super::execute_list_subagents(
        &serde_json::json!({ "status": "in_progress" }),
        &engine,
        "thread-parent",
        Some(child_a.id.as_str()),
    )
    .await
    .expect("list_subagents should succeed");

    let payload: serde_json::Value = serde_json::from_str(&result).expect("valid JSON payload");
    let items = payload
        .as_array()
        .expect("list_subagents should return an array");
    let returned_ids = items
        .iter()
        .filter_map(|item| item.get("id").and_then(|value| value.as_str()))
        .collect::<std::collections::HashSet<_>>();

    assert!(
        returned_ids.contains(child_a.id.as_str()),
        "child scope should include the current subagent via its parent tree"
    );
    assert!(
        returned_ids.contains(child_b.id.as_str()),
        "child scope should include sibling subagents under the same parent task"
    );
}

#[tokio::test]
async fn list_subagents_from_child_task_uses_persisted_parent_scope() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;

    let parent = engine
        .enqueue_task(
            "Persisted parent coordinator".to_string(),
            "Coordinate persisted child work".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "user",
            None,
            None,
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;

    let mut child_a = engine
        .enqueue_task(
            "Persisted child A".to_string(),
            "Inspect deployment risks".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            Some(parent.id.clone()),
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    let mut child_b = engine
        .enqueue_task(
            "Persisted child B".to_string(),
            "Inspect auth risks".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            Some(parent.id.clone()),
            Some("thread-parent".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    child_a.status = crate::agent::types::TaskStatus::InProgress;
    child_b.status = crate::agent::types::TaskStatus::InProgress;
    {
        let mut tasks = engine.tasks.lock().await;
        if let Some(existing) = tasks.iter_mut().find(|task| task.id == child_a.id) {
            *existing = child_a.clone();
        }
        if let Some(existing) = tasks.iter_mut().find(|task| task.id == child_b.id) {
            *existing = child_b.clone();
        }
    }
    engine.persist_tasks().await;
    engine.tasks.lock().await.clear();

    let result = super::execute_list_subagents(
        &serde_json::json!({ "status": "in_progress" }),
        &engine,
        "thread-parent",
        Some(child_a.id.as_str()),
    )
    .await
    .expect("list_subagents should succeed");

    let payload: serde_json::Value = serde_json::from_str(&result).expect("valid JSON payload");
    let items = payload
        .as_array()
        .expect("list_subagents should return an array");
    let returned_ids = items
        .iter()
        .filter_map(|item| item.get("id").and_then(|value| value.as_str()))
        .collect::<std::collections::HashSet<_>>();

    assert!(
        returned_ids.contains(child_a.id.as_str()),
        "persisted child scope should include the current subagent via its parent tree"
    );
    assert!(
        returned_ids.contains(child_b.id.as_str()),
        "persisted child scope should include sibling subagents under the same parent task"
    );
}

#[tokio::test]
async fn handoff_thread_agent_push_updates_active_responder_and_writes_system_event() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);
    let thread_id = "thread-handoff-push";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            crate::agent::types::AgentThread {
                id: thread_id.to_string(),
                agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
                title: "Handoff candidate".to_string(),
                messages: vec![crate::agent::types::AgentMessage::user(
                    "Please ask Weles to review the risky migration.",
                    1,
                )],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: 1,
                updated_at: 1,
            },
        );
    }
    engine
        .set_thread_handoff_state(
            thread_id,
            crate::agent::ThreadHandoffState {
                origin_agent_id: crate::agent::agent_identity::MAIN_AGENT_ID.to_string(),
                active_agent_id: crate::agent::agent_identity::MAIN_AGENT_ID.to_string(),
                responder_stack: vec![crate::agent::ThreadResponderFrame {
                    agent_id: crate::agent::agent_identity::MAIN_AGENT_ID.to_string(),
                    agent_name: crate::agent::agent_identity::MAIN_AGENT_NAME.to_string(),
                    entered_at: 1,
                    entered_via_handoff_event_id: None,
                    linked_thread_id: None,
                }],
                events: Vec::new(),
                pending_approval_id: None,
            },
        )
        .await;

    let tool_call = ToolCall::with_default_weles_review(
            "tool-handoff-thread-push".to_string(),
            ToolFunction {
                name: "handoff_thread_agent".to_string(),
                arguments: serde_json::json!({
                    "action": "push_handoff",
                    "target_agent_id": "weles",
                    "reason": "Risky migration needs governance review",
                    "summary": "Review the migration plan, identify risk, and continue answering from Weles.",
                    "requested_by": "user"
                })
                .to_string(),
            },
        );

    let result = execute_tool(
        &tool_call,
        &engine,
        thread_id,
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(
        !result.is_error,
        "handoff push should succeed: {}",
        result.content
    );
    assert!(result.pending_approval.is_none());
    let state = engine
        .thread_handoff_state(thread_id)
        .await
        .expect("handoff state should exist");
    assert_eq!(state.active_agent_id, "weles");
    assert_eq!(state.responder_stack.len(), 2);

    let thread = engine
        .threads
        .read()
        .await
        .get(thread_id)
        .cloned()
        .expect("thread should exist");
    assert_eq!(thread.agent_name.as_deref(), Some("Weles"));
    let system_event = thread
        .messages
        .iter()
        .find(|message| message.role == crate::agent::types::MessageRole::System)
        .expect("handoff should append a system event");
    assert!(
        system_event.content.contains("[[handoff_event]]"),
        "system event should use the structured handoff marker"
    );
    assert!(system_event.content.contains("\"to_agent_name\":\"Weles\""));
}

#[tokio::test]
async fn participant_managed_handoff_rejects_non_participant_target() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);
    let thread_id = "thread-handoff-non-participant-rejected";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        crate::agent::types::AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Participant-managed handoff".to_string(),
            messages: vec![crate::agent::types::AgentMessage::user(
                "Hand off to a participant only.",
                1,
            )],
            pinned: false,
            upstream_thread_id: None,
            upstream_transport: None,
            upstream_provider: None,
            upstream_model: None,
            upstream_assistant_id: None,
            total_input_tokens: 0,
            total_output_tokens: 0,
            created_at: 1,
            updated_at: 1,
        },
    );
    engine
        .upsert_thread_participant(thread_id, "weles", "watch this thread")
        .await
        .expect("participant should register");
    engine
        .set_thread_handoff_state(
            thread_id,
            crate::agent::ThreadHandoffState {
                origin_agent_id: crate::agent::agent_identity::MAIN_AGENT_ID.to_string(),
                active_agent_id: crate::agent::agent_identity::MAIN_AGENT_ID.to_string(),
                responder_stack: vec![crate::agent::ThreadResponderFrame {
                    agent_id: crate::agent::agent_identity::MAIN_AGENT_ID.to_string(),
                    agent_name: crate::agent::agent_identity::MAIN_AGENT_NAME.to_string(),
                    entered_at: 1,
                    entered_via_handoff_event_id: None,
                    linked_thread_id: None,
                }],
                events: Vec::new(),
                pending_approval_id: None,
            },
        )
        .await;

    let tool_call = ToolCall::with_default_weles_review(
        "tool-handoff-thread-non-participant".to_string(),
        ToolFunction {
            name: "handoff_thread_agent".to_string(),
            arguments: serde_json::json!({
                "action": "push_handoff",
                "target_agent_id": "radogost",
                "reason": "Try handing to a non participant",
                "summary": "This should be rejected.",
                "requested_by": "user"
            })
            .to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        thread_id,
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(result.is_error, "non-participant handoff should fail");
    assert!(
        result.content.contains("participant"),
        "rejection should mention participant-only ownership: {}",
        result.content
    );
}

#[tokio::test]
async fn participant_managed_handoff_auto_registers_previous_responder() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);
    let thread_id = "thread-handoff-auto-registers-previous";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        crate::agent::types::AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Participant-managed handoff".to_string(),
            messages: vec![crate::agent::types::AgentMessage::user(
                "Hand off to Weles.",
                1,
            )],
            pinned: false,
            upstream_thread_id: None,
            upstream_transport: None,
            upstream_provider: None,
            upstream_model: None,
            upstream_assistant_id: None,
            total_input_tokens: 0,
            total_output_tokens: 0,
            created_at: 1,
            updated_at: 1,
        },
    );
    engine
        .upsert_thread_participant(thread_id, "weles", "watch this thread")
        .await
        .expect("participant should register");
    engine
        .set_thread_handoff_state(
            thread_id,
            crate::agent::ThreadHandoffState {
                origin_agent_id: crate::agent::agent_identity::MAIN_AGENT_ID.to_string(),
                active_agent_id: crate::agent::agent_identity::MAIN_AGENT_ID.to_string(),
                responder_stack: vec![crate::agent::ThreadResponderFrame {
                    agent_id: crate::agent::agent_identity::MAIN_AGENT_ID.to_string(),
                    agent_name: crate::agent::agent_identity::MAIN_AGENT_NAME.to_string(),
                    entered_at: 1,
                    entered_via_handoff_event_id: None,
                    linked_thread_id: None,
                }],
                events: Vec::new(),
                pending_approval_id: None,
            },
        )
        .await;

    let tool_call = ToolCall::with_default_weles_review(
        "tool-handoff-thread-auto-register".to_string(),
        ToolFunction {
            name: "handoff_thread_agent".to_string(),
            arguments: serde_json::json!({
                "action": "push_handoff",
                "target_agent_id": "weles",
                "reason": "Participant takeover",
                "summary": "Let Weles own future replies.",
                "requested_by": "user"
            })
            .to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        thread_id,
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(
        !result.is_error,
        "participant handoff should succeed: {}",
        result.content
    );
    let participants = engine.list_thread_participants(thread_id).await;
    assert!(participants.iter().any(|participant| {
        participant
            .agent_id
            .eq_ignore_ascii_case(crate::agent::agent_identity::MAIN_AGENT_ID)
            && participant.status == crate::agent::ThreadParticipantStatus::Active
    }));
}

#[tokio::test]
async fn handoff_thread_agent_push_accepts_svarog_alias_for_main_agent() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);
    let thread_id = "thread-handoff-push-svarog-alias";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            crate::agent::types::AgentThread {
                id: thread_id.to_string(),
                agent_name: Some(crate::agent::agent_identity::WELES_AGENT_NAME.to_string()),
                title: "Handoff alias candidate".to_string(),
                messages: vec![crate::agent::types::AgentMessage::user(
                    "Please hand this back to Svarog.",
                    1,
                )],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: 1,
                updated_at: 1,
            },
        );
    }
    engine
        .set_thread_handoff_state(
            thread_id,
            crate::agent::ThreadHandoffState {
                origin_agent_id: crate::agent::agent_identity::MAIN_AGENT_ID.to_string(),
                active_agent_id: crate::agent::agent_identity::WELES_AGENT_ID.to_string(),
                responder_stack: vec![
                    crate::agent::ThreadResponderFrame {
                        agent_id: crate::agent::agent_identity::MAIN_AGENT_ID.to_string(),
                        agent_name: crate::agent::agent_identity::MAIN_AGENT_NAME.to_string(),
                        entered_at: 1,
                        entered_via_handoff_event_id: None,
                        linked_thread_id: None,
                    },
                    crate::agent::ThreadResponderFrame {
                        agent_id: crate::agent::agent_identity::WELES_AGENT_ID.to_string(),
                        agent_name: crate::agent::agent_identity::WELES_AGENT_NAME.to_string(),
                        entered_at: 2,
                        entered_via_handoff_event_id: Some("handoff-existing".to_string()),
                        linked_thread_id: Some("handoff:existing".to_string()),
                    },
                ],
                events: Vec::new(),
                pending_approval_id: None,
            },
        )
        .await;

    let tool_call = ToolCall::with_default_weles_review(
        "tool-handoff-thread-push-svarog-alias".to_string(),
        ToolFunction {
            name: "handoff_thread_agent".to_string(),
            arguments: serde_json::json!({
                "action": "push_handoff",
                "target_agent_id": "svarog",
                "reason": "Operator requested to switch to Svarog",
                "summary": "Operator wants to continue with Svarog, the main agent.",
                "requested_by": "user"
            })
            .to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        thread_id,
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(
        !result.is_error,
        "svarog alias handoff should succeed: {}",
        result.content
    );
    let state = engine
        .thread_handoff_state(thread_id)
        .await
        .expect("handoff state should exist");
    assert_eq!(
        state.active_agent_id,
        crate::agent::agent_identity::MAIN_AGENT_ID
    );
}

#[tokio::test]
async fn handoff_thread_agent_agent_push_requires_approval_outside_yolo() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.managed_execution.security_level = SecurityLevel::Moderate;
    let engine = AgentEngine::new_test(manager.clone(), config, root.path()).await;
    let (event_tx, _) = broadcast::channel(8);
    let thread_id = "thread-handoff-approval";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            crate::agent::types::AgentThread {
                id: thread_id.to_string(),
                agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
                title: "Approval handoff".to_string(),
                messages: vec![crate::agent::types::AgentMessage::user(
                    "Maybe ask Weles to take over this risky thread.",
                    1,
                )],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: 1,
                updated_at: 1,
            },
        );
    }
    engine
        .set_thread_handoff_state(
            thread_id,
            crate::agent::ThreadHandoffState {
                origin_agent_id: crate::agent::agent_identity::MAIN_AGENT_ID.to_string(),
                active_agent_id: crate::agent::agent_identity::MAIN_AGENT_ID.to_string(),
                responder_stack: vec![crate::agent::ThreadResponderFrame {
                    agent_id: crate::agent::agent_identity::MAIN_AGENT_ID.to_string(),
                    agent_name: crate::agent::agent_identity::MAIN_AGENT_NAME.to_string(),
                    entered_at: 1,
                    entered_via_handoff_event_id: None,
                    linked_thread_id: None,
                }],
                events: Vec::new(),
                pending_approval_id: None,
            },
        )
        .await;

    let tool_call = ToolCall::with_default_weles_review(
        "tool-handoff-thread-approval".to_string(),
        ToolFunction {
            name: "handoff_thread_agent".to_string(),
            arguments: serde_json::json!({
                "action": "push_handoff",
                "target_agent_id": "weles",
                "reason": "Governance review required",
                "summary": "Let Weles take over and continue this safety-sensitive thread.",
                "requested_by": "agent"
            })
            .to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        thread_id,
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(!result.is_error, "approval handoff should not hard-fail");
    let pending = result
        .pending_approval
        .as_ref()
        .expect("agent-initiated handoff should request approval");
    assert!(pending.command.contains("handoff_thread_agent"));

    let state = engine
        .thread_handoff_state(thread_id)
        .await
        .expect("handoff state should still exist");
    assert_eq!(
        state.active_agent_id,
        crate::agent::agent_identity::MAIN_AGENT_ID,
        "active responder should not switch before approval"
    );
    assert_eq!(
        state.pending_approval_id.as_deref(),
        Some(pending.approval_id.as_str())
    );

    let tasks = engine.list_tasks().await;
    assert_eq!(tasks.len(), 1);
    assert_eq!(
        tasks[0].status,
        crate::agent::types::TaskStatus::AwaitingApproval
    );
    assert_eq!(tasks[0].source, "thread_handoff");
    assert_eq!(
        tasks[0].awaiting_approval_id.as_deref(),
        Some(pending.approval_id.as_str())
    );
}

#[tokio::test]
async fn handoff_thread_agent_return_pops_stack_and_restores_previous_responder() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);
    let thread_id = "thread-handoff-return";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            crate::agent::types::AgentThread {
                id: thread_id.to_string(),
                agent_name: Some(crate::agent::agent_identity::WELES_AGENT_NAME.to_string()),
                title: "Return handoff".to_string(),
                messages: vec![crate::agent::types::AgentMessage::user(
                    "Let Weles finish and then hand the thread back.",
                    1,
                )],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: 1,
                updated_at: 1,
            },
        );
    }
    engine
        .set_thread_handoff_state(
            thread_id,
            crate::agent::ThreadHandoffState {
                origin_agent_id: crate::agent::agent_identity::MAIN_AGENT_ID.to_string(),
                active_agent_id: crate::agent::agent_identity::WELES_AGENT_ID.to_string(),
                responder_stack: vec![
                    crate::agent::ThreadResponderFrame {
                        agent_id: crate::agent::agent_identity::MAIN_AGENT_ID.to_string(),
                        agent_name: crate::agent::agent_identity::MAIN_AGENT_NAME.to_string(),
                        entered_at: 1,
                        entered_via_handoff_event_id: None,
                        linked_thread_id: None,
                    },
                    crate::agent::ThreadResponderFrame {
                        agent_id: crate::agent::agent_identity::WELES_AGENT_ID.to_string(),
                        agent_name: crate::agent::agent_identity::WELES_AGENT_NAME.to_string(),
                        entered_at: 2,
                        entered_via_handoff_event_id: Some("handoff-existing".to_string()),
                        linked_thread_id: Some("handoff:existing".to_string()),
                    },
                ],
                events: Vec::new(),
                pending_approval_id: None,
            },
        )
        .await;

    let tool_call = ToolCall::with_default_weles_review(
        "tool-handoff-thread-return".to_string(),
        ToolFunction {
            name: "handoff_thread_agent".to_string(),
            arguments: serde_json::json!({
                "action": "return_handoff",
                "reason": "Weles completed the governance pass",
                "summary": "Returning control to Swarog with the review result and next steps.",
                "requested_by": "agent"
            })
            .to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        thread_id,
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(
        !result.is_error,
        "handoff return should succeed: {}",
        result.content
    );
    assert!(result.pending_approval.is_none());
    let state = engine
        .thread_handoff_state(thread_id)
        .await
        .expect("handoff state should exist");
    assert_eq!(
        state.active_agent_id,
        crate::agent::agent_identity::MAIN_AGENT_ID
    );
    assert_eq!(state.responder_stack.len(), 1);

    let thread = engine
        .threads
        .read()
        .await
        .get(thread_id)
        .cloned()
        .expect("thread should exist");
    assert_eq!(
        thread.agent_name.as_deref(),
        Some(crate::agent::agent_identity::MAIN_AGENT_NAME)
    );
    let system_event = thread
        .messages
        .iter()
        .find(|message| message.role == crate::agent::types::MessageRole::System)
        .expect("return handoff should append a system event");
    assert!(system_event.content.contains("\"kind\":\"return\""));
    assert!(
        system_event.content.contains(&format!(
            "\"to_agent_name\":\"{}\"",
            crate::agent::agent_identity::MAIN_AGENT_NAME
        )),
        "system event should announce the restored responder"
    );
}

#[tokio::test]
async fn approved_thread_handoff_activation_updates_stack_and_thread_identity() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.managed_execution.security_level = SecurityLevel::Moderate;
    let engine = AgentEngine::new_test(manager.clone(), config, root.path()).await;
    let (event_tx, _) = broadcast::channel(8);
    let thread_id = "thread-handoff-approved";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            crate::agent::types::AgentThread {
                id: thread_id.to_string(),
                agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
                title: "Approve handoff".to_string(),
                messages: vec![crate::agent::types::AgentMessage::user(
                    "If needed, let Weles take over after approval.",
                    1,
                )],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: 1,
                updated_at: 1,
            },
        );
    }
    engine
        .set_thread_handoff_state(
            thread_id,
            crate::agent::ThreadHandoffState {
                origin_agent_id: crate::agent::agent_identity::MAIN_AGENT_ID.to_string(),
                active_agent_id: crate::agent::agent_identity::MAIN_AGENT_ID.to_string(),
                responder_stack: vec![crate::agent::ThreadResponderFrame {
                    agent_id: crate::agent::agent_identity::MAIN_AGENT_ID.to_string(),
                    agent_name: crate::agent::agent_identity::MAIN_AGENT_NAME.to_string(),
                    entered_at: 1,
                    entered_via_handoff_event_id: None,
                    linked_thread_id: None,
                }],
                events: Vec::new(),
                pending_approval_id: None,
            },
        )
        .await;

    engine
        .set_thread_execution_profile(
            thread_id,
            Some(crate::agent::types::ThreadExecutionProfile {
                provider: Some("previous-provider".to_string()),
                model: Some("previous-model".to_string()),
                reasoning_effort: Some("high".to_string()),
                context_window_tokens: Some(123_456),
            }),
        )
        .await;

    let tool_call = ToolCall::with_default_weles_review(
        "tool-handoff-thread-approved".to_string(),
        ToolFunction {
            name: "handoff_thread_agent".to_string(),
            arguments: serde_json::json!({
                "action": "push_handoff",
                "target_agent_id": "weles",
                "reason": "Governance review required",
                "summary": "Let Weles take over and continue this safety-sensitive thread.",
                "requested_by": "agent"
            })
            .to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        thread_id,
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    let pending = result
        .pending_approval
        .as_ref()
        .expect("agent-initiated handoff should request approval");
    assert!(
        engine
            .handle_task_approval_resolution(
                &pending.approval_id,
                zorai_protocol::ApprovalDecision::ApproveOnce
            )
            .await,
        "approval resolution should find the synthetic handoff task"
    );

    let state = engine
        .thread_handoff_state(thread_id)
        .await
        .expect("handoff state should exist");
    assert_eq!(
        state.active_agent_id,
        crate::agent::agent_identity::WELES_AGENT_ID
    );
    assert!(state.pending_approval_id.is_none());
    assert_eq!(state.responder_stack.len(), 2);

    let thread = engine
        .threads
        .read()
        .await
        .get(thread_id)
        .cloned()
        .expect("thread should exist");
    assert_eq!(thread.agent_name.as_deref(), Some("Weles"));
    assert!(
        thread.messages.iter().any(|message| {
            message.role == crate::agent::types::MessageRole::System
                && message.content.contains("\"approval_id\":\"")
        }),
        "approval activation should append a structured handoff system event"
    );

    let handoff_system_message = thread
        .messages
        .iter()
        .find(|message| {
            message.role == crate::agent::types::MessageRole::System
                && message.content.starts_with("[[handoff_event]]")
        })
        .map(|message| message.content.clone())
        .expect("handoff should append a [[handoff_event]] system message");
    assert!(
        handoff_system_message.contains("\"to_agent_name\":\"Weles\""),
        "handoff system message must carry the resolved responder name, got {handoff_system_message}"
    );
    assert!(
        handoff_system_message.contains(&format!(
            "\"from_agent_name\":\"{}\"",
            crate::agent::agent_identity::MAIN_AGENT_NAME
        )),
        "handoff system message must carry the resolved previous responder name, got {handoff_system_message}"
    );

    assert!(
        engine
            .thread_execution_profiles
            .read()
            .await
            .get(thread_id)
            .is_none(),
        "handoff must clear the previous responder's execution profile so the header reflects the new agent"
    );
}

#[tokio::test]
async fn get_todos_returns_thread_scoped_items_with_optional_task_id() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);
    let thread_id = "thread-get-todos";
    let task = engine
        .enqueue_task(
            "Track thread todos".to_string(),
            "Keep a live thread todo list available to the runtime.".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "agent",
            None,
            None,
            Some(thread_id.to_string()),
            Some("daemon".to_string()),
        )
        .await;

    let update_call = ToolCall::with_default_weles_review(
        "tool-update-todos-runtime".to_string(),
        ToolFunction {
            name: tool_names::UPDATE_TODO.to_string(),
            arguments: serde_json::json!({
                "items": [
                    { "content": "Inspect current thread todos", "status": "in_progress" },
                    { "content": "Return todo snapshot", "status": "pending" }
                ]
            })
            .to_string(),
        },
    );

    let update_result = execute_tool(
        &update_call,
        &engine,
        thread_id,
        Some(task.id.as_str()),
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(
        !update_result.is_error,
        "update_todo should succeed before get_todos is called: {}",
        update_result.content
    );

    let get_call = ToolCall::with_default_weles_review(
        "tool-get-todos-runtime".to_string(),
        ToolFunction {
            name: tool_names::GET_TODOS.to_string(),
            arguments: serde_json::json!({}).to_string(),
        },
    );

    let get_result = execute_tool(
        &get_call,
        &engine,
        thread_id,
        Some(task.id.as_str()),
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(
        !get_result.is_error,
        "get_todos should return thread-scoped items: {}",
        get_result.content
    );

    let payload: serde_json::Value =
        serde_json::from_str(&get_result.content).expect("get_todos should return JSON");
    assert_eq!(payload["thread_id"], thread_id);
    let items = payload["items"]
        .as_array()
        .expect("items should be serialized as an array");
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["content"], "Inspect current thread todos");
    assert_eq!(items[0]["status"], "in_progress");
    assert_eq!(items[1]["content"], "Return todo snapshot");
    assert_eq!(items[1]["status"], "pending");
}

#[tokio::test]
async fn get_todos_resolves_persisted_task_context_by_id() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-get-todos-persisted-task";
    let task = engine
        .enqueue_task(
            "Persisted todo context".to_string(),
            "Resolve todo metadata from SQLite after the live queue is gone.".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "agent",
            Some("goal-persisted-todos".to_string()),
            None,
            Some(thread_id.to_string()),
            Some("daemon".to_string()),
        )
        .await;
    engine.tasks.lock().await.clear();

    let payload = execute_get_todos(
        &serde_json::json!({
            "task_id": task.id,
        }),
        &engine,
        thread_id,
        None,
    )
    .await
    .expect("get_todos should resolve persisted task context");
    let payload: serde_json::Value =
        serde_json::from_str(&payload).expect("get_todos should return JSON");

    assert_eq!(payload["thread_id"], thread_id);
    assert_eq!(payload["task_id"], task.id);
    assert_eq!(payload["goal_run_id"], "goal-persisted-todos");
}

#[tokio::test]
async fn show_harness_state_returns_projected_sections_for_current_scope() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);
    let thread_id = "thread-show-harness-state";
    let task = engine
        .enqueue_task(
            "Inspect harness state".to_string(),
            "Return a persisted harness visibility payload for the current scope.".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "agent",
            None,
            None,
            Some(thread_id.to_string()),
            Some("daemon".to_string()),
        )
        .await;

    crate::agent::harness::run_minimal_closed_loop(
        &engine.history,
        crate::agent::harness::HarnessLoopInput {
            thread_id: Some(thread_id.to_string()),
            goal_run_id: Some("goal-show-harness-state".to_string()),
            task_id: Some(task.id.clone()),
            observation_summary: "surface persisted harness state for tooling".to_string(),
            observation_details: serde_json::json!({
                "state": {"actual": "aligned"},
                "opportunities": ["Expose the harness state to developers"],
            }),
            goal_summary: "ship a harness visibility surface".to_string(),
            desired_state: Some(serde_json::json!({"actual": "aligned"})),
            preferred_effect_kind: Some(crate::agent::harness::EffectExecutionKind::ReadOnly),
            allow_network: false,
            sandbox_enabled: true,
        },
    )
    .await
    .expect("harness loop should succeed");

    let tool_call = ToolCall::with_default_weles_review(
        "tool-show-harness-state".to_string(),
        ToolFunction {
            name: "show_harness_state".to_string(),
            arguments: serde_json::json!({
                "limit": 3,
            })
            .to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        thread_id,
        Some(task.id.as_str()),
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(
        !result.is_error,
        "show_harness_state should succeed: {}",
        result.content
    );

    let payload: serde_json::Value =
        serde_json::from_str(&result.content).expect("show_harness_state should return JSON");
    assert_eq!(payload["scope"]["thread_id"], thread_id);
    assert_eq!(payload["scope"]["task_id"], task.id);
    assert!(payload["world_state"]["latest"].is_object());
    assert!(payload["tensions"]["active"]
        .as_array()
        .is_some_and(|items| !items.is_empty()));
    assert!(payload["procedures"]["latest"].is_object());
    assert!(payload["verifications"]["latest"]["verified"].is_boolean());
}

#[tokio::test]
async fn show_harness_state_resolves_explicit_task_id_from_persisted_row() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);
    let thread_id = "thread-show-harness-state-persisted-task";
    let task = engine
        .enqueue_task(
            "Inspect persisted harness task".to_string(),
            "Return harness state for an explicit persisted task id.".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "agent",
            None,
            None,
            Some(thread_id.to_string()),
            Some("daemon".to_string()),
        )
        .await;

    crate::agent::harness::run_minimal_closed_loop(
        &engine.history,
        crate::agent::harness::HarnessLoopInput {
            thread_id: Some(thread_id.to_string()),
            goal_run_id: Some("goal-show-harness-state-persisted-task".to_string()),
            task_id: Some(task.id.clone()),
            observation_summary: "persisted task rows should resolve tool scope".to_string(),
            observation_details: serde_json::json!({
                "state": {"actual": "persisted"},
            }),
            goal_summary: "avoid broad task snapshots for exact task lookups".to_string(),
            desired_state: Some(serde_json::json!({"actual": "persisted"})),
            preferred_effect_kind: Some(crate::agent::harness::EffectExecutionKind::ReadOnly),
            allow_network: false,
            sandbox_enabled: true,
        },
    )
    .await
    .expect("harness loop should succeed");

    engine.tasks.lock().await.clear();

    let tool_call = ToolCall::with_default_weles_review(
        "tool-show-harness-state-persisted-task".to_string(),
        ToolFunction {
            name: "show_harness_state".to_string(),
            arguments: serde_json::json!({
                "task_id": task.id,
                "limit": 3,
            })
            .to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        thread_id,
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(
        !result.is_error,
        "show_harness_state should resolve the task from sqlite: {}",
        result.content
    );

    let payload: serde_json::Value =
        serde_json::from_str(&result.content).expect("show_harness_state should return JSON");
    assert_eq!(payload["scope"]["task_id"], task.id);
}

#[tokio::test]
async fn execute_tool_records_timeout_adaptation_in_weles_review_for_fetch_url() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);

    {
        let mut model = engine.operator_model.write().await;
        model.operator_satisfaction.label = "strained".to_string();
        model.operator_satisfaction.score = 0.18;
    }

    let tool_call = ToolCall::with_default_weles_review(
        "tool-fetch-url-adapted-timeout".to_string(),
        ToolFunction {
            name: "fetch_url".to_string(),
            arguments: serde_json::json!({
                "url": "https://example.com",
                "max_length": 64
            })
            .to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        "thread-fetch-url-adapted-timeout",
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    let reasons = result
        .weles_review
        .as_ref()
        .expect("weles review should exist")
        .reasons
        .clone();
    assert!(
        reasons
            .iter()
            .any(|reason| reason == "adapted_runtime:timeout:fetch_url:180"),
        "expected timeout adaptation reason, got {reasons:?}"
    );
}

#[tokio::test]
async fn execute_tool_records_timeout_adaptation_in_weles_review_for_search_files_under_fragile_feedback(
) {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);

    {
        let mut model = engine.operator_model.write().await;
        model.cognitive_style.message_count = 1;
        model.implicit_feedback.tool_hesitation_count = 1;
        model.implicit_feedback.correction_message_count = 1;
        crate::agent::operator_model::refresh_operator_satisfaction(&mut model);
        assert_eq!(model.operator_satisfaction.label, "fragile");
    }

    let tool_call = ToolCall::with_default_weles_review(
        "tool-search-files-adapted-timeout".to_string(),
        ToolFunction {
            name: "search_files".to_string(),
            arguments: serde_json::json!({
                "path": root.path(),
                "pattern": "needle"
            })
            .to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        "thread-search-files-adapted-timeout",
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    let reasons = result
        .weles_review
        .as_ref()
        .expect("weles review should exist")
        .reasons
        .clone();
    assert!(
        reasons
            .iter()
            .any(|reason| reason == "adapted_runtime:timeout:search_files:90"),
        "expected search_files timeout adaptation reason, got {reasons:?}"
    );
}

#[tokio::test]
async fn update_browser_profile_health_emits_repair_notice_for_repair_needed() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, mut event_rx) = broadcast::channel(8);

    engine
        .history
        .upsert_browser_profile(&crate::agent::types::BrowserProfile {
            profile_id: "browser-profile-main".to_string(),
            label: "Main Browser".to_string(),
            profile_dir: "/tmp/browser-profile-main".to_string(),
            browser_kind: Some("chrome".to_string()),
            workspace_id: None,
            health_state: crate::agent::types::BrowserProfileHealth::Healthy,
            created_at: 1_777_230_000,
            updated_at: 1_777_230_100,
            last_used_at: None,
            last_auth_success_at: None,
            last_auth_failure_at: None,
            last_auth_failure_reason: None,
        })
        .await
        .expect("profile should persist");

    let result = execute_tool(
        &ToolCall::with_default_weles_review(
            "tool-update-browser-profile-health-repair-needed".to_string(),
            ToolFunction {
                name: "update_browser_profile_health".to_string(),
                arguments: serde_json::json!({
                    "profile_id": "browser-profile-main",
                    "health_state": "repair_needed",
                    "last_auth_failure_at": 1_777_230_200u64,
                    "last_auth_failure_reason": "cookies expired"
                })
                .to_string(),
            },
        ),
        &engine,
        "thread-browser-profile-repair-needed",
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(
        !result.is_error,
        "update_browser_profile_health should succeed: {}",
        result.content
    );

    let notice = timeout(Duration::from_millis(250), event_rx.recv())
        .await
        .expect("expected browser profile repair workflow notice")
        .expect("workflow notice should be received");
    match notice {
        AgentEvent::WorkflowNotice {
            kind,
            message,
            details: Some(details),
            ..
        } => {
            assert_eq!(kind, "browser-profile-repair");
            assert!(message.contains("browser-profile-main"));
            assert!(message.contains("needs repair"));
            assert!(message.contains("cookies expired"));
            let details: serde_json::Value =
                serde_json::from_str(&details).expect("notice details should be json");
            assert_eq!(
                details.get("profile_id").and_then(|value| value.as_str()),
                Some("browser-profile-main")
            );
            assert_eq!(
                details.get("health_state").and_then(|value| value.as_str()),
                Some("repair_needed")
            );
            assert_eq!(
                details
                    .get("last_auth_failure_reason")
                    .and_then(|value| value.as_str()),
                Some("cookies expired")
            );
        }
        other => panic!("expected workflow notice, got {other:?}"),
    }
}

#[tokio::test]
async fn list_browser_profiles_tool_returns_reclassified_expired_profiles() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);
    let now_ms = crate::agent::now_millis();
    let sixty_days_ago = now_ms.saturating_sub(60 * 24 * 60 * 60 * 1000);

    engine
        .history
        .upsert_browser_profile(&crate::agent::types::BrowserProfile {
            profile_id: "expired-work".to_string(),
            label: "Expired Work Profile".to_string(),
            profile_dir: "/tmp/zorai/browser/expired-work".to_string(),
            browser_kind: Some("chrome".to_string()),
            workspace_id: None,
            health_state: crate::agent::types::BrowserProfileHealth::Healthy,
            created_at: sixty_days_ago,
            updated_at: sixty_days_ago,
            last_used_at: Some(sixty_days_ago),
            last_auth_success_at: Some(sixty_days_ago),
            last_auth_failure_at: None,
            last_auth_failure_reason: None,
        })
        .await
        .expect("profile should persist");

    let result = execute_tool(
        &ToolCall::with_default_weles_review(
            "tool-list-browser-profiles-expired".to_string(),
            ToolFunction {
                name: "list_browser_profiles".to_string(),
                arguments: serde_json::json!({}).to_string(),
            },
        ),
        &engine,
        "thread-list-browser-profiles-expired",
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(
        !result.is_error,
        "list_browser_profiles should succeed: {}",
        result.content
    );

    let payload: serde_json::Value =
        serde_json::from_str(&result.content).expect("list_browser_profiles should return JSON");
    let profiles = payload
        .as_array()
        .expect("profiles should serialize as an array");
    let expired = profiles
        .iter()
        .find(|profile| profile["profile_id"] == serde_json::json!("expired-work"))
        .expect("expired profile should be listed");
    assert_eq!(expired["health_state"], serde_json::json!("expired"));
}

#[tokio::test]
async fn update_browser_profile_health_does_not_emit_repair_notice_for_healthy() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, mut event_rx) = broadcast::channel(8);

    engine
        .history
        .upsert_browser_profile(&crate::agent::types::BrowserProfile {
            profile_id: "browser-profile-main".to_string(),
            label: "Main Browser".to_string(),
            profile_dir: "/tmp/browser-profile-main".to_string(),
            browser_kind: Some("chrome".to_string()),
            workspace_id: None,
            health_state: crate::agent::types::BrowserProfileHealth::RepairInProgress,
            created_at: 1_777_230_000,
            updated_at: 1_777_230_100,
            last_used_at: None,
            last_auth_success_at: None,
            last_auth_failure_at: Some(1_777_230_200),
            last_auth_failure_reason: Some("cookies expired".to_string()),
        })
        .await
        .expect("profile should persist");

    let result = execute_tool(
        &ToolCall::with_default_weles_review(
            "tool-update-browser-profile-health-healthy".to_string(),
            ToolFunction {
                name: "update_browser_profile_health".to_string(),
                arguments: serde_json::json!({
                    "profile_id": "browser-profile-main",
                    "health_state": "healthy",
                    "last_auth_success_at": 1_777_230_300u64
                })
                .to_string(),
            },
        ),
        &engine,
        "thread-browser-profile-healthy",
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(
        !result.is_error,
        "update_browser_profile_health should succeed: {}",
        result.content
    );

    assert!(
        timeout(Duration::from_millis(150), event_rx.recv())
            .await
            .is_err(),
        "healthy browser profile updates should not emit repair notices"
    );
}
