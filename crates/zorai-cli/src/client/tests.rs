use super::agent_api::{
    parse_config_set_response, parse_external_runtime_migration_terminal_response,
    parse_operation_status_response, send_thread_get_query,
};
use super::agent_bridge::handle_message_for_test as handle_bridge_message;
use super::agent_bridge::initial_bridge_messages;
use super::agent_protocol::AgentBridgeCommand;
use super::connection::closed_connection_error;
use super::skill_api::{
    parse_guideline_discover_terminal_response, parse_skill_discover_terminal_response,
    parse_skill_import_terminal_response, parse_skill_publish_terminal_response,
};
use futures::{SinkExt, StreamExt};
use tokio_util::codec::Framed;
use zorai_protocol::{ClientMessage, DaemonCodec, DaemonMessage};

#[test]
fn closed_connection_error_mentions_version_mismatch_and_restart() {
    let message = closed_connection_error().to_string();

    assert!(
        message.contains("daemon closed connection"),
        "base failure should remain visible: {message}"
    );
    assert!(
        message.contains("version mismatch"),
        "message should point to likely protocol skew: {message}"
    );
    assert!(
        message.contains("restart"),
        "message should tell the operator what to do next: {message}"
    );
}

#[test]
fn skill_discover_terminal_response_ignores_unsolicited_session_frames() {
    let session_id =
        uuid::Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").expect("valid test uuid");

    assert!(
        parse_skill_discover_terminal_response(DaemonMessage::CwdChanged {
            id: session_id,
            cwd: "/workspace/repo".to_string(),
        })
        .is_none()
    );
    assert!(
        parse_skill_discover_terminal_response(DaemonMessage::Output {
            id: session_id,
            data: b"log".to_vec(),
        })
        .is_none()
    );
    assert!(
        parse_skill_discover_terminal_response(DaemonMessage::CommandStarted {
            id: session_id,
            command: "cargo test".to_string(),
        })
        .is_none()
    );
    assert!(
        parse_skill_discover_terminal_response(DaemonMessage::CommandFinished {
            id: session_id,
            exit_code: Some(0),
        })
        .is_none()
    );
}

#[test]
fn skill_discover_terminal_response_parses_result_payload() {
    let parsed = parse_skill_discover_terminal_response(DaemonMessage::SkillDiscoverResult {
        result_json: serde_json::json!({
            "query": "debug panic",
            "required": true,
            "confidence_tier": "strong",
            "recommended_action": "read_skill systematic-debugging",
            "explicit_rationale_required": false,
            "workspace_tags": ["rust"],
            "candidates": []
        })
        .to_string(),
    })
    .expect("result frame should terminate")
    .expect("payload should parse");

    assert_eq!(parsed.query, "debug panic");
    assert_eq!(parsed.confidence_tier, "strong");
}

#[test]
fn guideline_discover_terminal_response_parses_result_payload() {
    let parsed =
        parse_guideline_discover_terminal_response(DaemonMessage::GuidelineDiscoverResult {
            result_json: serde_json::json!({
                "query": "research papers",
                "required": true,
                "confidence_tier": "strong",
                "recommended_action": "read_guideline academic-literature-review-task",
                "explicit_rationale_required": false,
                "workspace_tags": ["research"],
                "candidates": []
            })
            .to_string(),
        })
        .expect("result frame should terminate")
        .expect("payload should parse");

    assert_eq!(parsed.query, "research papers");
    assert_eq!(
        parsed.recommended_action,
        "read_guideline academic-literature-review-task"
    );
}

