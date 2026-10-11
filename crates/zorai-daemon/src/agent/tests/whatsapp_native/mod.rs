use super::*;
use crate::agent::load_persisted_provider_state;
use crate::agent::types::AgentConfig;
use crate::agent::{gateway::GatewayState, gateway_loop::process_replay_result};
use crate::session_manager::SessionManager;
use tempfile::tempdir;

mod part1;
mod part2;

fn make_replay_gateway_config() -> super::super::types::GatewayConfig {
    super::super::types::GatewayConfig {
        enabled: true,
        slack_token: String::new(),
        slack_channel_filter: String::new(),
        telegram_token: String::new(),
        telegram_allowed_chats: String::new(),
        discord_token: String::new(),
        discord_channel_filter: String::new(),
        discord_allowed_users: String::new(),
        whatsapp_allowed_contacts: String::new(),
        whatsapp_token: String::new(),
        whatsapp_phone_id: String::new(),
        command_prefix: "!".into(),
        default_agent: "rarog".into(),
        slack_default_agent: String::new(),
        discord_default_agent: String::new(),
        telegram_default_agent: String::new(),
        whatsapp_default_agent: String::new(),
        gateway_electron_bridges_enabled: false,
        whatsapp_link_fallback_electron: false,
    }
}

async fn simulate_live_whatsapp_enqueue(
    allowlist: &str,
    sender: &str,
    chat: &str,
    own_identifiers: std::collections::HashSet<String>,
    exact_self_jids: Vec<String>,
    is_from_me: bool,
    known_outbound_echo: bool,
    text: &str,
) -> (WhatsAppEnqueueDecision, usize, Option<String>) {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let mut gw = GatewayState::new(make_replay_gateway_config(), reqwest::Client::new());
    let mut seen_ids = Vec::new();
    let decision = classify_whatsapp_enqueue_decision(
        text,
        chat,
        sender,
        &own_identifiers,
        &exact_self_jids,
        known_outbound_echo,
        is_from_me,
        allowlist,
    );

    let result = gateway::ReplayFetchResult::Replay(vec![gateway::ReplayEnvelope {
        message: IncomingMessage {
            platform: "WhatsApp".into(),
            sender: sender.into(),
            content: if matches!(decision, WhatsAppEnqueueDecision::Enqueue) {
                text.into()
            } else {
                String::new()
            },
            channel: chat.into(),
            message_id: Some("wa:LIVE001".into()),
            thread_context: None,
        },
        channel_id: normalize_identifier(chat),
        cursor_value: build_whatsapp_cursor(1700000100, "LIVE001"),
        cursor_type: "ts_msgid",
    }]);

    let (accepted, _) =
        process_replay_result(&engine.history, "whatsapp", result, &mut gw, &mut seen_ids).await;
    *engine.gateway_state.lock().await = Some(gw);

    for msg in accepted {
        if matches!(decision, WhatsAppEnqueueDecision::Enqueue) {
            engine
                .enqueue_gateway_message(msg)
                .await
                .expect("enqueue live whatsapp message");
        }
    }

    let queue_len = engine.gateway_injected_messages.lock().await.len();
    let cursor = engine
        .gateway_state
        .lock()
        .await
        .as_ref()
        .and_then(|state| {
            state
                .whatsapp_replay_cursors
                .get(&normalize_identifier(chat))
                .cloned()
        });
    (decision, queue_len, cursor)
}
