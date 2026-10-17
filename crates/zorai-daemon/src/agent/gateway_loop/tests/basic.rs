use super::*;

#[test]
fn reset_commands_require_bang_prefix() {
    assert!(is_gateway_reset_command("!reset"));
    assert!(is_gateway_reset_command("!new"));
    assert!(!is_gateway_reset_command("reset"));
    assert!(!is_gateway_reset_command("new"));
    assert!(!is_gateway_reset_command("!renew"));
}

#[test]
fn gateway_incoming_event_carries_existing_thread_binding() {
    let source = fs::read_to_string(
        repo_root().join("crates/zorai-daemon/src/agent/gateway_loop/message_flow.rs"),
    )
    .expect("read message_flow.rs");
    assert!(
        source.contains("thread_id: existing_thread.clone()"),
        "GatewayIncoming must carry the bound daemon thread so the UI does not dump the message on whatever thread is currently open"
    );
}

#[test]
fn gateway_route_requests_support_commands_and_natural_language() {
    assert_eq!(
        classify_gateway_route_request("!svarog"),
        Some(GatewayRouteRequest {
            mode: gateway::GatewayRouteMode::Swarog,
            ack_only: true,
        })
    );
    assert_eq!(
        classify_gateway_route_request("!swarog"),
        Some(GatewayRouteRequest {
            mode: gateway::GatewayRouteMode::Swarog,
            ack_only: true,
        })
    );
    assert_eq!(
        classify_gateway_route_request("switch to svarog"),
        Some(GatewayRouteRequest {
            mode: gateway::GatewayRouteMode::Swarog,
            ack_only: true,
        })
    );
    assert_eq!(
        classify_gateway_route_request("switch to swarog"),
        Some(GatewayRouteRequest {
            mode: gateway::GatewayRouteMode::Swarog,
            ack_only: true,
        })
    );
    assert_eq!(
        classify_gateway_route_request("switch to svarog and take over this channel"),
        Some(GatewayRouteRequest {
            mode: gateway::GatewayRouteMode::Swarog,
            ack_only: false,
        })
    );
    assert_eq!(
        classify_gateway_route_request("switch back to rarog"),
        Some(GatewayRouteRequest {
            mode: gateway::GatewayRouteMode::Rarog,
            ack_only: true,
        })
    );
    assert_eq!(
        classify_gateway_route_request("rarog, take this back and answer directly"),
        Some(GatewayRouteRequest {
            mode: gateway::GatewayRouteMode::Rarog,
            ack_only: false,
        })
    );
    assert_eq!(
        classify_gateway_route_request("what does svarog think?"),
        None
    );
}

#[tokio::test]
async fn gateway_default_responder_routes_concierge_main_and_personas() {
    let root = make_test_root("gateway-default-responder");
    let manager = SessionManager::new_test(&root).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), &root).await;

    async fn responder(engine: &AgentEngine, agent: &str) -> (bool, Option<String>) {
        let mut config = engine.get_config().await;
        config.gateway.default_agent = agent.to_string();
        engine.set_config(config).await;
        engine.resolve_gateway_default_responder("Telegram").await
    }

    assert_eq!(responder(&engine, "rarog").await, (true, None));
    assert_eq!(responder(&engine, "").await, (true, None));
    assert_eq!(responder(&engine, "swarog").await, (false, None));
    assert_eq!(responder(&engine, "main").await, (false, None));
    assert_eq!(responder(&engine, "totally-unknown").await, (false, None));
    assert_eq!(
        responder(&engine, "radogost").await,
        (false, Some("radogost".to_string()))
    );
}

#[test]
fn gateway_reply_helpers_accept_lowercase_platform_names() {
    assert_eq!(
        gateway_reply_args("discord", "chan-1", "hello"),
        serde_json::json!({"channel_id": "chan-1", "message": "hello"})
    );
    assert_eq!(
        gateway_reply_tool("discord", "chan-1"),
        (
            "send_discord_message with channel_id=\"chan-1\"".to_string(),
            "send_discord_message"
        )
    );
}