#[test]
fn operator_profile_bridge_commands_deserialize() {
    let json = r#"{"type":"start-operator-profile-session","kind":"onboarding"}"#;
    let cmd: AgentBridgeCommand =
        serde_json::from_str(json).expect("start-operator-profile-session must deserialize");
    match cmd {
        AgentBridgeCommand::StartOperatorProfileSession { kind } => {
            assert_eq!(kind, "onboarding");
        }
        _ => panic!("unexpected variant for start-operator-profile-session"),
    }

    let json = r#"{"type":"next-operator-profile-question","session_id":"s1"}"#;
    let cmd: AgentBridgeCommand =
        serde_json::from_str(json).expect("next-operator-profile-question must deserialize");
    match cmd {
        AgentBridgeCommand::NextOperatorProfileQuestion { session_id } => {
            assert_eq!(session_id, "s1");
        }
        _ => panic!("unexpected variant for next-operator-profile-question"),
    }

    let json = r#"{"type":"submit-operator-profile-answer","session_id":"s1","question_id":"q1","answer_json":"\"yes\""}"#;
    let cmd: AgentBridgeCommand =
        serde_json::from_str(json).expect("submit-operator-profile-answer must deserialize");
    match cmd {
        AgentBridgeCommand::SubmitOperatorProfileAnswer {
            session_id,
            question_id,
            answer_json,
        } => {
            assert_eq!(session_id, "s1");
            assert_eq!(question_id, "q1");
            assert_eq!(answer_json, "\"yes\"");
        }
        _ => panic!("unexpected variant for submit-operator-profile-answer"),
    }

    let json = r#"{"type":"skip-operator-profile-question","session_id":"s1","question_id":"q1"}"#;
    let cmd: AgentBridgeCommand =
        serde_json::from_str(json).expect("skip-operator-profile-question must deserialize");
    match cmd {
        AgentBridgeCommand::SkipOperatorProfileQuestion {
            session_id,
            question_id,
            reason,
        } => {
            assert_eq!(session_id, "s1");
            assert_eq!(question_id, "q1");
            assert!(reason.is_none());
        }
        _ => panic!("unexpected variant for skip-operator-profile-question"),
    }

    let json = r#"{"type":"defer-operator-profile-question","session_id":"s1","question_id":"q1","defer_until_unix_ms":1700000000000}"#;
    let cmd: AgentBridgeCommand =
        serde_json::from_str(json).expect("defer-operator-profile-question must deserialize");
    match cmd {
        AgentBridgeCommand::DeferOperatorProfileQuestion {
            session_id,
            question_id,
            defer_until_unix_ms,
        } => {
            assert_eq!(session_id, "s1");
            assert_eq!(question_id, "q1");
            assert_eq!(defer_until_unix_ms, Some(1700000000000));
        }
        _ => panic!("unexpected variant for defer-operator-profile-question"),
    }

    let json = r#"{"type":"get-operator-profile-summary"}"#;
    let cmd: AgentBridgeCommand =
        serde_json::from_str(json).expect("get-operator-profile-summary must deserialize");
    assert!(
        matches!(cmd, AgentBridgeCommand::GetOperatorProfileSummary),
        "expected GetOperatorProfileSummary"
    );

    let json =
        r#"{"type":"set-operator-profile-consent","consent_key":"analytics","granted":true}"#;
    let cmd: AgentBridgeCommand =
        serde_json::from_str(json).expect("set-operator-profile-consent must deserialize");
    match cmd {
        AgentBridgeCommand::SetOperatorProfileConsent {
            consent_key,
            granted,
        } => {
            assert_eq!(consent_key, "analytics");
            assert!(granted);
        }
        _ => panic!("unexpected variant for set-operator-profile-consent"),
    }

    let json = r#"{"type":"query-audits","action_types":["tool","heartbeat"],"limit":25}"#;
    let cmd: AgentBridgeCommand =
        serde_json::from_str(json).expect("query-audits must deserialize");
    match cmd {
        AgentBridgeCommand::QueryAudits {
            action_types,
            since,
            limit,
        } => {
            assert_eq!(
                action_types,
                Some(vec!["tool".to_string(), "heartbeat".to_string()])
            );
            assert_eq!(since, None);
            assert_eq!(limit, Some(25));
        }
        _ => panic!("unexpected variant for query-audits"),
    }

    let json = r#"{"type":"get-provenance-report","limit":10}"#;
    let cmd: AgentBridgeCommand =
        serde_json::from_str(json).expect("get-provenance-report must deserialize");
    match cmd {
        AgentBridgeCommand::GetProvenanceReport { limit } => {
            assert_eq!(limit, Some(10));
        }
        _ => panic!("unexpected variant for get-provenance-report"),
    }

    let json = r#"{"type":"query-audits","action_types":["tool","heartbeat"],"limit":25}"#;
    let cmd: AgentBridgeCommand =
        serde_json::from_str(json).expect("query-audits must deserialize");
    match cmd {
        AgentBridgeCommand::QueryAudits {
            action_types,
            since,
            limit,
        } => {
            assert_eq!(
                action_types,
                Some(vec!["tool".to_string(), "heartbeat".to_string()])
            );
            assert_eq!(since, None);
            assert_eq!(limit, Some(25));
        }
        _ => panic!("unexpected variant for query-audits"),
    }

    let json = r#"{"type":"get-provenance-report","limit":10}"#;
    let cmd: AgentBridgeCommand =
        serde_json::from_str(json).expect("get-provenance-report must deserialize");
    match cmd {
        AgentBridgeCommand::GetProvenanceReport { limit } => {
            assert_eq!(limit, Some(10));
        }
        _ => panic!("unexpected variant for get-provenance-report"),
    }

    let json = r#"{"type":"get-memory-provenance-report","target":"MEMORY.md","limit":12}"#;
    let cmd: AgentBridgeCommand =
        serde_json::from_str(json).expect("get-memory-provenance-report must deserialize");
    match cmd {
        AgentBridgeCommand::GetMemoryProvenanceReport { target, limit } => {
            assert_eq!(target.as_deref(), Some("MEMORY.md"));
            assert_eq!(limit, Some(12));
        }
        _ => panic!("unexpected variant for get-memory-provenance-report"),
    }

    let json = r#"{"type":"confirm-memory-provenance-entry","entry_id":"old-confirmable"}"#;
    let cmd: AgentBridgeCommand =
        serde_json::from_str(json).expect("confirm-memory-provenance-entry must deserialize");
    match cmd {
        AgentBridgeCommand::ConfirmMemoryProvenanceEntry { entry_id } => {
            assert_eq!(entry_id, "old-confirmable");
        }
        _ => panic!("unexpected variant for confirm-memory-provenance-entry"),
    }

    let json =
        r#"{"type":"retract-memory-provenance-entry","entry_id":"retractable-memory-entry"}"#;
    let cmd: AgentBridgeCommand =
        serde_json::from_str(json).expect("retract-memory-provenance-entry must deserialize");
    match cmd {
        AgentBridgeCommand::RetractMemoryProvenanceEntry { entry_id } => {
            assert_eq!(entry_id, "retractable-memory-entry");
        }
        _ => panic!("unexpected variant for retract-memory-provenance-entry"),
    }

    let json = r#"{"type":"get-collaboration-sessions","parent_task_id":"task-1"}"#;
    let cmd: AgentBridgeCommand =
        serde_json::from_str(json).expect("get-collaboration-sessions must deserialize");
    match cmd {
        AgentBridgeCommand::GetCollaborationSessions { parent_task_id } => {
            assert_eq!(parent_task_id.as_deref(), Some("task-1"));
        }
        _ => panic!("unexpected variant for get-collaboration-sessions"),
    }

    let json = r#"{"type":"vote-on-collaboration-disagreement","parent_task_id":"task-1","disagreement_id":"disagree-1","task_id":"operator","position":"recommend","confidence":1.0}"#;
    let cmd: AgentBridgeCommand =
        serde_json::from_str(json).expect("vote-on-collaboration-disagreement must deserialize");
    match cmd {
        AgentBridgeCommand::VoteOnCollaborationDisagreement {
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
        _ => panic!("unexpected variant for vote-on-collaboration-disagreement"),
    }
}

#[test]
fn operator_profile_bridge_commands_deserialize_failures() {
    let result: Result<AgentBridgeCommand, _> =
        serde_json::from_str(r#"{"type":"not-a-real-operator-command"}"#);
    assert!(result.is_err(), "unknown type should not deserialize");

    let result: Result<AgentBridgeCommand, _> =
        serde_json::from_str(r#"{"type":"start-operator-profile-session"}"#);
    assert!(result.is_err(), "missing `kind` should not deserialize");

    let result: Result<AgentBridgeCommand, _> =
        serde_json::from_str(r#"{"type":"next-operator-profile-question"}"#);
    assert!(
        result.is_err(),
        "missing `session_id` should not deserialize"
    );

    let result: Result<AgentBridgeCommand, _> = serde_json::from_str(
        r#"{"type":"set-operator-profile-consent","consent_key":"analytics","granted":"yes"}"#,
    );
    assert!(
        result.is_err(),
        "wrong field type for `granted` should not deserialize"
    );

    let result: Result<AgentBridgeCommand, _> = serde_json::from_str(
        r#"{"type":"submit-operator-profile-answer","session_id":"s1","answer_json":"\"v\""}"#,
    );
    assert!(
        result.is_err(),
        "missing `question_id` should not deserialize"
    );

    let result: Result<AgentBridgeCommand, _> = serde_json::from_str(
        r#"{"type":"defer-operator-profile-question","session_id":"s1","question_id":"q1","defer_until_unix_ms":"soon"}"#,
    );
    assert!(
        result.is_err(),
        "wrong type for `defer_until_unix_ms` should not deserialize"
    );
}

#[test]
fn handoff_bridge_command_deserializes() {
    let cmd: AgentBridgeCommand = serde_json::from_str(
        r#"{"type":"handoff-thread","thread_id":"thread-1","action":"push_handoff","target_agent_id":"rarog","reason":"operator requested","summary":"handoff now","requested_by":"user","session_id":"session-1"}"#,
    )
    .expect("handoff-thread must deserialize");
    match cmd {
        AgentBridgeCommand::HandoffThread {
            thread_id,
            action,
            target_agent_id,
            reason,
            summary,
            requested_by,
            session_id,
        } => {
            assert_eq!(thread_id, "thread-1");
            assert_eq!(action, "push_handoff");
            assert_eq!(target_agent_id.as_deref(), Some("rarog"));
            assert_eq!(reason, "operator requested");
            assert_eq!(summary, "handoff now");
            assert_eq!(requested_by, "user");
            assert_eq!(session_id.as_deref(), Some("session-1"));
        }
        _ => panic!("unexpected variant for handoff-thread"),
    }
}

#[test]
fn operation_status_bridge_command_deserializes() {
    let cmd: AgentBridgeCommand =
        serde_json::from_str(r#"{"type":"get-operation-status","operation_id":"op-1"}"#)
            .expect("get-operation-status must deserialize");
    match cmd {
        AgentBridgeCommand::GetOperationStatus { operation_id } => {
            assert_eq!(operation_id, "op-1");
        }
        _ => panic!("unexpected variant for get-operation-status"),
    }
}

#[test]
fn participant_suggestion_bridge_commands_deserialize_force_send() {
    let cmd: AgentBridgeCommand = serde_json::from_str(
        r#"{"type":"send-participant-suggestion","thread_id":"thread-1","suggestion_id":"sugg-1","session_id":"session-1","force_send":true}"#,
    )
    .expect("send-participant-suggestion must deserialize");
    match cmd {
        AgentBridgeCommand::SendParticipantSuggestion {
            thread_id,
            suggestion_id,
            session_id,
            force_send,
        } => {
            assert_eq!(thread_id, "thread-1");
            assert_eq!(suggestion_id, "sugg-1");
            assert_eq!(session_id.as_deref(), Some("session-1"));
            assert!(force_send);
        }
        _ => panic!("unexpected variant for send-participant-suggestion"),
    }

    let cmd: AgentBridgeCommand = serde_json::from_str(
        r#"{"type":"send-participant-suggestion","thread_id":"thread-1","suggestion_id":"sugg-1"}"#,
    )
    .expect("send-participant-suggestion must deserialize with default force_send");
    match cmd {
        AgentBridgeCommand::SendParticipantSuggestion { force_send, .. } => {
            assert!(!force_send);
        }
        _ => panic!("unexpected variant for send-participant-suggestion default"),
    }
}

#[test]
fn generated_tool_bridge_commands_deserialize() {
    let json = r#"{"type":"list-generated-tools"}"#;
    let cmd: AgentBridgeCommand =
        serde_json::from_str(json).expect("list-generated-tools must deserialize");
    assert!(
        matches!(cmd, AgentBridgeCommand::ListGeneratedTools),
        "expected ListGeneratedTools"
    );

    let json = r#"{"type":"run-generated-tool","tool_name":"tool-1","args_json":"{}"}"#;
    let cmd: AgentBridgeCommand =
        serde_json::from_str(json).expect("run-generated-tool must deserialize");
    match cmd {
        AgentBridgeCommand::RunGeneratedTool {
            tool_name,
            args_json,
        } => {
            assert_eq!(tool_name, "tool-1");
            assert_eq!(args_json, "{}");
        }
        _ => panic!("unexpected variant for run-generated-tool"),
    }

    let json = r#"{"type":"activate-generated-tool","tool_name":"tool-1"}"#;
    let cmd: AgentBridgeCommand =
        serde_json::from_str(json).expect("activate-generated-tool must deserialize");
    match cmd {
        AgentBridgeCommand::ActivateGeneratedTool { tool_name } => {
            assert_eq!(tool_name, "tool-1");
        }
        _ => panic!("unexpected variant for activate-generated-tool"),
    }

    let json = r#"{"type":"promote-generated-tool","tool_name":"tool-1"}"#;
    let cmd: AgentBridgeCommand =
        serde_json::from_str(json).expect("promote-generated-tool must deserialize");
    match cmd {
        AgentBridgeCommand::PromoteGeneratedTool { tool_name } => {
            assert_eq!(tool_name, "tool-1");
        }
        _ => panic!("unexpected variant for promote-generated-tool"),
    }

    let json = r#"{"type":"retire-generated-tool","tool_name":"tool-1"}"#;
    let cmd: AgentBridgeCommand =
        serde_json::from_str(json).expect("retire-generated-tool must deserialize");
    match cmd {
        AgentBridgeCommand::RetireGeneratedTool { tool_name } => {
            assert_eq!(tool_name, "tool-1");
        }
        _ => panic!("unexpected variant for retire-generated-tool"),
    }
}

#[test]
fn speech_bridge_commands_deserialize() {
    let json = r#"{"type":"speech-to-text","args_json":"{}"}"#;
    let cmd: AgentBridgeCommand =
        serde_json::from_str(json).expect("speech-to-text must deserialize");
    match cmd {
        AgentBridgeCommand::SpeechToText { args_json } => {
            assert_eq!(args_json, "{}");
        }
        _ => panic!("unexpected variant for speech-to-text"),
    }

    let json = r#"{"type":"text-to-speech","args_json":"{}"}"#;
    let cmd: AgentBridgeCommand =
        serde_json::from_str(json).expect("text-to-speech must deserialize");
    match cmd {
        AgentBridgeCommand::TextToSpeech { args_json } => {
            assert_eq!(args_json, "{}");
        }
        _ => panic!("unexpected variant for text-to-speech"),
    }
}

#[test]
fn media_generation_bridge_commands_deserialize_from_the_gui_command_names() {
    let cmd: AgentBridgeCommand =
        serde_json::from_str(r#"{"type":"generate-image","args_json":"{\"prompt\":\"cat\"}"}"#)
            .expect("generate-image must deserialize");
    assert!(
        matches!(cmd, AgentBridgeCommand::GenerateImage { ref args_json } if args_json == r#"{"prompt":"cat"}"#)
    );

    let cmd: AgentBridgeCommand =
        serde_json::from_str(r#"{"type":"generate-video","args_json":"{\"prompt\":\"cat\"}"}"#)
            .expect("generate-video must deserialize");
    assert!(
        matches!(cmd, AgentBridgeCommand::GenerateVideo { ref args_json } if args_json == r#"{"prompt":"cat"}"#)
    );

    let cmd: AgentBridgeCommand =
        serde_json::from_str(r#"{"type":"generate-music","args_json":"{\"prompt\":\"lofi\"}"}"#)
            .expect("generate-music must deserialize");
    assert!(
        matches!(cmd, AgentBridgeCommand::GenerateMusic { ref args_json } if args_json == r#"{"prompt":"lofi"}"#)
    );
}

#[test]
fn browser_result_bridge_command_deserializes_from_the_gui_command_name() {
    let cmd: AgentBridgeCommand = serde_json::from_str(
        r#"{"type":"browser-result","request_id":"br_1","ok":true,"result_json":"{\"url\":\"https://example.com\"}"}"#,
    )
    .expect("browser-result must deserialize");
    assert!(matches!(
        cmd,
        AgentBridgeCommand::BrowserResult { ref request_id, ok: true, ref result_json }
            if request_id == "br_1" && result_json == r#"{"url":"https://example.com"}"#
    ));
}

#[test]
fn agent_protocol_codex_auth_bridge_commands_deserialize() {
    fn parse(json: &str) -> AgentBridgeCommand {
        serde_json::from_str(json).expect("codex auth command must deserialize")
    }

    assert!(matches!(
        parse(r#"{"type":"openai-codex-auth-status"}"#),
        AgentBridgeCommand::GetOpenAICodexAuthStatus
    ));
    assert!(matches!(
        parse(r#"{"type":"openai-codex-auth-login"}"#),
        AgentBridgeCommand::LoginOpenAICodex
    ));
    assert!(matches!(
        parse(r#"{"type":"openai-codex-auth-logout"}"#),
        AgentBridgeCommand::LogoutOpenAICodex
    ));
}

#[test]
fn agent_bridge_declares_async_command_capability_on_connect() {
    let messages = initial_bridge_messages();
    assert_eq!(messages.len(), 2);
    assert!(matches!(messages[0], ClientMessage::AgentSubscribe));
    match &messages[1] {
        ClientMessage::AgentDeclareAsyncCommandCapability { capability } => {
            assert_eq!(capability.version, 1);
            assert!(capability.supports_operation_acceptance);
        }
        other => panic!("expected async command capability declaration, got {other:?}"),
    }
}

#[test]
fn skill_import_terminal_response_ignores_operation_acceptance() {
    let response = parse_skill_import_terminal_response(DaemonMessage::OperationAccepted {
        operation_id: "op-skill-import-1".to_string(),
        kind: "skill_import".to_string(),
        dedup: None,
        revision: 1,
    });
    assert!(response.is_none(), "operation acceptance is not terminal");
}

#[test]
fn skill_import_terminal_response_extracts_result_payload() {
    let response = parse_skill_import_terminal_response(DaemonMessage::SkillImportResult {
        operation_id: Some("op-skill-import-1".to_string()),
        success: true,
        message: "Imported skill".to_string(),
        variant_id: Some("variant-1".to_string()),
        scan_verdict: Some("warn".to_string()),
        findings_count: 0,
    })
    .expect("terminal response")
    .expect("successful parse");

    assert_eq!(
        response,
        (
            true,
            "Imported skill".to_string(),
            Some("variant-1".to_string()),
            Some("warn".to_string()),
            0
        )
    );
}

#[test]
fn skill_publish_terminal_response_ignores_operation_acceptance() {
    let response = parse_skill_publish_terminal_response(DaemonMessage::OperationAccepted {
        operation_id: "op-skill-publish-1".to_string(),
        kind: "skill_publish".to_string(),
        dedup: None,
        revision: 1,
    });
    assert!(response.is_none(), "operation acceptance is not terminal");
}

#[test]
fn skill_publish_terminal_response_extracts_result_payload() {
    let response = parse_skill_publish_terminal_response(DaemonMessage::SkillPublishResult {
        operation_id: Some("op-skill-publish-1".to_string()),
        success: true,
        message: "Published skill".to_string(),
    })
    .expect("terminal response")
    .expect("successful parse");

    assert_eq!(response, (true, "Published skill".to_string()));
}

#[test]
fn config_set_response_accepts_operation_acceptance() {
    parse_config_set_response(DaemonMessage::OperationAccepted {
        operation_id: "op-config-set-1".to_string(),
        kind: "config_set_item".to_string(),
        dedup: None,
        revision: 1,
    })
    .expect("operation acceptance should be treated as success");
}

#[test]
fn external_runtime_migration_response_ignores_unsolicited_daemon_frames() {
    let session_id =
        uuid::Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").expect("valid test uuid");

    assert!(
        parse_external_runtime_migration_terminal_response(DaemonMessage::CwdChanged {
            id: session_id,
            cwd: "/workspace/repo".to_string(),
        })
        .is_none()
    );
}

#[test]
fn external_runtime_migration_response_parses_result_payload() {
    let response = parse_external_runtime_migration_terminal_response(
        DaemonMessage::AgentExternalRuntimeMigrationResult {
            result_json: serde_json::json!({
                "runtime": "daemon",
                "sources": []
            })
            .to_string(),
        },
    )
    .expect("terminal response")
    .expect("successful parse");

    assert_eq!(response["runtime"], "daemon");
    assert_eq!(response["sources"].as_array().map(Vec::len), Some(0));
}

#[test]
fn operation_status_response_extracts_snapshot() {
    let snapshot = parse_operation_status_response(DaemonMessage::OperationStatus {
        snapshot: zorai_protocol::OperationStatusSnapshot {
            operation_id: "op-status-1".to_string(),
            kind: "managed_command".to_string(),
            dedup: Some("managed:exec-1".to_string()),
            state: zorai_protocol::OperationLifecycleState::Started,
            revision: 2,
        },
    })
    .expect("operation status should parse");

    assert_eq!(snapshot.operation_id, "op-status-1");
    assert_eq!(snapshot.kind, "managed_command");
    assert!(matches!(
        snapshot.state,
        zorai_protocol::OperationLifecycleState::Started
    ));
    assert_eq!(snapshot.revision, 2);
}

#[test]
fn thread_control_result_round_trips_through_codec() {
    let bytes = serde_json::to_vec(&DaemonMessage::AgentThreadControlled {
        thread_id: "thread-1".to_string(),
        action: "stop".to_string(),
        ok: true,
    })
    .expect("serialize thread control result");

    let decoded: DaemonMessage =
        serde_json::from_slice(&bytes).expect("deserialize thread control result");
    match decoded {
        DaemonMessage::AgentThreadControlled {
            thread_id,
            action,
            ok,
        } => {
            assert_eq!(thread_id, "thread-1");
            assert_eq!(action, "stop");
            assert!(ok);
        }
        other => panic!("unexpected daemon message: {other:?}"),
    }
}

#[tokio::test]
async fn agent_bridge_ignores_operation_acceptance_messages() {
    let (client_side, server_side) = tokio::io::duplex(1024);
    let mut bridge = Framed::new(client_side, zorai_protocol::ZoraiCodec);
    let mut daemon = Framed::new(server_side, DaemonCodec);

    daemon
        .send(DaemonMessage::OperationAccepted {
            operation_id: "op-bridge-1".to_string(),
            kind: "agent_set_sub_agent".to_string(),
            dedup: None,
            revision: 1,
        })
        .await
        .expect("send operation acceptance");

    let should_continue = handle_bridge_message(&mut bridge)
        .await
        .expect("operation acceptance should not fail bridge handling");

    assert!(should_continue);
}

#[cfg(unix)]
#[tokio::test]
async fn send_thread_get_query_rejects_chunks_for_unexpected_thread() {
    let runtime_dir = tempfile::tempdir().expect("tempdir");
    let socket_path = runtime_dir.path().join("zorai-daemon.sock");
    let listener = tokio::net::UnixListener::bind(&socket_path).expect("bind daemon socket");

    let original_runtime_dir = std::env::var("XDG_RUNTIME_DIR").ok();
    unsafe {
        std::env::set_var("XDG_RUNTIME_DIR", runtime_dir.path());
    }

    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept client connection");
        let mut framed = Framed::new(stream, DaemonCodec);
        let request = framed
            .next()
            .await
            .expect("client request frame")
            .expect("decode client request");
        assert!(matches!(request, ClientMessage::AgentGetThread { .. }));

        let thread_json = serde_json::json!({
            "id": "other-thread",
            "agent_name": null,
            "title": "Wrong thread",
            "messages": [],
            "pinned": false,
            "created_at": 1,
            "updated_at": 1,
            "total_input_tokens": 0,
            "total_output_tokens": 0
        })
        .to_string()
        .into_bytes();

        framed
            .send(DaemonMessage::AgentThreadDetailChunk {
                thread_id: "other-thread".to_string(),
                thread_json_chunk: thread_json,
                done: true,
            })
            .await
            .expect("send mismatched chunk");
    });

    let result = send_thread_get_query("expected-thread".to_string()).await;
    server.await.expect("server task should complete");

    match original_runtime_dir {
        Some(value) => unsafe {
            std::env::set_var("XDG_RUNTIME_DIR", value);
        },
        None => unsafe {
            std::env::remove_var("XDG_RUNTIME_DIR");
        },
    }

    let error = result.expect_err("mismatched chunk should be rejected");
    assert!(
        error
            .to_string()
            .contains("received chunk for unexpected thread"),
        "unexpected error: {error}"
    );
}

#[test]
fn plugin_api_call_bridge_command_carries_params_as_a_json_string_for_the_daemon() {
    let cmd: AgentBridgeCommand = serde_json::from_str(
        r#"{"type":"plugin-api-call","plugin_name":"github","endpoint_name":"get_repo","params":"{\"owner\":\"me\",\"repo\":\"zorai\"}"}"#,
    )
    .expect("plugin-api-call must deserialize");
    match cmd {
        AgentBridgeCommand::PluginApiCallCmd {
            plugin_name,
            endpoint_name,
            params,
        } => {
            assert_eq!(plugin_name, "github");
            assert_eq!(endpoint_name, "get_repo");
            let parsed: serde_json::Value =
                serde_json::from_str(&params).expect("params stay valid json");
            assert_eq!(parsed["repo"], "zorai");
        }
        _ => panic!("unexpected variant for plugin-api-call"),
    }
}
