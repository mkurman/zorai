use super::*;
use crate::codec::{DaemonCodec, ZoraiCodec};
use bytes::BytesMut;
use serde::Serialize;
use tokio_util::codec::{Decoder, Encoder};

fn assert_bincode_variant_index<T: Serialize>(value: &T, expected_index: u32) {
    let bytes = bincode::serialize(value).unwrap();
    assert!(
        bytes.len() >= 4,
        "bincode payload must include a variant index"
    );
    assert_eq!(
        &bytes[..4],
        &expected_index.to_le_bytes(),
        "variant index changed in the wire format"
    );
}

fn bincode_variant_index<T: Serialize>(value: &T) -> u32 {
    let bytes = bincode::serialize(value).unwrap();
    u32::from_le_bytes(bytes[..4].try_into().unwrap())
}

#[test]
fn agent_provider_validation_bincode_roundtrip() {
    let msg = DaemonMessage::AgentProviderValidation {
        operation_id: None,
        provider_id: "openrouter".to_string(),
        valid: true,
        error: None,
        models_json: None,
    };
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
    match decoded {
        DaemonMessage::AgentProviderValidation {
            operation_id,
            provider_id,
            valid,
            error,
            models_json,
        } => {
            assert!(operation_id.is_none());
            assert_eq!(provider_id, "openrouter");
            assert!(valid);
            assert!(error.is_none());
            assert!(models_json.is_none());
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}

#[test]
fn agent_provider_validation_codec_roundtrip() {
    let msg = DaemonMessage::AgentProviderValidation {
        operation_id: Some("op-provider-1".to_string()),
        provider_id: "openrouter".to_string(),
        valid: true,
        error: None,
        models_json: None,
    };
    let mut daemon_codec = DaemonCodec;
    let mut buf = BytesMut::new();
    daemon_codec.encode(msg.clone(), &mut buf).unwrap();
    let mut client_codec = ZoraiCodec;
    let decoded = client_codec.decode(&mut buf).unwrap().unwrap();
    match decoded {
        DaemonMessage::AgentProviderValidation {
            operation_id,
            provider_id,
            valid,
            error,
            models_json,
        } => {
            assert_eq!(operation_id.as_deref(), Some("op-provider-1"));
            assert_eq!(provider_id, "openrouter");
            assert!(valid);
            assert!(error.is_none());
            assert!(models_json.is_none());
        }
        other => panic!("decoded wrong variant: {:?}", other),
    }
}

#[test]
fn daemon_message_roundtrips_provider_validation_with_operation_id() {
    let msg = DaemonMessage::AgentProviderValidation {
        operation_id: Some("op-provider-1".to_string()),
        provider_id: "openrouter".to_string(),
        valid: false,
        error: Some("invalid api key".to_string()),
        models_json: None,
    };
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
    match decoded {
        DaemonMessage::AgentProviderValidation {
            operation_id,
            provider_id,
            valid,
            error,
            ..
        } => {
            assert_eq!(operation_id.as_deref(), Some("op-provider-1"));
            assert_eq!(provider_id, "openrouter");
            assert!(!valid);
            assert_eq!(error.as_deref(), Some("invalid api key"));
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}

#[test]
fn daemon_message_roundtrips_models_response_with_operation_id() {
    let msg = DaemonMessage::AgentModelsResponse {
        operation_id: Some("op-models-1".to_string()),
        models_json: r#"[{"id":"gpt-5.4","label":"GPT-5.4"}]"#.to_string(),
    };
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
    match decoded {
        DaemonMessage::AgentModelsResponse {
            operation_id,
            models_json,
        } => {
            assert_eq!(operation_id.as_deref(), Some("op-models-1"));
            assert!(models_json.contains("gpt-5.4"));
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}

#[test]
fn daemon_message_roundtrips_goal_run_detail_placeholder_payload() {
    let msg = DaemonMessage::AgentGoalRunDetail {
        goal_run_json: serde_json::json!({
            "id": "goal-1",
        })
        .to_string(),
    };
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
    match decoded {
        DaemonMessage::AgentGoalRunDetail { goal_run_json } => {
            let goal_run: serde_json::Value = serde_json::from_str(&goal_run_json).unwrap();
            assert_eq!(
                goal_run.get("id").and_then(serde_json::Value::as_str),
                Some("goal-1")
            );
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}

#[test]
fn daemon_message_roundtrips_empty_goal_checkpoint_list_with_goal_id() {
    let msg = DaemonMessage::AgentCheckpointList {
        goal_run_id: "goal-1".to_string(),
        checkpoints_json: "[]".to_string(),
    };
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
    match decoded {
        DaemonMessage::AgentCheckpointList {
            goal_run_id,
            checkpoints_json,
        } => {
            assert_eq!(goal_run_id, "goal-1");
            assert_eq!(checkpoints_json, "[]");
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}

#[test]
fn daemon_message_roundtrips_agent_tool_list() {
    let msg = DaemonMessage::AgentToolList {
        result: ToolListResultPublic {
            total: 1,
            limit: 20,
            offset: 0,
            items: vec![ToolDescriptorPublic {
                name: "read_file".to_string(),
                description: "Read file contents".to_string(),
                required: vec!["path".to_string()],
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" }
                    },
                    "required": ["path"]
                })
                .to_string(),
            }],
        },
    };

    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
    match decoded {
        DaemonMessage::AgentToolList { result } => {
            assert_eq!(result.total, 1);
            assert_eq!(result.items.len(), 1);
            assert_eq!(result.items[0].name, "read_file");
            let parameters: serde_json::Value =
                serde_json::from_str(&result.items[0].parameters).unwrap();
            assert_eq!(
                parameters
                    .get("required")
                    .and_then(|value| value.as_array())
                    .map(|items| items.len()),
                Some(1)
            );
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}

#[test]
fn workspace_task_payload_round_trips() {
    let task = WorkspaceTask {
        id: "wtask_1".into(),
        workspace_id: "workspace-main".into(),
        title: "Ship workspace board".into(),
        task_type: WorkspaceTaskType::Goal,
        description: "Implement daemon and TUI board".into(),
        definition_of_done: Some("Four columns render live".into()),
        priority: WorkspacePriority::Low,
        status: WorkspaceTaskStatus::Todo,
        sort_order: 10,
        reporter: WorkspaceActor::User,
        assignee: Some(WorkspaceActor::Agent(AGENT_ID_SWAROG.into())),
        reviewer: Some(WorkspaceActor::User),
        thread_id: None,
        goal_run_id: None,
        runtime_history: Vec::new(),
        created_at: 1,
        updated_at: 2,
        started_at: None,
        completed_at: None,
        deleted_at: None,
        last_notice_id: None,
    };

    let json = serde_json::to_string(&task).unwrap();
    let decoded: WorkspaceTask = serde_json::from_str(&json).unwrap();

    assert_eq!(decoded.task_type, WorkspaceTaskType::Goal);
    assert_eq!(decoded.status, WorkspaceTaskStatus::Todo);
    assert_eq!(decoded.priority, WorkspacePriority::Low);
    assert_eq!(
        decoded.assignee,
        Some(WorkspaceActor::Agent(AGENT_ID_SWAROG.into()))
    );
}

#[test]
fn client_message_roundtrips_create_workspace_task() {
    let actor = WorkspaceActor::Agent(AGENT_ID_SWAROG.into());
    let actor_bytes = bincode::serialize(&actor).unwrap();
    let decoded_actor: WorkspaceActor = bincode::deserialize(&actor_bytes).unwrap();
    assert_eq!(decoded_actor, actor);

    let request = WorkspaceTaskCreate {
        workspace_id: "workspace-main".into(),
        title: "Write daemon store".into(),
        task_type: WorkspaceTaskType::Thread,
        description: "Persist workspace tasks in SQLite".into(),
        definition_of_done: None,
        priority: None,
        assignee: Some(WorkspaceActor::Agent(AGENT_ID_SWAROG.into())),
        reviewer: Some(WorkspaceActor::User),
    };
    let request_bytes = bincode::serialize(&request).unwrap();
    let decoded_request: WorkspaceTaskCreate = bincode::deserialize(&request_bytes).unwrap();
    assert_eq!(decoded_request.task_type, WorkspaceTaskType::Thread);

    let msg = ClientMessage::AgentCreateWorkspaceTask { request };

    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: ClientMessage = bincode::deserialize(&bytes).unwrap();

    match decoded {
        ClientMessage::AgentCreateWorkspaceTask { request } => {
            assert_eq!(request.workspace_id, "workspace-main");
            assert_eq!(request.task_type, WorkspaceTaskType::Thread);
            assert_eq!(request.priority, None);
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}

#[test]
fn daemon_message_roundtrips_workspace_task_update() {
    let msg = DaemonMessage::AgentWorkspaceTaskUpdated {
        task: WorkspaceTask {
            id: "wtask_1".into(),
            workspace_id: "workspace-main".into(),
            title: "Review workspace schema".into(),
            task_type: WorkspaceTaskType::Goal,
            description: "Check DB tables".into(),
            definition_of_done: None,
            priority: WorkspacePriority::Low,
            status: WorkspaceTaskStatus::InReview,
            sort_order: 20,
            reporter: WorkspaceActor::User,
            assignee: None,
            reviewer: Some(WorkspaceActor::Agent(AGENT_ID_SWAROG.into())),
            thread_id: None,
            goal_run_id: Some("goal_1".into()),
            runtime_history: Vec::new(),
            created_at: 1,
            updated_at: 3,
            started_at: Some(2),
            completed_at: None,
            deleted_at: None,
            last_notice_id: Some("wnotice_1".into()),
        },
    };

    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();

    match decoded {
        DaemonMessage::AgentWorkspaceTaskUpdated { task } => {
            assert_eq!(task.id, "wtask_1");
            assert_eq!(task.status, WorkspaceTaskStatus::InReview);
            assert_eq!(task.goal_run_id.as_deref(), Some("goal_1"));
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}

#[test]
fn daemon_message_roundtrips_agent_tool_search_result() {
    let msg = DaemonMessage::AgentToolSearchResult {
        result: ToolSearchResultPublic {
            query: "file".to_string(),
            total: 1,
            limit: 20,
            offset: 0,
            items: vec![ToolSearchMatchPublic {
                name: "read_file".to_string(),
                description: "Read file contents".to_string(),
                required: vec!["path".to_string()],
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" }
                    }
                })
                .to_string(),
                score: 91,
                matched_fields: vec!["name".to_string()],
            }],
        },
    };

    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
    match decoded {
        DaemonMessage::AgentToolSearchResult { result } => {
            assert_eq!(result.query, "file");
            assert_eq!(result.items.len(), 1);
            assert_eq!(result.items[0].score, 91);
            let parameters: serde_json::Value =
                serde_json::from_str(&result.items[0].parameters).unwrap();
            assert_eq!(
                parameters
                    .get("properties")
                    .and_then(|value| value.as_object())
                    .map(|properties| properties.contains_key("path")),
                Some(true)
            );
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}

#[test]
fn daemon_message_roundtrips_agent_generated_tool_result_with_operation_id() {
    let msg = DaemonMessage::AgentGeneratedToolResult {
        operation_id: Some("op-tool-1".to_string()),
        tool_name: None,
        result_json: r#"{"id":"generated_echo","status":"new"}"#.to_string(),
    };
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
    match decoded {
        DaemonMessage::AgentGeneratedToolResult {
            operation_id,
            tool_name,
            result_json,
        } => {
            assert_eq!(operation_id.as_deref(), Some("op-tool-1"));
            assert!(tool_name.is_none());
            assert!(result_json.contains("generated_echo"));
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}

#[test]
fn daemon_message_roundtrips_agent_divergent_session_started_with_operation_id() {
    let msg = DaemonMessage::AgentDivergentSessionStarted {
        operation_id: Some("op-divergent-1".to_string()),
        session_json: serde_json::json!({
            "session_id": "div-123",
            "status": "started",
        })
        .to_string(),
    };
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
    match decoded {
        DaemonMessage::AgentDivergentSessionStarted {
            operation_id,
            session_json,
        } => {
            assert_eq!(operation_id.as_deref(), Some("op-divergent-1"));
            let payload: serde_json::Value = serde_json::from_str(&session_json).unwrap();
            assert_eq!(payload["session_id"], "div-123");
            assert_eq!(payload["status"], "started");
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}

#[test]
fn client_message_supports_participant_suggestion_send() {
    let msg = ClientMessage::AgentSendParticipantSuggestion {
        thread_id: "thread-1".to_string(),
        suggestion_id: "sugg-1".to_string(),
        session_id: None,
        client_surface: None,
        force_send: false,
    };
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: ClientMessage = bincode::deserialize(&bytes).unwrap();
    assert!(matches!(
        decoded,
        ClientMessage::AgentSendParticipantSuggestion {
            thread_id,
            suggestion_id,
            force_send,
            ..
        } if thread_id == "thread-1" && suggestion_id == "sugg-1" && !force_send
    ));
}

#[test]
fn client_message_supports_participant_suggestion_dismiss() {
    let msg = ClientMessage::AgentDismissParticipantSuggestion {
        thread_id: "thread-1".to_string(),
        suggestion_id: "sugg-1".to_string(),
        session_id: None,
        client_surface: None,
    };
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: ClientMessage = bincode::deserialize(&bytes).unwrap();
    assert!(matches!(
        decoded,
        ClientMessage::AgentDismissParticipantSuggestion {
            thread_id,
            suggestion_id,
            ..
        } if thread_id == "thread-1" && suggestion_id == "sugg-1"
    ));
}

#[test]
fn client_message_roundtrips_agent_handoff_thread() {
    let msg = ClientMessage::AgentHandoffThread {
        thread_id: "thread-1".to_string(),
        action: "push_handoff".to_string(),
        target_agent_id: Some("rarog".to_string()),
        reason: "operator requested concierge".to_string(),
        summary: "Continue the visible thread.".to_string(),
        requested_by: "user".to_string(),
        session_id: Some("session-1".to_string()),
        client_surface: Some(ClientSurface::Electron),
    };
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: ClientMessage = bincode::deserialize(&bytes).unwrap();
    match decoded {
        ClientMessage::AgentHandoffThread {
            thread_id,
            action,
            target_agent_id,
            reason,
            summary,
            requested_by,
            session_id,
            client_surface,
        } => {
            assert_eq!(thread_id, "thread-1");
            assert_eq!(action, "push_handoff");
            assert_eq!(target_agent_id.as_deref(), Some("rarog"));
            assert_eq!(reason, "operator requested concierge");
            assert_eq!(summary, "Continue the visible thread.");
            assert_eq!(requested_by, "user");
            assert_eq!(session_id.as_deref(), Some("session-1"));
            assert_eq!(client_surface, Some(ClientSurface::Electron));
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}

#[test]
fn client_message_agent_handoff_thread_defaults_surface() {
    let decoded: ClientMessage = serde_json::from_value(serde_json::json!({
        "AgentHandoffThread": {
            "thread_id": "thread-1",
            "action": "return_handoff",
            "target_agent_id": null,
            "reason": "done",
            "summary": "Returning to previous agent.",
            "requested_by": "agent",
            "session_id": null
        }
    }))
    .unwrap();

    match decoded {
        ClientMessage::AgentHandoffThread { client_surface, .. } => {
            assert_eq!(client_surface, None);
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}

#[test]
fn daemon_message_roundtrips_thread_handoff_result() {
    let msg = DaemonMessage::AgentThreadHandoffResult {
        result: ThreadHandoffResult {
            ok: true,
            thread_id: "thread-1".to_string(),
            active_agent_id: Some("rarog".to_string()),
            stack_depth: Some(2),
            error: None,
        },
    };
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
    match decoded {
        DaemonMessage::AgentThreadHandoffResult { result } => {
            assert!(result.ok);
            assert_eq!(result.thread_id, "thread-1");
            assert_eq!(result.active_agent_id.as_deref(), Some("rarog"));
            assert_eq!(result.stack_depth, Some(2));
            assert!(result.error.is_none());
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}

#[test]
fn client_message_roundtrips_participant_suggestion_force_send() {
    let msg = ClientMessage::AgentSendParticipantSuggestion {
        thread_id: "thread-1".to_string(),
        suggestion_id: "sugg-1".to_string(),
        session_id: None,
        client_surface: Some(ClientSurface::Electron),
        force_send: true,
    };
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: ClientMessage = bincode::deserialize(&bytes).unwrap();
    match decoded {
        ClientMessage::AgentSendParticipantSuggestion {
            thread_id,
            suggestion_id,
            client_surface,
            force_send,
            ..
        } => {
            assert_eq!(thread_id, "thread-1");
            assert_eq!(suggestion_id, "sugg-1");
            assert_eq!(client_surface, Some(ClientSurface::Electron));
            assert!(force_send);
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}

#[test]
fn client_message_participant_suggestion_defaults_force_send() {
    let decoded: ClientMessage = serde_json::from_value(serde_json::json!({
        "AgentSendParticipantSuggestion": {
            "thread_id": "thread-1",
            "suggestion_id": "sugg-1",
            "session_id": null,
            "client_surface": null
        }
    }))
    .unwrap();

    match decoded {
        ClientMessage::AgentSendParticipantSuggestion { force_send, .. } => {
            assert!(!force_send);
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}

#[test]
fn client_message_roundtrips_agent_semantic_query() {
    let msg = ClientMessage::AgentSemanticQuery {
        args_json: serde_json::json!({
            "kind": "packages",
            "root": "/tmp/workspace",
            "limit": 10,
        })
        .to_string(),
    };
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: ClientMessage = bincode::deserialize(&bytes).unwrap();
    match decoded {
        ClientMessage::AgentSemanticQuery { args_json } => {
            let payload: serde_json::Value = serde_json::from_str(&args_json).unwrap();
            assert_eq!(payload["kind"], "packages");
            assert_eq!(payload["root"], "/tmp/workspace");
            assert_eq!(payload["limit"], 10);
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}

#[test]
fn daemon_message_roundtrips_agent_semantic_query_result() {
    let msg = DaemonMessage::AgentSemanticQueryResult {
        content: "## Packages\n- cargo: zorai-daemon".to_string(),
    };
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
    match decoded {
        DaemonMessage::AgentSemanticQueryResult { content } => {
            assert!(content.contains("Packages"));
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}

#[test]
fn client_message_roundtrips_agent_fetch_models_with_output_filter() {
    let msg = ClientMessage::AgentFetchModels {
        provider_id: "openrouter".to_string(),
        base_url: "https://openrouter.ai/api/v1".to_string(),
        api_key: "router-key".to_string(),
        output_modalities: Some("image".to_string()),
    };
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: ClientMessage = bincode::deserialize(&bytes).unwrap();
    match decoded {
        ClientMessage::AgentFetchModels {
            provider_id,
            base_url,
            api_key,
            output_modalities,
        } => {
            assert_eq!(provider_id, "openrouter");
            assert_eq!(base_url, "https://openrouter.ai/api/v1");
            assert_eq!(api_key, "router-key");
            assert_eq!(output_modalities.as_deref(), Some("image"));
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}

#[test]
fn client_message_roundtrips_agent_set_target_agent_reasoning_effort() {
    let msg = ClientMessage::AgentSetTargetAgentReasoningEffort {
        target_agent_id: "weles".to_string(),
        reasoning_effort: "high".to_string(),
    };
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: ClientMessage = bincode::deserialize(&bytes).unwrap();
    match decoded {
        ClientMessage::AgentSetTargetAgentReasoningEffort {
            target_agent_id,
            reasoning_effort,
        } => {
            assert_eq!(target_agent_id, "weles");
            assert_eq!(reasoning_effort, "high");
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}

#[test]
fn client_message_roundtrips_agent_set_target_agent_context_window() {
    let msg = ClientMessage::AgentSetTargetAgentContextWindow {
        target_agent_id: "weles".to_string(),
        context_window_tokens: 200_000,
    };
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: ClientMessage = bincode::deserialize(&bytes).unwrap();
    match decoded {
        ClientMessage::AgentSetTargetAgentContextWindow {
            target_agent_id,
            context_window_tokens,
        } => {
            assert_eq!(target_agent_id, "weles");
            assert_eq!(context_window_tokens, 200_000);
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}

#[test]
fn client_message_roundtrips_agent_execute_memory_tool() {
    let msg = ClientMessage::AgentExecuteMemoryTool {
        tool_name: "search_memory".to_string(),
        args_json: serde_json::json!({
            "query": "operator preferences",
            "limit": 5,
        })
        .to_string(),
    };
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: ClientMessage = bincode::deserialize(&bytes).unwrap();
    match decoded {
        ClientMessage::AgentExecuteMemoryTool {
            tool_name,
            args_json,
        } => {
            let payload: serde_json::Value = serde_json::from_str(&args_json).unwrap();
            assert_eq!(tool_name, "search_memory");
            assert_eq!(payload["query"], "operator preferences");
            assert_eq!(payload["limit"], 5);
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}

#[test]
fn client_message_roundtrips_agent_speech_to_text() {
    let msg = ClientMessage::AgentSpeechToText {
        args_json: serde_json::json!({
            "path": "/tmp/audio.wav",
            "mime_type": "audio/wav",
        })
        .to_string(),
    };
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: ClientMessage = bincode::deserialize(&bytes).unwrap();
    match decoded {
        ClientMessage::AgentSpeechToText { args_json } => {
            let payload: serde_json::Value = serde_json::from_str(&args_json).unwrap();
            assert_eq!(payload["path"], "/tmp/audio.wav");
            assert_eq!(payload["mime_type"], "audio/wav");
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}

#[test]
fn client_message_roundtrips_agent_text_to_speech() {
    let msg = ClientMessage::AgentTextToSpeech {
        args_json: serde_json::json!({
            "input": "hello",
            "voice": "alloy",
        })
        .to_string(),
    };
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: ClientMessage = bincode::deserialize(&bytes).unwrap();
    match decoded {
        ClientMessage::AgentTextToSpeech { args_json } => {
            let payload: serde_json::Value = serde_json::from_str(&args_json).unwrap();
            assert_eq!(payload["input"], "hello");
            assert_eq!(payload["voice"], "alloy");
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}

#[test]
fn client_message_roundtrips_agent_generate_image() {
    let msg = ClientMessage::AgentGenerateImage {
        args_json: serde_json::json!({
            "thread_id": "thread-image",
            "prompt": "cinematic neon city"
        })
        .to_string(),
    };
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: ClientMessage = bincode::deserialize(&bytes).unwrap();
    match decoded {
        ClientMessage::AgentGenerateImage { args_json } => {
            let payload: serde_json::Value = serde_json::from_str(&args_json).unwrap();
            assert_eq!(payload["thread_id"], "thread-image");
            assert_eq!(payload["prompt"], "cinematic neon city");
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}

#[test]
fn daemon_message_roundtrips_agent_memory_tool_result() {
    let msg = DaemonMessage::AgentMemoryToolResult {
        content: serde_json::json!({
            "scope": "memory",
            "matches": [],
            "truncated": false,
        })
        .to_string(),
    };
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
    match decoded {
        DaemonMessage::AgentMemoryToolResult { content } => {
            let payload: serde_json::Value = serde_json::from_str(&content).unwrap();
            assert_eq!(payload["scope"], "memory");
            assert_eq!(payload["truncated"], false);
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}

#[test]
fn client_message_roundtrips_external_runtime_migration_commands() {
    let messages = [
        ClientMessage::AgentExternalRuntimeMigrationStatus,
        ClientMessage::AgentExternalRuntimeMigrationPreview {
            runtime: "hermes".to_string(),
            config_path: Some("/tmp/hermes.yaml".to_string()),
        },
        ClientMessage::AgentExternalRuntimeMigrationApply {
            runtime: "openclaw".to_string(),
            config_path: None,
            conflict_policy: "stage_for_review".to_string(),
        },
        ClientMessage::AgentExternalRuntimeMigrationReport {
            runtime: None,
            limit: Some(5),
        },
        ClientMessage::AgentExternalRuntimeMigrationShadowRun {
            runtime: "hermes".to_string(),
        },
    ];

    for msg in messages {
        let bytes = bincode::serialize(&msg).unwrap();
        let decoded: ClientMessage = bincode::deserialize(&bytes).unwrap();
        match decoded {
            ClientMessage::AgentExternalRuntimeMigrationStatus
            | ClientMessage::AgentExternalRuntimeMigrationPreview { .. }
            | ClientMessage::AgentExternalRuntimeMigrationApply { .. }
            | ClientMessage::AgentExternalRuntimeMigrationReport { .. }
            | ClientMessage::AgentExternalRuntimeMigrationShadowRun { .. } => {}
            other => panic!("unexpected variant: {:?}", other),
        }
    }
}

#[test]
fn daemon_message_roundtrips_external_runtime_migration_result() {
    let msg = DaemonMessage::AgentExternalRuntimeMigrationResult {
        result_json: serde_json::json!({
            "runtime": "hermes",
            "dry_run": true,
            "persisted": false
        })
        .to_string(),
    };
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
    match decoded {
        DaemonMessage::AgentExternalRuntimeMigrationResult { result_json } => {
            let payload: serde_json::Value = serde_json::from_str(&result_json).unwrap();
            assert_eq!(payload["runtime"], "hermes");
            assert_eq!(payload["dry_run"], true);
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}

#[test]
fn daemon_message_roundtrips_agent_speech_to_text_result() {
    let msg = DaemonMessage::AgentSpeechToTextResult {
        content: serde_json::json!({
            "text": "hello world"
        })
        .to_string(),
    };
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
    match decoded {
        DaemonMessage::AgentSpeechToTextResult { content } => {
            let payload: serde_json::Value = serde_json::from_str(&content).unwrap();
            assert_eq!(payload["text"], "hello world");
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}

#[test]
fn daemon_message_roundtrips_agent_text_to_speech_result() {
    let msg = DaemonMessage::AgentTextToSpeechResult {
        content: serde_json::json!({
            "path": "/tmp/speech.mp3",
            "mime_type": "audio/mpeg"
        })
        .to_string(),
    };
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
    match decoded {
        DaemonMessage::AgentTextToSpeechResult { content } => {
            let payload: serde_json::Value = serde_json::from_str(&content).unwrap();
            assert_eq!(payload["path"], "/tmp/speech.mp3");
            assert_eq!(payload["mime_type"], "audio/mpeg");
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}

#[test]
fn daemon_message_roundtrips_agent_generate_image_result() {
    let msg = DaemonMessage::AgentGenerateImageResult {
        content: serde_json::json!({
            "thread_id": "thread-image",
            "path": "/tmp/thread-image/result.png",
            "mime_type": "image/png"
        })
        .to_string(),
    };
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
    match decoded {
        DaemonMessage::AgentGenerateImageResult { content } => {
            let payload: serde_json::Value = serde_json::from_str(&content).unwrap();
            assert_eq!(payload["thread_id"], "thread-image");
            assert_eq!(payload["path"], "/tmp/thread-image/result.png");
            assert_eq!(payload["mime_type"], "image/png");
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}

#[test]
fn client_message_roundtrips_agent_vote_on_collaboration_disagreement() {
    let msg = ClientMessage::AgentVoteOnCollaborationDisagreement {
        parent_task_id: "task-1".to_string(),
        disagreement_id: "disagree-1".to_string(),
        task_id: "operator".to_string(),
        position: "recommend".to_string(),
        confidence: Some(1.0),
    };
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: ClientMessage = bincode::deserialize(&bytes).unwrap();
    match decoded {
        ClientMessage::AgentVoteOnCollaborationDisagreement {
            parent_task_id,
            disagreement_id,
            task_id,
            position,
            confidence,
        } => {
            assert_eq!(parent_task_id, "task-1");
            assert_eq!(disagreement_id, "disagree-1");
            assert_eq!(task_id, "operator");
            assert_eq!(position, "recommend");
            assert_eq!(confidence, Some(1.0));
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}

#[test]
fn daemon_message_roundtrips_agent_collaboration_vote_result() {
    let msg = DaemonMessage::AgentCollaborationVoteResult {
        report_json: serde_json::json!({
            "session_id": "session-1",
            "resolution": "resolved"
        })
        .to_string(),
    };
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
    match decoded {
        DaemonMessage::AgentCollaborationVoteResult { report_json } => {
            let payload: serde_json::Value = serde_json::from_str(&report_json).unwrap();
            assert_eq!(payload["session_id"], "session-1");
            assert_eq!(payload["resolution"], "resolved");
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}

#[test]
fn client_message_roundtrips_agent_confirm_memory_provenance_entry() {
    let msg = ClientMessage::AgentConfirmMemoryProvenanceEntry {
        entry_id: "old-confirmable".to_string(),
    };
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: ClientMessage = bincode::deserialize(&bytes).unwrap();
    match decoded {
        ClientMessage::AgentConfirmMemoryProvenanceEntry { entry_id } => {
            assert_eq!(entry_id, "old-confirmable");
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}

#[test]
fn daemon_message_roundtrips_agent_memory_provenance_confirmed() {
    let msg = DaemonMessage::AgentMemoryProvenanceConfirmed {
        entry_id: "old-confirmable".to_string(),
        confirmed_at: 123,
    };
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
    match decoded {
        DaemonMessage::AgentMemoryProvenanceConfirmed {
            entry_id,
            confirmed_at,
        } => {
            assert_eq!(entry_id, "old-confirmable");
            assert_eq!(confirmed_at, 123);
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}

#[test]
fn async_command_capability_roundtrips() {
    let payload = AsyncCommandCapability {
        version: 1,
        supports_operation_acceptance: true,
    };
    let bytes = bincode::serialize(&payload).unwrap();
    let decoded: AsyncCommandCapability = bincode::deserialize(&bytes).unwrap();
    assert_eq!(decoded.version, 1);
    assert!(decoded.supports_operation_acceptance);
}

#[test]
fn client_message_roundtrips_async_command_capability() {
    let msg = ClientMessage::AgentDeclareAsyncCommandCapability {
        capability: AsyncCommandCapability {
            version: 1,
            supports_operation_acceptance: true,
        },
    };
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: ClientMessage = bincode::deserialize(&bytes).unwrap();
    assert!(matches!(
        decoded,
        ClientMessage::AgentDeclareAsyncCommandCapability { .. }
    ));
}

#[test]
fn daemon_message_roundtrips_async_command_capability_ack() {
    let msg = DaemonMessage::AgentAsyncCommandCapabilityAck {
        capability: AsyncCommandCapability {
            version: 1,
            supports_operation_acceptance: true,
        },
    };
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
    assert!(matches!(
        decoded,
        DaemonMessage::AgentAsyncCommandCapabilityAck { .. }
    ));
}

#[test]
fn operation_status_snapshot_roundtrips() {
    let snapshot = OperationStatusSnapshot {
        operation_id: "op-1".to_string(),
        kind: "concierge_welcome".to_string(),
        state: OperationLifecycleState::Accepted,
        dedup: Some("concierge:default".to_string()),
        revision: 0,
    };
    let bytes = bincode::serialize(&snapshot).unwrap();
    let decoded: OperationStatusSnapshot = bincode::deserialize(&bytes).unwrap();
    assert_eq!(decoded.operation_id, "op-1");
    assert!(matches!(decoded.state, OperationLifecycleState::Accepted));
}

#[test]
fn daemon_message_roundtrips_operation_accepted() {
    let msg = DaemonMessage::OperationAccepted {
        operation_id: "op-1".to_string(),
        kind: "concierge_welcome".to_string(),
        dedup: Some("concierge:default".to_string()),
        revision: 0,
    };
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
    assert!(matches!(decoded, DaemonMessage::OperationAccepted { .. }));
}

#[test]
fn client_message_roundtrips_operation_status_query() {
    let msg = ClientMessage::AgentGetOperationStatus {
        operation_id: "op-1".to_string(),
    };
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: ClientMessage = bincode::deserialize(&bytes).unwrap();
    assert!(matches!(
        decoded,
        ClientMessage::AgentGetOperationStatus { .. }
    ));
}

#[test]
fn client_message_roundtrips_explain_action() {
    let msg = ClientMessage::AgentExplainAction {
        action_id: "missing-action".to_string(),
        step_index: None,
    };
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: ClientMessage = bincode::deserialize(&bytes).unwrap();
    assert!(matches!(decoded, ClientMessage::AgentExplainAction { .. }));
}

#[test]
fn client_message_roundtrips_start_divergent_session() {
    let msg = ClientMessage::AgentStartDivergentSession {
        problem_statement: "compare rollout strategies".to_string(),
        thread_id: "thread-div-1".to_string(),
        goal_run_id: None,
        custom_framings_json: None,
    };
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: ClientMessage = bincode::deserialize(&bytes).unwrap();
    assert!(matches!(
        decoded,
        ClientMessage::AgentStartDivergentSession { .. }
    ));
}

#[test]
fn daemon_message_roundtrips_operation_status_snapshot() {
    let msg = DaemonMessage::OperationStatus {
        snapshot: OperationStatusSnapshot {
            operation_id: "op-1".to_string(),
            kind: "concierge_welcome".to_string(),
            state: OperationLifecycleState::Started,
            dedup: Some("concierge:default".to_string()),
            revision: 1,
        },
    };
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
    assert!(matches!(decoded, DaemonMessage::OperationStatus { .. }));
}

#[test]
fn daemon_message_roundtrips_task_approval_rules_with_missing_last_used_at() {
    let msg = DaemonMessage::AgentTaskApprovalRules {
        rules: vec![TaskApprovalRule {
            id: "rule-1".to_string(),
            command: "review low-confidence goal plan".to_string(),
            created_at: 1,
            last_used_at: None,
            use_count: 0,
        }],
    };

    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
    match decoded {
        DaemonMessage::AgentTaskApprovalRules { rules } => {
            assert_eq!(rules.len(), 1);
            assert_eq!(rules[0].id, "rule-1");
            assert_eq!(rules[0].last_used_at, None);
            assert_eq!(rules[0].use_count, 0);
        }
        other => panic!("expected task approval rules, got {other:?}"),
    }
}

#[test]
fn daemon_message_roundtrips_plugin_list_with_missing_optional_metadata() {
    let msg = DaemonMessage::PluginListResult {
        plugins: vec![PluginInfo {
            name: "calendar".to_string(),
            version: "1.1.0".to_string(),
            description: None,
            author: None,
            enabled: true,
            install_source: "bundled".to_string(),
            has_api: true,
            has_auth: true,
            has_commands: true,
            has_skills: true,
            endpoint_count: 5,
            settings_count: 3,
            installed_at: "2026-04-20T00:00:00Z".to_string(),
            updated_at: "2026-04-20T00:00:00Z".to_string(),
            auth_status: "disconnected".to_string(),
            connector_kind: Some("calendar".to_string()),
            connector_category: Some("calendar".to_string()),
            readiness_state: "ready".to_string(),
            readiness_message: Some("Connector auth is healthy.".to_string()),
            recovery_hint: None,
            setup_hint: Some(
                "Configure Google OAuth client credentials and connect the plugin.".to_string(),
            ),
            docs_path: Some("plugins/zorai-plugin-gmail-calendar/README.md".to_string()),
            workflow_primitives: vec!["list_schedule_items".to_string()],
            read_actions: vec!["list_events".to_string()],
            write_actions: vec!["create_event".to_string()],
        }],
    };

    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
    match decoded {
        DaemonMessage::PluginListResult { plugins } => {
            assert_eq!(plugins.len(), 1);
            assert_eq!(plugins[0].name, "calendar");
            assert_eq!(plugins[0].description, None);
            assert_eq!(plugins[0].author, None);
            assert_eq!(plugins[0].auth_status, "disconnected");
            assert_eq!(plugins[0].connector_kind.as_deref(), Some("calendar"));
            assert_eq!(plugins[0].readiness_state, "ready");
        }
        other => panic!("expected plugin list result, got {other:?}"),
    }
}

#[test]
fn daemon_message_keeps_agent_thread_deleted_after_older_thread_responses() {
    let direct_message_index = bincode_variant_index(&DaemonMessage::AgentDirectMessageResponse {
        target: "operator".to_string(),
        thread_id: "thread-1".to_string(),
        response: "ok".to_string(),
        session_id: None,
        provider_final_result_json: None,
    });
    let deleted_index = bincode_variant_index(&DaemonMessage::AgentThreadDeleted {
        thread_id: "thread-1".to_string(),
        deleted: true,
    });

    assert!(
        deleted_index > direct_message_index,
        "new daemon variants must be appended to preserve older wire indices"
    );
}

#[test]
fn client_message_roundtrips_effective_config_state_query() {
    let msg = ClientMessage::AgentGetEffectiveConfigState;
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: ClientMessage = bincode::deserialize(&bytes).unwrap();
    assert!(matches!(
        decoded,
        ClientMessage::AgentGetEffectiveConfigState
    ));
}

#[test]
fn daemon_message_roundtrips_effective_config_state() {
    let msg = DaemonMessage::AgentEffectiveConfigState {
        state_json: r#"{"reconcile":{"state":"reconciling","desired_revision":2,"effective_revision":1,"last_error":null},"gateway_runtime_connected":false}"#.to_string(),
    };
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
    assert!(matches!(
        decoded,
        DaemonMessage::AgentEffectiveConfigState { .. }
    ));
}

#[test]
fn client_message_roundtrips_subsystem_metrics_query() {
    let msg = ClientMessage::AgentGetSubsystemMetrics;
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: ClientMessage = bincode::deserialize(&bytes).unwrap();
    assert!(matches!(decoded, ClientMessage::AgentGetSubsystemMetrics));
}

#[test]
fn client_message_roundtrips_get_openai_codex_auth_status() {
    let msg = ClientMessage::AgentGetOpenAICodexAuthStatus;
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: ClientMessage = bincode::deserialize(&bytes).unwrap();
    assert!(matches!(
        decoded,
        ClientMessage::AgentGetOpenAICodexAuthStatus
    ));
}

#[test]
fn client_message_roundtrips_login_openai_codex() {
    let msg = ClientMessage::AgentLoginOpenAICodex;
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: ClientMessage = bincode::deserialize(&bytes).unwrap();
    assert!(matches!(decoded, ClientMessage::AgentLoginOpenAICodex));
}

#[test]
fn client_message_roundtrips_logout_openai_codex() {
    let msg = ClientMessage::AgentLogoutOpenAICodex;
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: ClientMessage = bincode::deserialize(&bytes).unwrap();
    assert!(matches!(decoded, ClientMessage::AgentLogoutOpenAICodex));
}

#[test]
fn daemon_message_roundtrips_subsystem_metrics_response() {
    let msg = DaemonMessage::AgentSubsystemMetrics {
        metrics_json: r#"{"plugin_io":{"current_depth":1,"max_depth":2,"rejection_count":1,"accepted_count":3,"started_count":3,"completed_count":1,"failed_count":2,"accepted_to_started_samples":3,"started_to_terminal_samples":3,"last_accepted_to_started_ms":1,"last_started_to_terminal_ms":2}}"#.to_string(),
    };
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
    assert!(matches!(
        decoded,
        DaemonMessage::AgentSubsystemMetrics { .. }
    ));
}

#[test]
fn daemon_message_roundtrips_openai_codex_auth_status() {
    let msg = DaemonMessage::AgentOpenAICodexAuthStatus {
        status_json: r#"{"provider":"openai_codex","state":"authenticated","last_checked_at":"2026-04-01T12:00:00Z"}"#.to_string(),
    };
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
    match decoded {
        DaemonMessage::AgentOpenAICodexAuthStatus { status_json } => {
            assert_eq!(
                status_json,
                r#"{"provider":"openai_codex","state":"authenticated","last_checked_at":"2026-04-01T12:00:00Z"}"#
            );
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}

#[test]
fn daemon_message_roundtrips_openai_codex_auth_login_result() {
    let msg = DaemonMessage::AgentOpenAICodexAuthLoginResult {
        result_json: r#"{"provider":"openai_codex","login_url":"https://auth.openai.example/device","expires_in_seconds":900}"#.to_string(),
    };
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
    match decoded {
        DaemonMessage::AgentOpenAICodexAuthLoginResult { result_json } => {
            assert_eq!(
                result_json,
                r#"{"provider":"openai_codex","login_url":"https://auth.openai.example/device","expires_in_seconds":900}"#
            );
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}

#[test]
fn daemon_message_roundtrips_openai_codex_auth_logout_result() {
    let msg = DaemonMessage::AgentOpenAICodexAuthLogoutResult {
        ok: false,
        error: Some("no cached codex credentials found".to_string()),
    };
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
    match decoded {
        DaemonMessage::AgentOpenAICodexAuthLogoutResult { ok, error } => {
            assert!(!ok);
            assert_eq!(error.as_deref(), Some("no cached codex credentials found"));
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}

#[test]
fn daemon_message_roundtrips_plugin_api_call_result_with_operation_id() {
    let msg = DaemonMessage::PluginApiCallResult {
        operation_id: Some("op-plugin-1".to_string()),
        plugin_name: "api-test".to_string(),
        endpoint_name: "slow".to_string(),
        success: false,
        result: "timed out".to_string(),
        error_type: Some("timeout".to_string()),
    };
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
    match decoded {
        DaemonMessage::PluginApiCallResult { operation_id, .. } => {
            assert_eq!(operation_id.as_deref(), Some("op-plugin-1"));
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}

#[test]
fn daemon_message_roundtrips_skill_import_result_with_operation_id() {
    let msg = DaemonMessage::SkillImportResult {
        operation_id: Some("op-skill-import-1".to_string()),
        success: true,
        message: "Imported community skill 'test-skill' as draft.".to_string(),
        variant_id: Some("variant-1".to_string()),
        scan_verdict: Some("warn".to_string()),
        findings_count: 0,
    };
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
    match decoded {
        DaemonMessage::SkillImportResult { operation_id, .. } => {
            assert_eq!(operation_id.as_deref(), Some("op-skill-import-1"));
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}

#[test]
fn daemon_message_roundtrips_skill_publish_result_with_operation_id() {
    let msg = DaemonMessage::SkillPublishResult {
        operation_id: Some("op-skill-publish-1".to_string()),
        success: true,
        message: "Published skill 'publish-test'.".to_string(),
    };
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
    match decoded {
        DaemonMessage::SkillPublishResult { operation_id, .. } => {
            assert_eq!(operation_id.as_deref(), Some("op-skill-publish-1"));
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}

#[test]
fn daemon_message_roundtrips_agent_explanation_with_operation_id() {
    let msg = DaemonMessage::AgentExplanation {
        operation_id: Some("op-explain-1".to_string()),
        explanation_json: r#"{"action_id":"missing-action","source":"fallback"}"#.to_string(),
    };
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
    match decoded {
        DaemonMessage::AgentExplanation {
            operation_id,
            explanation_json,
        } => {
            assert_eq!(operation_id.as_deref(), Some("op-explain-1"));
            assert!(explanation_json.contains("missing-action"));
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}

#[test]
fn daemon_message_roundtrips_plugin_oauth_complete_with_operation_id() {
    let msg = DaemonMessage::PluginOAuthComplete {
        operation_id: Some("op-oauth-1".to_string()),
        name: "oauth-test".to_string(),
        success: true,
        error: None,
    };
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
    match decoded {
        DaemonMessage::PluginOAuthComplete {
            operation_id,
            name,
            success,
            error,
        } => {
            assert_eq!(operation_id.as_deref(), Some("op-oauth-1"));
            assert_eq!(name, "oauth-test");
            assert!(success);
            assert!(error.is_none());
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}

#[test]
fn agent_get_thread_round_trip_preserves_message_page_arguments() {
    let msg = ClientMessage::AgentGetThread {
        thread_id: "thread-1".to_string(),
        message_limit: Some(50),
        message_offset: Some(100),
        collapse_tool_calls: true,
    };

    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: ClientMessage = bincode::deserialize(&bytes).unwrap();
    match decoded {
        ClientMessage::AgentGetThread {
            thread_id,
            message_limit,
            message_offset,
            collapse_tool_calls,
        } => {
            assert_eq!(thread_id, "thread-1");
            assert_eq!(message_limit, Some(50));
            assert_eq!(message_offset, Some(100));
            assert!(collapse_tool_calls);
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}

#[test]
fn agent_get_thread_defaults_collapse_tool_calls_to_false() {
    let value = serde_json::json!({
        "AgentGetThread": {
            "thread_id": "thread-1",
            "message_limit": 50,
            "message_offset": 100,
        }
    });
    let decoded: ClientMessage = serde_json::from_value(value).unwrap();
    match decoded {
        ClientMessage::AgentGetThread {
            thread_id,
            message_limit,
            message_offset,
            collapse_tool_calls,
        } => {
            assert_eq!(thread_id, "thread-1");
            assert_eq!(message_limit, Some(50));
            assert_eq!(message_offset, Some(100));
            assert!(!collapse_tool_calls);
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}

#[test]
fn agent_list_threads_round_trip_preserves_pagination_arguments() {
    let msg = ClientMessage::AgentListThreads {
        limit: Some(20),
        offset: Some(40),
        include_internal: true,
        agent_filter: None,
    };

    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: ClientMessage = bincode::deserialize(&bytes).unwrap();
    match decoded {
        ClientMessage::AgentListThreads {
            limit,
            offset,
            include_internal,
            agent_filter: _,
        } => {
            assert_eq!(limit, Some(20));
            assert_eq!(offset, Some(40));
            assert!(include_internal);
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}

#[test]
fn agent_list_goal_runs_round_trip_preserves_pagination_arguments() {
    let msg = ClientMessage::AgentListGoalRuns {
        limit: Some(10),
        offset: Some(20),
    };

    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: ClientMessage = bincode::deserialize(&bytes).unwrap();
    match decoded {
        ClientMessage::AgentListGoalRuns { limit, offset } => {
            assert_eq!(limit, Some(10));
            assert_eq!(offset, Some(20));
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}

#[test]
fn agent_delete_goal_run_round_trip_preserves_goal_run_id() {
    let msg = ClientMessage::AgentDeleteGoalRun {
        goal_run_id: "goal-1".to_string(),
    };

    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: ClientMessage = bincode::deserialize(&bytes).unwrap();
    match decoded {
        ClientMessage::AgentDeleteGoalRun { goal_run_id } => {
            assert_eq!(goal_run_id, "goal-1");
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}

#[test]
fn daemon_message_roundtrips_agent_goal_run_deleted() {
    let msg = DaemonMessage::AgentGoalRunDeleted {
        goal_run_id: "goal-1".to_string(),
        deleted: true,
    };

    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
    match decoded {
        DaemonMessage::AgentGoalRunDeleted {
            goal_run_id,
            deleted,
        } => {
            assert_eq!(goal_run_id, "goal-1");
            assert!(deleted);
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}

#[test]
fn daemon_message_roundtrips_agent_thread_controlled() {
    let msg = DaemonMessage::AgentThreadControlled {
        thread_id: "thread-1".to_string(),
        action: "resume".to_string(),
        ok: true,
    };

    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
    match decoded {
        DaemonMessage::AgentThreadControlled {
            thread_id,
            action,
            ok,
        } => {
            assert_eq!(thread_id, "thread-1");
            assert_eq!(action, "resume");
            assert!(ok);
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}

fn sample_skill_variant() -> SkillVariantPublic {
    SkillVariantPublic {
        variant_id: "sv-001".to_string(),
        skill_name: "git_rebase_workflow".to_string(),
        variant_name: "v1".to_string(),
        relative_path: "drafts/git_rebase_workflow/SKILL.md".to_string(),
        status: "active".to_string(),
        use_count: 12,
        success_count: 10,
        failure_count: 2,
        context_tags: vec!["git".to_string(), "rebase".to_string()],
        created_at: 1700000000,
        updated_at: 1700001000,
    }
}

fn sample_community_skill_entry() -> CommunitySkillEntry {
    CommunitySkillEntry {
        name: "git-rebase-workflow".to_string(),
        description: "Safely rebase a feature branch".to_string(),
        version: "1.2.0".to_string(),
        publisher_id: "abcd1234efgh5678".to_string(),
        publisher_verified: true,
        success_rate: 0.93,
        use_count: 42,
        content_hash: "d34db33f".to_string(),
        zorai_version: "0.1.10".to_string(),
        maturity_at_publish: "proven".to_string(),
        tags: vec!["git".to_string(), "workflow".to_string()],
        published_at: 1700001234,
    }
}

fn sample_skill_discovery_candidate() -> SkillDiscoveryCandidatePublic {
    SkillDiscoveryCandidatePublic {
        variant_id: "local:git_rebase_workflow:v1".to_string(),
        skill_name: "git_rebase_workflow".to_string(),
        variant_name: "v1".to_string(),
        relative_path: "drafts/git_rebase_workflow/SKILL.md".to_string(),
        status: "active".to_string(),
        score: 0.94,
        confidence_tier: "strong".to_string(),
        reasons: vec!["matches git rebase workflow".to_string()],
        matched_intents: vec!["git rebase workflow".to_string()],
        matched_trigger_phrases: vec!["rebase".to_string()],
        context_tags: vec!["git".to_string(), "rebase".to_string()],
        risk_level: "low".to_string(),
        trust_tier: "trusted_builtin".to_string(),
        source_kind: "builtin".to_string(),
        recommended_action: "read_skill git_rebase_workflow".to_string(),
        use_count: 12,
        success_count: 10,
        failure_count: 2,
        canonical_pack: false,
        delivery_modes: Vec::new(),
        prerequisite_hints: Vec::new(),
        prerequisite_connectors: Vec::new(),
        source_links: Vec::new(),
        mobile_safe: false,
        approval_behavior: None,
    }
}

fn sample_skill_discovery_result() -> SkillDiscoveryResultPublic {
    SkillDiscoveryResultPublic {
        query: "git rebase workflow".to_string(),
        normalized_intent: "git rebase workflow".to_string(),
        required: true,
        confidence_tier: "strong".to_string(),
        recommended_action: "read_skill git_rebase_workflow".to_string(),
        requires_approval: false,
        mesh_state: "fresh".to_string(),
        rationale: vec!["matched git rebase workflow".to_string()],
        capability_family: vec!["development".to_string(), "git".to_string()],
        explicit_rationale_required: false,
        workspace_tags: vec!["git".to_string(), "rebase".to_string()],
        candidates: vec![sample_skill_discovery_candidate()],
        next_cursor: Some("cursor:git-rebase".to_string()),
    }
}

#[test]
fn minimal_skill_discovery_result_deserializes_with_defaults() {
    let result_json = serde_json::json!({
        "query": "debug panic",
        "normalized_intent": "debug panic",
        "confidence_tier": "strong",
        "recommended_action": "read_skill",
        "requires_approval": false,
        "mesh_state": "fresh",
        "rationale": ["matched debug intent"],
        "capability_family": ["development", "debugging"],
        "candidates": [{
            "skill_name": "systematic-debugging",
            "score": 93.0,
            "reasons": ["matched debug", "workspace rust", "active variant"],
            "matched_intents": ["debug panic"],
            "matched_trigger_phrases": ["panic"],
            "risk_level": "low",
            "trust_tier": "trusted_builtin",
            "source_kind": "builtin",
            "recommended_action": "read_skill systematic-debugging"
        }]
    })
    .to_string();

    let result: SkillDiscoveryResultPublic = serde_json::from_str(&result_json).unwrap();
    assert_eq!(result.query, "debug panic");
    assert_eq!(result.normalized_intent, "debug panic");
    assert!(!result.required);
    assert_eq!(result.confidence_tier, "strong");
    assert_eq!(result.recommended_action, "read_skill");
    assert!(!result.requires_approval);
    assert_eq!(result.mesh_state, "fresh");
    assert_eq!(result.rationale, vec!["matched debug intent".to_string()]);
    assert_eq!(
        result.capability_family,
        vec!["development".to_string(), "debugging".to_string()]
    );
    assert!(!result.explicit_rationale_required);
    assert!(result.workspace_tags.is_empty());
    assert_eq!(result.candidates.len(), 1);
    let candidate = &result.candidates[0];
    assert_eq!(candidate.variant_id, "");
    assert_eq!(candidate.skill_name, "systematic-debugging");
    assert_eq!(candidate.variant_name, "");
    assert_eq!(candidate.relative_path, "");
    assert_eq!(candidate.status, "");
    assert!((candidate.score - 93.0).abs() < f64::EPSILON);
    assert_eq!(candidate.confidence_tier, "");
    assert_eq!(
        candidate.reasons,
        vec![
            "matched debug".to_string(),
            "workspace rust".to_string(),
            "active variant".to_string()
        ]
    );
    assert!(candidate.context_tags.is_empty());
    assert_eq!(candidate.matched_intents, vec!["debug panic".to_string()]);
    assert_eq!(candidate.matched_trigger_phrases, vec!["panic".to_string()]);
    assert_eq!(candidate.risk_level, "low");
    assert_eq!(candidate.trust_tier, "trusted_builtin");
    assert_eq!(candidate.source_kind, "builtin");
    assert_eq!(
        candidate.recommended_action,
        "read_skill systematic-debugging"
    );
    assert_eq!(candidate.use_count, 0);
    assert_eq!(candidate.success_count, 0);
    assert_eq!(candidate.failure_count, 0);
    assert!(!candidate.canonical_pack);
    assert!(candidate.delivery_modes.is_empty());
    assert!(candidate.prerequisite_hints.is_empty());
    assert!(candidate.prerequisite_connectors.is_empty());
    assert!(candidate.source_links.is_empty());
    assert!(!candidate.mobile_safe);
    assert_eq!(candidate.approval_behavior, None);
}

#[test]
fn gateway_register_round_trip() {
    let msg = ClientMessage::GatewayRegister {
        registration: GatewayRegistration {
            gateway_id: "gateway-main".to_string(),
            instance_id: "instance-01".to_string(),
            protocol_version: 1,
            supported_platforms: vec![
                "slack".to_string(),
                "discord".to_string(),
                "telegram".to_string(),
            ],
            process_id: Some(4242),
        },
    };
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: ClientMessage = bincode::deserialize(&bytes).unwrap();
    match decoded {
        ClientMessage::GatewayRegister { registration } => {
            assert_eq!(registration.gateway_id, "gateway-main");
            assert_eq!(registration.instance_id, "instance-01");
            assert_eq!(registration.protocol_version, 1);
            assert_eq!(registration.supported_platforms.len(), 3);
            assert_eq!(registration.process_id, Some(4242));
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}

#[test]
fn gateway_bootstrap_round_trip() {
    let msg = DaemonMessage::GatewayBootstrap {
        payload: GatewayBootstrapPayload {
            bootstrap_correlation_id: "boot-1".to_string(),
            feature_flags: vec![
                "gateway_runtime_ownership".to_string(),
                "gateway_route_persistence".to_string(),
            ],
            providers: vec![GatewayProviderBootstrap {
                platform: "slack".to_string(),
                enabled: true,
                credentials_json: r#"{"token":"secret"}"#.to_string(),
                config_json: r#"{"channel_filter":"C123"}"#.to_string(),
            }],
            continuity: GatewayContinuityState {
                cursors: vec![GatewayCursorState {
                    platform: "slack".to_string(),
                    channel_id: "C123".to_string(),
                    cursor_value: "1712345678.000100".to_string(),
                    cursor_type: "message_ts".to_string(),
                    updated_at_ms: 1_710_000_000_000,
                }],
                thread_bindings: vec![GatewayThreadBindingState {
                    channel_key: "Slack:C123".to_string(),
                    thread_id: Some("thread-123".to_string()),
                    updated_at_ms: 1_710_000_000_001,
                }],
                route_modes: vec![GatewayRouteModeState {
                    channel_key: "Slack:C123".to_string(),
                    route_mode: GatewayRouteMode::Rarog,
                    updated_at_ms: 1_710_000_000_002,
                }],
                health_snapshots: vec![GatewayHealthState {
                    platform: "slack".to_string(),
                    status: GatewayConnectionStatus::Error,
                    last_success_at_ms: Some(1_710_000_000_000),
                    last_error_at_ms: Some(1_710_000_000_100),
                    consecutive_failure_count: 2,
                    last_error: Some("timeout".to_string()),
                    current_backoff_secs: 30,
                }],
            },
        },
    };
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
    match decoded {
        DaemonMessage::GatewayBootstrap { payload } => {
            assert_eq!(payload.bootstrap_correlation_id, "boot-1");
            assert_eq!(payload.feature_flags.len(), 2);
            assert_eq!(payload.providers.len(), 1);
            assert_eq!(payload.providers[0].platform, "slack");
            assert_eq!(payload.continuity.cursors.len(), 1);
            assert_eq!(payload.continuity.thread_bindings.len(), 1);
            assert_eq!(payload.continuity.route_modes.len(), 1);
            assert_eq!(payload.continuity.health_snapshots.len(), 1);
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}

#[test]
fn skill_variant_result_round_trip() {
    let msg = DaemonMessage::SkillListResult {
        variants: vec![sample_skill_variant()],
        next_cursor: Some("cursor:variant-2".to_string()),
    };
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
    match decoded {
        DaemonMessage::SkillListResult {
            variants,
            next_cursor,
        } => {
            assert_eq!(variants.len(), 1);
            assert_eq!(variants[0].skill_name, "git_rebase_workflow");
            assert_eq!(variants[0].status, "active");
            assert_eq!(next_cursor.as_deref(), Some("cursor:variant-2"));
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}

#[test]
fn skill_search_result_round_trip() {
    let msg = DaemonMessage::SkillSearchResult {
        entries: vec![sample_community_skill_entry()],
    };
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
    match decoded {
        DaemonMessage::SkillSearchResult { entries } => {
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].name, "git-rebase-workflow");
            assert!(entries[0].publisher_verified);
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}

#[test]
fn client_message_agent_logout_openai_codex_preserves_pre_change_wire_discriminant() {
    let msg = ClientMessage::AgentLogoutOpenAICodex;
    assert_bincode_variant_index(&msg, 169);
}

#[test]
fn daemon_message_agent_openai_codex_auth_logout_result_preserves_pre_change_wire_discriminant() {
    let msg = DaemonMessage::AgentOpenAICodexAuthLogoutResult {
        ok: true,
        error: None,
    };
    assert_bincode_variant_index(&msg, 137);
}

#[test]
fn skill_discover_round_trip() {
    let expected_session_id = SessionId::new_v4();
    let msg = ClientMessage::SkillDiscover {
        query: "git rebase workflow".to_string(),
        session_id: Some(expected_session_id),
        limit: 5,
        cursor: Some("cursor:git-rebase".to_string()),
    };
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: ClientMessage = bincode::deserialize(&bytes).unwrap();
    match decoded {
        ClientMessage::SkillDiscover {
            query,
            session_id,
            limit,
            cursor,
        } => {
            assert_eq!(query, "git rebase workflow");
            assert_eq!(session_id, Some(expected_session_id));
            assert_eq!(limit, 5);
            assert_eq!(cursor.as_deref(), Some("cursor:git-rebase"));
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}

#[test]
fn guideline_discover_round_trip() {
    let expected_session_id = SessionId::new_v4();
    let msg = ClientMessage::GuidelineDiscover {
        query: "coding task workflow".to_string(),
        session_id: Some(expected_session_id),
        limit: 3,
        cursor: Some("cursor:coding-task".to_string()),
    };
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: ClientMessage = bincode::deserialize(&bytes).unwrap();
    match decoded {
        ClientMessage::GuidelineDiscover {
            query,
            session_id,
            limit,
            cursor,
        } => {
            assert_eq!(query, "coding task workflow");
            assert_eq!(session_id, Some(expected_session_id));
            assert_eq!(limit, 3);
            assert_eq!(cursor.as_deref(), Some("cursor:coding-task"));
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}

#[test]
fn semantic_document_sync_round_trip() {
    let msg = ClientMessage::AgentSemanticDocumentSync;
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: ClientMessage = bincode::deserialize(&bytes).unwrap();
    assert!(matches!(decoded, ClientMessage::AgentSemanticDocumentSync));

    let payload = SemanticDocumentIndexSyncResultPublic {
        embedding_model: "text-embedding-3-small".to_string(),
        dimensions: 1536,
        skills: SemanticDocumentSyncSummaryPublic {
            discovered: 3,
            changed: 2,
            queued_embeddings: 2,
            removed: 0,
        },
        guidelines: SemanticDocumentSyncSummaryPublic {
            discovered: 1,
            changed: 1,
            queued_embeddings: 1,
            removed: 0,
        },
    };
    let msg = DaemonMessage::SemanticDocumentSyncResult {
        result_json: serde_json::to_string(&payload).unwrap(),
    };
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
    match decoded {
        DaemonMessage::SemanticDocumentSyncResult { result_json } => {
            let result: SemanticDocumentIndexSyncResultPublic =
                serde_json::from_str(&result_json).unwrap();
            assert_eq!(result.embedding_model, "text-embedding-3-small");
            assert_eq!(result.dimensions, 1536);
            assert_eq!(result.skills.discovered, 3);
            assert_eq!(result.guidelines.queued_embeddings, 1);
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}

#[test]
fn semantic_index_repair_round_trip() {
    let msg = ClientMessage::AgentRepairSemanticIndex { confirmed: true };
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: ClientMessage = bincode::deserialize(&bytes).unwrap();
    match decoded {
        ClientMessage::AgentRepairSemanticIndex { confirmed } => assert!(confirmed),
        other => panic!("unexpected variant: {:?}", other),
    }

    let payload = SemanticIndexRepairResultPublic {
        backup_path: Some("/tmp/lancedb.backup".to_string()),
        removed_vector_index: true,
        cleared_completions: 12,
        cleared_deletions: 3,
        reset_failed_jobs: 2,
    };
    let msg = DaemonMessage::SemanticIndexRepairResult {
        result_json: serde_json::to_string(&payload).unwrap(),
    };
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
    match decoded {
        DaemonMessage::SemanticIndexRepairResult { result_json } => {
            let result: SemanticIndexRepairResultPublic =
                serde_json::from_str(&result_json).unwrap();
            assert_eq!(result.backup_path.as_deref(), Some("/tmp/lancedb.backup"));
            assert!(result.removed_vector_index);
            assert_eq!(result.cleared_completions, 12);
            assert_eq!(result.cleared_deletions, 3);
            assert_eq!(result.reset_failed_jobs, 2);
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}

#[test]
fn skill_discover_result_round_trip() {
    let payload = sample_skill_discovery_result();
    let msg = DaemonMessage::SkillDiscoverResult {
        result_json: serde_json::to_string(&payload).unwrap(),
    };
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
    match decoded {
        DaemonMessage::SkillDiscoverResult { result_json } => {
            let result: SkillDiscoveryResultPublic = serde_json::from_str(&result_json).unwrap();
            assert_eq!(result.query, "git rebase workflow");
            assert_eq!(result.normalized_intent, "git rebase workflow");
            assert!(result.required);
            assert_eq!(result.confidence_tier, "strong");
            assert_eq!(result.recommended_action, "read_skill git_rebase_workflow");
            assert!(!result.requires_approval);
            assert_eq!(result.mesh_state, "fresh");
            assert_eq!(
                result.rationale,
                vec!["matched git rebase workflow".to_string()]
            );
            assert_eq!(
                result.capability_family,
                vec!["development".to_string(), "git".to_string()]
            );
            assert!(!result.explicit_rationale_required);
            assert_eq!(result.workspace_tags, vec!["git", "rebase"]);
            assert_eq!(result.candidates.len(), 1);
            assert_eq!(result.next_cursor.as_deref(), Some("cursor:git-rebase"));
            assert_eq!(
                result.candidates[0].variant_id,
                "local:git_rebase_workflow:v1"
            );
            assert_eq!(result.candidates[0].skill_name, "git_rebase_workflow");
            assert_eq!(result.candidates[0].variant_name, "v1");
            assert_eq!(
                result.candidates[0].relative_path,
                "drafts/git_rebase_workflow/SKILL.md"
            );
            assert_eq!(result.candidates[0].status, "active");
            assert!((result.candidates[0].score - 0.94).abs() < f64::EPSILON);
            assert_eq!(result.candidates[0].confidence_tier, "strong");
            assert_eq!(
                result.candidates[0].reasons,
                vec!["matches git rebase workflow".to_string()]
            );
            assert_eq!(
                result.candidates[0].matched_intents,
                vec!["git rebase workflow".to_string()]
            );
            assert_eq!(
                result.candidates[0].matched_trigger_phrases,
                vec!["rebase".to_string()]
            );
            assert_eq!(result.candidates[0].context_tags, vec!["git", "rebase"]);
            assert_eq!(result.candidates[0].risk_level, "low");
            assert_eq!(result.candidates[0].trust_tier, "trusted_builtin");
            assert_eq!(result.candidates[0].source_kind, "builtin");
            assert_eq!(
                result.candidates[0].recommended_action,
                "read_skill git_rebase_workflow"
            );
            assert_eq!(result.candidates[0].use_count, 12);
            assert_eq!(result.candidates[0].success_count, 10);
            assert_eq!(result.candidates[0].failure_count, 2);
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}

#[test]
fn guideline_discover_result_round_trip() {
    let mut payload = sample_skill_discovery_result();
    payload.recommended_action = "read_guideline coding-task".to_string();
    payload.candidates[0].skill_name = "coding-task".to_string();
    payload.candidates[0].relative_path = "coding-task.md".to_string();
    payload.candidates[0].source_kind = "guideline".to_string();
    payload.candidates[0].recommended_action = "read_guideline coding-task".to_string();

    let msg = DaemonMessage::GuidelineDiscoverResult {
        result_json: serde_json::to_string(&payload).unwrap(),
    };
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
    match decoded {
        DaemonMessage::GuidelineDiscoverResult { result_json } => {
            let result: SkillDiscoveryResultPublic = serde_json::from_str(&result_json).unwrap();
            assert_eq!(result.recommended_action, "read_guideline coding-task");
            assert_eq!(result.candidates[0].skill_name, "coding-task");
            assert_eq!(result.candidates[0].source_kind, "guideline");
            assert_eq!(
                result.candidates[0].recommended_action,
                "read_guideline coding-task"
            );
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}

#[test]
fn skill_list_round_trip_preserves_cursor() {
    let msg = ClientMessage::SkillList {
        status: Some("active".to_string()),
        limit: 25,
        cursor: Some("cursor:active-page-2".to_string()),
    };
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: ClientMessage = bincode::deserialize(&bytes).unwrap();
    match decoded {
        ClientMessage::SkillList {
            status,
            limit,
            cursor,
        } => {
            assert_eq!(status.as_deref(), Some("active"));
            assert_eq!(limit, 25);
            assert_eq!(cursor.as_deref(), Some("cursor:active-page-2"));
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}

#[test]
fn approval_payload_deserializes_legacy_shape_with_defaults() {
    let legacy_json = serde_json::json!({
        "approval_id": "apr_legacy",
        "execution_id": "exec_legacy",
        "command": "echo hello",
        "rationale": "legacy payload",
        "risk_level": "medium",
        "blast_radius": "current session",
        "reasons": ["network access requested"],
        "workspace_id": "workspace-a",
        "allow_network": true
    })
    .to_string();

    let payload: ApprovalPayload = serde_json::from_str(&legacy_json).unwrap();
    assert_eq!(payload.approval_id, "apr_legacy");
    assert_eq!(payload.execution_id, "exec_legacy");
    assert_eq!(payload.transition_kind, None);
    assert_eq!(payload.policy_fingerprint, None);
    assert_eq!(payload.expires_at, None);
    assert!(payload.constraints.is_empty());
    assert_eq!(payload.scope_summary, None);
}

#[test]
fn approval_payload_omits_new_governance_fields_when_absent() {
    let payload = ApprovalPayload {
        approval_id: "apr_minimal".to_string(),
        execution_id: "exec_minimal".to_string(),
        command: "echo hello".to_string(),
        rationale: "minimal payload".to_string(),
        risk_level: "low".to_string(),
        blast_radius: "current session".to_string(),
        reasons: vec!["safe command".to_string()],
        workspace_id: Some("workspace-a".to_string()),
        allow_network: false,
        transition_kind: None,
        policy_fingerprint: None,
        expires_at: None,
        constraints: Vec::new(),
        scope_summary: None,
    };

    let json = serde_json::to_value(&payload).unwrap();
    let object = json.as_object().unwrap();
    assert!(!object.contains_key("transition_kind"));
    assert!(!object.contains_key("policy_fingerprint"));
    assert!(!object.contains_key("expires_at"));
    assert!(!object.contains_key("constraints"));
    assert!(!object.contains_key("scope_summary"));
}

#[test]
fn approval_payload_round_trips_governance_metadata() {
    let payload = ApprovalPayload {
        approval_id: "apr_governed".to_string(),
        execution_id: "exec_governed".to_string(),
        command: "sudo terraform destroy".to_string(),
        rationale: "apply risky infra change".to_string(),
        risk_level: "high".to_string(),
        blast_radius: "network and workspace".to_string(),
        reasons: vec!["risk score exceeded approval threshold".to_string()],
        workspace_id: Some("workspace-a".to_string()),
        allow_network: true,
        transition_kind: Some("managed_command_dispatch".to_string()),
        policy_fingerprint: Some("fingerprint-123".to_string()),
        expires_at: Some(1_717_171_717),
        constraints: vec!["serial_only_execution".to_string()],
        scope_summary: Some("managed transition".to_string()),
    };

    let encoded = serde_json::to_string(&payload).unwrap();
    let decoded: ApprovalPayload = serde_json::from_str(&encoded).unwrap();
    assert_eq!(
        decoded.transition_kind.as_deref(),
        Some("managed_command_dispatch")
    );
    assert_eq!(
        decoded.policy_fingerprint.as_deref(),
        Some("fingerprint-123")
    );
    assert_eq!(decoded.expires_at, Some(1_717_171_717));
    assert_eq!(
        decoded.constraints,
        vec!["serial_only_execution".to_string()]
    );
    assert_eq!(decoded.scope_summary.as_deref(), Some("managed transition"));
}

#[test]
fn mlflow_tracing_protocol_variants_round_trip_at_append_only_tail() {
    let requests = vec![
        ClientMessage::AgentGetMlflowTracingStatus,
        ClientMessage::AgentTestMlflowTracingConnection,
        ClientMessage::AgentSendMlflowTracingTestTrace,
        ClientMessage::AgentListMlflowTracingHeaders,
        ClientMessage::AgentSetMlflowTracingHeader {
            name: "X-Test".to_string(),
            value: "secret".to_string(),
        },
        ClientMessage::AgentDeleteMlflowTracingHeader {
            name: "X-Test".to_string(),
        },
    ];
    for request in requests {
        let bytes = bincode::serialize(&request).unwrap();
        let decoded: ClientMessage = bincode::deserialize(&bytes).unwrap();
        assert_eq!(
            bincode_variant_index(&request),
            bincode_variant_index(&decoded)
        );
    }

    let responses = vec![
        DaemonMessage::AgentMlflowTracingStatus {
            status_json: "{}".to_string(),
        },
        DaemonMessage::AgentMlflowTracingTestResult {
            result_json: "{\"ok\":true}".to_string(),
        },
        DaemonMessage::AgentMlflowTracingHeaders {
            names: vec!["X-Test".to_string()],
        },
        DaemonMessage::AgentPromptQueue {
            thread_id: Some("thread-1".to_string()),
            prompts: vec![QueuedPromptRecord {
                id: "prompt-1".to_string(),
                thread_id: "thread-1".to_string(),
                content: "follow up".to_string(),
                content_blocks_json: None,
                created_at: 1,
                position: 1,
            }],
        },
    ];
    for response in responses {
        let bytes = bincode::serialize(&response).unwrap();
        let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
        assert_eq!(
            bincode_variant_index(&response),
            bincode_variant_index(&decoded)
        );
    }

    let queue_requests = vec![
        ClientMessage::AgentEnqueuePrompt {
            thread_id: "thread-1".to_string(),
            content: "follow up".to_string(),
            content_blocks_json: None,
            prompt_id: Some("prompt-1".to_string()),
        },
        ClientMessage::AgentListPromptQueue {
            thread_id: Some("thread-1".to_string()),
        },
        ClientMessage::AgentUpdateQueuedPrompt {
            thread_id: "thread-1".to_string(),
            prompt_id: "prompt-1".to_string(),
            content: "edited".to_string(),
            content_blocks_json: None,
        },
        ClientMessage::AgentCancelQueuedPrompt {
            thread_id: "thread-1".to_string(),
            prompt_id: "prompt-1".to_string(),
        },
        ClientMessage::AgentSendQueuedPromptNow {
            thread_id: "thread-1".to_string(),
            prompt_id: "prompt-1".to_string(),
        },
    ];
    for request in queue_requests {
        let bytes = bincode::serialize(&request).unwrap();
        let decoded: ClientMessage = bincode::deserialize(&bytes).unwrap();
        assert_eq!(
            bincode_variant_index(&request),
            bincode_variant_index(&decoded)
        );
    }
}

#[test]
fn client_message_agent_generate_video_is_appended_after_existing_wire_discriminants() {
    let previous_last = ClientMessage::AgentSetPromptQueueHold {
        thread_id: "thread-video".to_string(),
        held: false,
    };
    let msg = ClientMessage::AgentGenerateVideo {
        args_json: serde_json::json!({
            "thread_id": "thread-video",
            "prompt": "drone shot over a foggy forest"
        })
        .to_string(),
    };
    assert_bincode_variant_index(&msg, bincode_variant_index(&previous_last) + 1);
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: ClientMessage = bincode::deserialize(&bytes).unwrap();
    match decoded {
        ClientMessage::AgentGenerateVideo { args_json } => {
            let payload: serde_json::Value = serde_json::from_str(&args_json).unwrap();
            assert_eq!(payload["thread_id"], "thread-video");
            assert_eq!(payload["prompt"], "drone shot over a foggy forest");
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}

#[test]
fn daemon_message_agent_generate_video_result_is_appended_after_existing_wire_discriminants() {
    let previous_last = DaemonMessage::AgentThreadSearchResults {
        results_json: "[]".to_string(),
    };
    let msg = DaemonMessage::AgentGenerateVideoResult {
        content: serde_json::json!({
            "thread_id": "thread-video",
            "path": "/tmp/thread-video/video.mp4",
            "mime_type": "video/mp4"
        })
        .to_string(),
    };
    assert_bincode_variant_index(&msg, bincode_variant_index(&previous_last) + 1);
    let bytes = bincode::serialize(&msg).unwrap();
    let decoded: DaemonMessage = bincode::deserialize(&bytes).unwrap();
    match decoded {
        DaemonMessage::AgentGenerateVideoResult { content } => {
            let payload: serde_json::Value = serde_json::from_str(&content).unwrap();
            assert_eq!(payload["path"], "/tmp/thread-video/video.mp4");
            assert_eq!(payload["mime_type"], "video/mp4");
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}