#[test]
fn gateway_approval_decision_parser_accepts_canonical_reply_tokens() {
    assert!(matches!(
        parse_gateway_approval_decision("approve-once"),
        Some(zorai_protocol::ApprovalDecision::ApproveOnce)
    ));
    assert!(matches!(
        parse_gateway_approval_decision("approve once"),
        Some(zorai_protocol::ApprovalDecision::ApproveOnce)
    ));
    assert!(matches!(
        parse_gateway_approval_decision("approve session"),
        Some(zorai_protocol::ApprovalDecision::ApproveSession)
    ));
    assert!(matches!(
        parse_gateway_approval_decision("deny"),
        Some(zorai_protocol::ApprovalDecision::Deny)
    ));
    assert!(matches!(
        parse_gateway_approval_decision("denied"),
        Some(zorai_protocol::ApprovalDecision::Deny)
    ));
    assert!(matches!(parse_gateway_approval_decision("what now?"), None));
}

#[test]
fn gateway_control_command_parser_accepts_supported_commands() {
    assert_eq!(
        parse_gateway_control_command("!status"),
        Some(GatewayControlCommand::Status)
    );
    assert_eq!(
        parse_gateway_control_command("!pause"),
        Some(GatewayControlCommand::Pause)
    );
    assert_eq!(
        parse_gateway_control_command("!resume"),
        Some(GatewayControlCommand::Resume)
    );
    assert_eq!(
        parse_gateway_control_command("!rerun"),
        Some(GatewayControlCommand::Rerun)
    );
    assert_eq!(parse_gateway_control_command("status"), None);
}

#[test]
fn gateway_prompt_prefers_auto_delivery_over_forced_send_tool() {
    let prompt = build_gateway_agent_prompt(
        "discord",
        "alice",
        "What model are you?",
        Some("- user: hi"),
        "send_discord_message",
        None,
    );

    assert!(
        prompt.contains("delivered back to the user automatically"),
        "gateway prompt should explain automatic delivery"
    );
    assert!(
        !prompt.contains("YOU MUST CALL"),
        "gateway prompt should not force a gateway send tool call"
    );
}

#[test]
fn gateway_prompt_tells_active_responder_not_to_handoff_to_itself() {
    let prompt = build_gateway_agent_prompt(
        "discord",
        "alice",
        "Give me svarog",
        None,
        "send_discord_message",
        Some("Svarog"),
    );

    assert!(prompt.contains("Current active responder for this thread: Svarog."));
    assert!(prompt
        .contains("Do not use `handoff_thread_agent` or `message_agent` to reach Svarog itself."));
    assert!(prompt.contains("If the operator asks to talk to Svarog, answer directly as Svarog."));
}

#[test]
fn gateway_high_reasoning_timeout_budgets_are_extended() {
    assert_eq!(gateway_stream_timeout_for_reasoning("high").as_secs(), 300);
    assert_eq!(gateway_stream_timeout_for_reasoning("off").as_secs(), 120);
}

#[test]
fn gateway_channel_key_normalizes_platform_label_casing() {
    assert_eq!(gateway::gateway_channel_key("Slack", "C123"), "Slack:C123");
    assert_eq!(gateway::gateway_channel_key("slack", "C123"), "Slack:C123");
    assert_eq!(
        gateway::gateway_channel_key("DISCORD", "user:123"),
        "Discord:user:123"
    );
    assert_eq!(
        gateway::gateway_channel_key("telegram", "123456"),
        "Telegram:123456"
    );
}

#[test]
fn gateway_auto_send_ignores_historic_send_tools_from_prior_turns() {
    let messages = vec![
        AgentMessage {
            id: "user-1".to_string(),
            role: MessageRole::User,
            content: "old question".to_string(),
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
            message_kind: AgentMessageKind::Normal,
            compaction_strategy: None,
            compaction_payload: None,
            offloaded_payload_id: None,
            tool_output_preview_path: None,
            structural_refs: Vec::new(),
            pinned_for_compaction: false,
            timestamp: 1,
            feedback: None,
        },
        AgentMessage {
            id: "assistant-1".to_string(),
            role: MessageRole::Assistant,
            content: "old answer".to_string(),
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
            message_kind: AgentMessageKind::Normal,
            compaction_strategy: None,
            compaction_payload: None,
            offloaded_payload_id: None,
            tool_output_preview_path: None,
            structural_refs: Vec::new(),
            pinned_for_compaction: false,
            timestamp: 2,
            feedback: None,
        },
        AgentMessage {
            id: "tool-1".to_string(),
            role: MessageRole::Tool,
            content: "Discord message sent".to_string(),
            content_blocks: Vec::new(),
            tool_calls: None,
            tool_call_id: Some("call-1".to_string()),
            tool_name: Some("send_discord_message".to_string()),
            tool_arguments: Some("{\"channel_id\":\"chan-1\"}".to_string()),
            tool_status: Some("done".to_string()),
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
            message_kind: AgentMessageKind::Normal,
            compaction_strategy: None,
            compaction_payload: None,
            offloaded_payload_id: None,
            tool_output_preview_path: None,
            structural_refs: Vec::new(),
            pinned_for_compaction: false,
            timestamp: 3,
            feedback: None,
        },
        AgentMessage::user("What model are you bro?", 4),
        AgentMessage {
            id: "assistant-2".to_string(),
            role: MessageRole::Assistant,
            content: "I am running gpt-test.".to_string(),
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
            message_kind: AgentMessageKind::Normal,
            compaction_strategy: None,
            compaction_payload: None,
            offloaded_payload_id: None,
            tool_output_preview_path: None,
            structural_refs: Vec::new(),
            pinned_for_compaction: false,
            timestamp: 5,
            feedback: None,
        },
    ];

    assert!(
        !gateway_turn_used_send_tool(&messages),
        "only send tools from the current turn should suppress auto-send"
    );
    assert_eq!(
        latest_gateway_turn_assistant_response(&messages).as_deref(),
        Some("I am running gpt-test.")
    );
}

#[test]
fn gateway_auto_send_keeps_latest_assistant_message_pending_after_earlier_send_tool() {
    let messages = vec![
        AgentMessage::user("Can you check this?", 1),
        AgentMessage {
            id: "assistant-1".to_string(),
            role: MessageRole::Assistant,
            content: "On it, give me a moment...".to_string(),
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
            message_kind: AgentMessageKind::Normal,
            compaction_strategy: None,
            compaction_payload: None,
            offloaded_payload_id: None,
            tool_output_preview_path: None,
            structural_refs: Vec::new(),
            pinned_for_compaction: false,
            timestamp: 2,
            feedback: None,
        },
        AgentMessage {
            id: "tool-1".to_string(),
            role: MessageRole::Tool,
            content: "Discord message sent".to_string(),
            content_blocks: Vec::new(),
            tool_calls: None,
            tool_call_id: Some("call-1".to_string()),
            tool_name: Some("send_discord_message".to_string()),
            tool_arguments: Some(
                "{\"channel_id\":\"chan-1\",\"message\":\"On it, give me a moment...\"}"
                    .to_string(),
            ),
            tool_status: Some("done".to_string()),
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
            message_kind: AgentMessageKind::Normal,
            compaction_strategy: None,
            compaction_payload: None,
            offloaded_payload_id: None,
            tool_output_preview_path: None,
            structural_refs: Vec::new(),
            pinned_for_compaction: false,
            timestamp: 3,
            feedback: None,
        },
        AgentMessage {
            id: "assistant-2".to_string(),
            role: MessageRole::Assistant,
            content: "I found the issue. The release notes need one more line.".to_string(),
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
            message_kind: AgentMessageKind::Normal,
            compaction_strategy: None,
            compaction_payload: None,
            offloaded_payload_id: None,
            tool_output_preview_path: None,
            structural_refs: Vec::new(),
            pinned_for_compaction: false,
            timestamp: 4,
            feedback: None,
        },
    ];

    assert!(
        !gateway_turn_used_send_tool(&messages),
        "a send tool before the latest assistant message should not suppress auto-send of that later message"
    );
    assert_eq!(
        latest_gateway_turn_assistant_response(&messages).as_deref(),
        Some("I found the issue. The release notes need one more line.")
    );
}

#[test]
fn gateway_auto_send_treats_tool_call_only_assistant_messages_as_latest_activity() {
    let messages = vec![
        AgentMessage::user("Can you post a quick update?", 1),
        AgentMessage {
            id: "assistant-1".to_string(),
            role: MessageRole::Assistant,
            content: "On it.".to_string(),
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
            message_kind: AgentMessageKind::Normal,
            compaction_strategy: None,
            compaction_payload: None,
            offloaded_payload_id: None,
            tool_output_preview_path: None,
            structural_refs: Vec::new(),
            pinned_for_compaction: false,
            timestamp: 2,
            feedback: None,
        },
        AgentMessage {
            id: "assistant-2".to_string(),
            role: MessageRole::Assistant,
            content: String::new(),
            content_blocks: Vec::new(),
            tool_calls: Some(vec![ToolCall::with_default_weles_review(
                "call-1".to_string(),
                ToolFunction {
                    name: "send_discord_message".to_string(),
                    arguments: "{\"channel_id\":\"chan-1\",\"message\":\"Still checking\"}"
                        .to_string(),
                },
            )]),
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
            message_kind: AgentMessageKind::Normal,
            compaction_strategy: None,
            compaction_payload: None,
            offloaded_payload_id: None,
            tool_output_preview_path: None,
            structural_refs: Vec::new(),
            pinned_for_compaction: false,
            timestamp: 3,
            feedback: None,
        },
    ];

    assert!(
        gateway_turn_used_send_tool(&messages),
        "assistant tool-call chunks without visible content should still suppress auto-send"
    );
}

#[test]
fn daemon_gateway_loop_no_longer_polls_slack_discord_or_telegram() {
    let production_source = gateway_loop_production_source();
    for forbidden in [
        "gateway::poll_telegram(gw).await",
        "gateway::poll_slack(gw, &slack_channels).await",
        "gateway::poll_discord(gw, &discord_channels).await",
        "gateway::fetch_telegram_replay(gw).await",
        "gateway::fetch_slack_replay(gw, ch).await",
        "gateway::fetch_discord_replay(gw, ch).await",
    ] {
        assert!(
            !production_source.contains(forbidden),
            "gateway loop still contains local transport ownership seam: {forbidden}"
        );
    }
}

#[test]
fn daemon_gateway_send_path_no_longer_issues_platform_http_requests() {
    let gateway_source =
        fs::read_to_string(repo_root().join("crates/zorai-daemon/src/agent/gateway.rs"))
            .expect("read gateway.rs");
    let tool_root = repo_root().join("crates/zorai-daemon/src/agent/tool_executor");
    let mut tool_paths = fs::read_dir(&tool_root)
        .expect("read tool_executor dir")
        .map(|entry| entry.expect("tool_executor dir entry").path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("rs"))
        .collect::<Vec<_>>();
    tool_paths.sort();
    let tool_source = tool_paths
        .into_iter()
        .map(|path| fs::read_to_string(path).expect("read split tool_executor source"))
        .collect::<Vec<_>>()
        .join("\n");
    for forbidden in [
        "https://slack.com/api",
        "https://discord.com/api/v10",
        "https://api.telegram.org",
        "conversations.history",
        "users/@me/channels",
        "getUpdates?offset=",
    ] {
        assert!(
            !gateway_source.contains(forbidden) && !tool_source.contains(forbidden),
            "daemon transport source still contains platform HTTP path: {forbidden}"
        );
    }
    assert!(
        !tool_source.contains("gateway_format::"),
        "daemon send path still depends on daemon-owned gateway formatting"
    );
}

#[test]
fn daemon_gateway_loop_boxes_large_delivery_futures() {
    let production_source = gateway_loop_production_source();

    for required in [
        "Box::pin(self.process_gateway_messages()).await",
        "let triage = match Box::pin(tokio::time::timeout(",
        "let send_result = Box::pin(self.send_message_with_agent_scope_override(",
    ] {
        assert!(
            production_source.contains(required),
            "gateway loop hot path should box oversized future: {required}"
        );
    }
}
