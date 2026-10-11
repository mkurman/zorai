use super::*;

const PRIMARY_ROUTE_SWITCH_COMMAND: &str = "!svarog";
const PRIMARY_ROUTE_SWITCH_COMMAND_LEGACY: &str = "!swarog";
const PRIMARY_ROUTE_EXACT_PHRASES: &[&str] = &[
    "switch to svarog",
    "switch to swarog",
    "talk to svarog",
    "talk to swarog",
    "connect me to svarog",
    "connect me to swarog",
    "put svarog on",
    "put swarog on",
    "use svarog",
    "use swarog",
    "hand me to svarog",
    "hand me to swarog",
    "route me to svarog",
    "route me to swarog",
    "switch me to svarog",
    "switch me to swarog",
    "i want svarog",
    "i want swarog",
];
const PRIMARY_ROUTE_PHRASES: &[&str] = &[
    "switch to svarog",
    "switch to swarog",
    "switch me to svarog",
    "switch me to swarog",
    "talk to svarog",
    "talk to swarog",
    "let svarog handle",
    "let swarog handle",
    "have svarog handle",
    "have swarog handle",
    "route this to svarog",
    "route this to swarog",
    "svarog take over",
    "swarog take over",
    "svarog, take over",
    "swarog, take over",
];
const CONCIERGE_ROUTE_EXACT_PHRASES: &[&str] = &[
    "switch to rarog",
    "switch back to rarog",
    "switch back to concierge",
    "talk to rarog",
    "connect me to rarog",
    "put rarog on",
    "use rarog",
    "hand me back to rarog",
    "route me to rarog",
    "switch me to rarog",
    "i want rarog",
];
const CONCIERGE_ROUTE_PHRASES: &[&str] = &[
    "switch to rarog",
    "switch back to rarog",
    "switch back to concierge",
    "talk to rarog",
    "let rarog handle",
    "have rarog handle",
    "route this to rarog",
    "rarog take this back",
    "rarog, take this back",
];

pub(crate) fn platform_health_from_snapshot(
    snapshot: &zorai_protocol::GatewayHealthState,
) -> PlatformHealthState {
    let mut health = PlatformHealthState::new();
    health.status = match snapshot.status {
        zorai_protocol::GatewayConnectionStatus::Connected => GatewayConnectionStatus::Connected,
        zorai_protocol::GatewayConnectionStatus::Disconnected => {
            GatewayConnectionStatus::Disconnected
        }
        zorai_protocol::GatewayConnectionStatus::Error => GatewayConnectionStatus::Error,
    };
    health.last_success_at = snapshot.last_success_at_ms;
    health.last_error_at = snapshot.last_error_at_ms;
    health.consecutive_failure_count = snapshot.consecutive_failure_count;
    health.last_error = snapshot.last_error.clone();
    health.current_backoff_secs = snapshot.current_backoff_secs;
    health
}

pub(crate) fn snapshot_from_platform_health(
    platform: &str,
    health: &PlatformHealthState,
) -> zorai_protocol::GatewayHealthState {
    zorai_protocol::GatewayHealthState {
        platform: platform.to_string(),
        status: match health.status {
            GatewayConnectionStatus::Connected => {
                zorai_protocol::GatewayConnectionStatus::Connected
            }
            GatewayConnectionStatus::Disconnected => {
                zorai_protocol::GatewayConnectionStatus::Disconnected
            }
            GatewayConnectionStatus::Error => zorai_protocol::GatewayConnectionStatus::Error,
        },
        last_success_at_ms: health.last_success_at,
        last_error_at_ms: health.last_error_at,
        consecutive_failure_count: health.consecutive_failure_count,
        last_error: health.last_error.clone(),
        current_backoff_secs: health.current_backoff_secs,
    }
}

pub(crate) fn apply_health_snapshot(
    gateway_state: &mut gateway::GatewayState,
    snapshot: &zorai_protocol::GatewayHealthState,
) {
    let health = platform_health_from_snapshot(snapshot);
    match snapshot.platform.as_str() {
        "slack" => gateway_state.slack_health = health,
        "discord" => gateway_state.discord_health = health,
        "telegram" => gateway_state.telegram_health = health,
        _ => {}
    }
}

#[derive(Default)]
pub(super) struct GatewayRuntimeControl {
    pub(super) restart_attempts: u32,
    pub(super) restart_not_before_ms: Option<u64>,
}

pub(super) fn gateway_runtime_control() -> &'static Mutex<GatewayRuntimeControl> {
    static CONTROL: OnceLock<Mutex<GatewayRuntimeControl>> = OnceLock::new();
    CONTROL.get_or_init(|| Mutex::new(GatewayRuntimeControl::default()))
}

pub(super) fn is_gateway_reset_command(trimmed_lower: &str) -> bool {
    matches!(trimmed_lower, "!reset" | "!new")
}

pub(super) fn parse_gateway_approval_decision(
    content: &str,
) -> Option<zorai_protocol::ApprovalDecision> {
    match content.trim().to_ascii_lowercase().as_str() {
        "approve-once" | "approve_once" | "approve once" | "allow-once" | "allow_once"
        | "allow once" => Some(zorai_protocol::ApprovalDecision::ApproveOnce),
        "approve-session" | "approve_session" | "approve session" | "allow-session"
        | "allow_session" | "allow session" => {
            Some(zorai_protocol::ApprovalDecision::ApproveSession)
        }
        "deny" | "denied" | "reject" => Some(zorai_protocol::ApprovalDecision::Deny),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GatewayControlCommand {
    Status,
    Pause,
    Resume,
    Rerun,
}

pub(super) fn parse_gateway_control_command(content: &str) -> Option<GatewayControlCommand> {
    match content.trim().to_ascii_lowercase().as_str() {
        "!status" => Some(GatewayControlCommand::Status),
        "!pause" => Some(GatewayControlCommand::Pause),
        "!resume" => Some(GatewayControlCommand::Resume),
        "!rerun" => Some(GatewayControlCommand::Rerun),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct GatewayRouteRequest {
    pub(super) mode: gateway::GatewayRouteMode,
    pub(super) ack_only: bool,
}

fn gateway_switch_command(trimmed_lower: &str) -> Option<gateway::GatewayRouteMode> {
    match trimmed_lower {
        PRIMARY_ROUTE_SWITCH_COMMAND | PRIMARY_ROUTE_SWITCH_COMMAND_LEGACY | "!main" => {
            Some(gateway::GatewayRouteMode::Swarog)
        }
        "!rarog" | "!concierge" => Some(gateway::GatewayRouteMode::Rarog),
        _ => None,
    }
}

pub(super) fn classify_gateway_route_request(content: &str) -> Option<GatewayRouteRequest> {
    let trimmed = content.trim();
    let trimmed_lower = trimmed.to_ascii_lowercase();
    if let Some(mode) = gateway_switch_command(&trimmed_lower) {
        return Some(GatewayRouteRequest {
            mode,
            ack_only: true,
        });
    }

    let normalized = trimmed_lower
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    if PRIMARY_ROUTE_EXACT_PHRASES.contains(&normalized.as_str()) {
        return Some(GatewayRouteRequest {
            mode: gateway::GatewayRouteMode::Swarog,
            ack_only: true,
        });
    }
    if CONCIERGE_ROUTE_EXACT_PHRASES.contains(&normalized.as_str()) {
        return Some(GatewayRouteRequest {
            mode: gateway::GatewayRouteMode::Rarog,
            ack_only: true,
        });
    }

    if PRIMARY_ROUTE_PHRASES
        .iter()
        .any(|phrase| normalized.contains(phrase))
    {
        return Some(GatewayRouteRequest {
            mode: gateway::GatewayRouteMode::Swarog,
            ack_only: false,
        });
    }

    if CONCIERGE_ROUTE_PHRASES
        .iter()
        .any(|phrase| normalized.contains(phrase))
    {
        return Some(GatewayRouteRequest {
            mode: gateway::GatewayRouteMode::Rarog,
            ack_only: false,
        });
    }

    None
}

pub(super) fn gateway_route_confirmation(mode: gateway::GatewayRouteMode) -> String {
    match mode {
        gateway::GatewayRouteMode::Swarog => format!(
            "Switched this channel to {}. I will keep routing here to {} until you ask for {} back.",
            MAIN_AGENT_NAME, MAIN_AGENT_NAME, CONCIERGE_AGENT_NAME
        ),
        gateway::GatewayRouteMode::Rarog => format!(
            "Switched this channel back to {}. I will keep routing here to {} until you ask for {}.",
            CONCIERGE_AGENT_NAME, CONCIERGE_AGENT_NAME, MAIN_AGENT_NAME
        ),
    }
}

pub(super) fn gateway_reply_args(
    platform: &str,
    channel: &str,
    message: &str,
) -> serde_json::Value {
    match platform.to_ascii_lowercase().as_str() {
        "discord" => serde_json::json!({"channel_id": channel, "message": message}),
        "slack" => serde_json::json!({"channel": channel, "message": message}),
        "telegram" => serde_json::json!({"chat_id": channel, "message": message}),
        "whatsapp" => serde_json::json!({"phone": channel, "message": message}),
        _ => serde_json::json!({"message": message}),
    }
}

pub(super) fn gateway_thread_title(msg: &gateway::IncomingMessage) -> String {
    format!("{} {}", msg.platform, msg.sender)
}

pub(super) fn gateway_default_agent_for_platform(gateway: &GatewayConfig, platform: &str) -> String {
    let platform_default = match platform.to_ascii_lowercase().as_str() {
        "slack" => &gateway.slack_default_agent,
        "discord" => &gateway.discord_default_agent,
        "telegram" => &gateway.telegram_default_agent,
        "whatsapp" => &gateway.whatsapp_default_agent,
        _ => &gateway.default_agent,
    };
    if platform_default.trim().is_empty() {
        gateway.default_agent.clone()
    } else {
        platform_default.clone()
    }
}

pub(super) fn gateway_reply_tool(platform: &str, channel: &str) -> (String, &'static str) {
    match platform.to_ascii_lowercase().as_str() {
        "discord" => (
            format!("send_discord_message with channel_id=\"{}\"", channel),
            zorai_protocol::tool_names::SEND_DISCORD_MESSAGE,
        ),
        "slack" => (
            format!("send_slack_message with channel=\"{}\"", channel),
            zorai_protocol::tool_names::SEND_SLACK_MESSAGE,
        ),
        "telegram" => (
            format!("send_telegram_message with chat_id=\"{}\"", channel),
            zorai_protocol::tool_names::SEND_TELEGRAM_MESSAGE,
        ),
        "whatsapp" => (
            format!("send_whatsapp_message with phone=\"{}\"", channel),
            zorai_protocol::tool_names::SEND_WHATSAPP_MESSAGE,
        ),
        _ => (
            "the appropriate gateway tool".to_string(),
            zorai_protocol::tool_names::SEND_DISCORD_MESSAGE,
        ),
    }
}

fn latest_gateway_turn_slice(messages: &[AgentMessage]) -> &[AgentMessage] {
    let turn_start = messages
        .iter()
        .rposition(|message| message.role == MessageRole::User)
        .unwrap_or(0);
    &messages[turn_start..]
}

fn is_gateway_send_tool_message(message: &AgentMessage) -> bool {
    message.role == MessageRole::Tool
        && message
            .tool_name
            .as_deref()
            .map(|name| name.starts_with("send_"))
            .unwrap_or(false)
}

fn assistant_message_has_gateway_send_tool_call(message: &AgentMessage) -> bool {
    message
        .tool_calls
        .as_ref()
        .map(|tool_calls| {
            tool_calls
                .iter()
                .any(|tool_call| tool_call.function.name.starts_with("send_"))
        })
        .unwrap_or(false)
}

pub(super) fn gateway_turn_used_send_tool(messages: &[AgentMessage]) -> bool {
    let turn = latest_gateway_turn_slice(messages);
    let Some(latest_assistant_index) = turn.iter().rposition(|message| {
        message.role == MessageRole::Assistant
            && (!message.content.is_empty()
                || message
                    .tool_calls
                    .as_ref()
                    .map(|tool_calls| !tool_calls.is_empty())
                    .unwrap_or(false))
    }) else {
        return turn.iter().any(is_gateway_send_tool_message);
    };

    if assistant_message_has_gateway_send_tool_call(&turn[latest_assistant_index]) {
        return true;
    }

    turn[latest_assistant_index + 1..]
        .iter()
        .any(is_gateway_send_tool_message)
}

pub(super) fn latest_gateway_turn_assistant_response(messages: &[AgentMessage]) -> Option<String> {
    latest_gateway_turn_slice(messages)
        .iter()
        .rev()
        .find(|message| message.role == MessageRole::Assistant && !message.content.is_empty())
        .map(|message| message.content.clone())
}

pub(super) fn build_gateway_agent_prompt(
    platform: &str,
    sender: &str,
    content: &str,
    history_window: Option<&str>,
    reply_tool_name: &str,
    active_responder_name: Option<&str>,
) -> String {
    let mut prompt = format!(
        "[{platform} message from {sender}]: {content}\n\n\
         Recent channel history is auto-injected for continuity. \
         If you need more context, call fetch_gateway_history with a larger count.\n\
         Reply naturally in plain text. Your final assistant response will be delivered back to the user automatically.\n\
         If you expect a longer multi-step tool run and want to send an early progress update, you may call {reply_tool_name} first with a brief acknowledgment such as \"On it, give me a moment...\", then continue with the rest of the work.",
    );
    if let Some(active_responder_name) =
        active_responder_name.filter(|value| !value.trim().is_empty())
    {
        prompt.push_str("\n\n");
        prompt.push_str(&format!(
            "Current active responder for this thread: {active_responder_name}. You are already speaking as {active_responder_name} here. \
             Do not use `handoff_thread_agent` or `message_agent` to reach {active_responder_name} itself. \
             If the operator asks to talk to {active_responder_name}, answer directly as {active_responder_name}. \
             Only use `handoff_thread_agent` when switching ownership to a different agent persona.",
        ));
    }
    if let Some(window) = history_window.filter(|value| !value.trim().is_empty()) {
        prompt.push_str("\n\nPrevious 10 messages from this channel (oldest first):\n");
        prompt.push_str(window);
    }
    prompt
}

pub(super) const GATEWAY_TRIAGE_TIMEOUT_SECS: u64 = 12;
pub(super) const GATEWAY_AGENT_TIMEOUT_SECS: u64 = 120;
pub(super) const GATEWAY_AGENT_TIMEOUT_HIGH_REASONING_SECS: u64 = 420;
pub(super) const GATEWAY_STREAM_TIMEOUT_HIGH_REASONING_SECS: u64 = 300;
pub(super) const GATEWAY_SEND_RESULT_TIMEOUT_SECS: u64 = 180;
pub(super) const GATEWAY_EVENT_DRAIN_INTERVAL_MS: u64 = 150;

fn is_high_reasoning_effort(effort: &str) -> bool {
    matches!(
        effort.trim().to_ascii_lowercase().as_str(),
        "high" | "very_high" | "xhigh" | "max"
    )
}

pub(super) fn gateway_agent_timeout_for_reasoning(effort: &str) -> std::time::Duration {
    std::time::Duration::from_secs(if is_high_reasoning_effort(effort) {
        GATEWAY_AGENT_TIMEOUT_HIGH_REASONING_SECS
    } else {
        GATEWAY_AGENT_TIMEOUT_SECS
    })
}

pub(super) fn gateway_stream_timeout_for_reasoning(effort: &str) -> std::time::Duration {
    std::time::Duration::from_secs(if is_high_reasoning_effort(effort) {
        GATEWAY_STREAM_TIMEOUT_HIGH_REASONING_SECS
    } else {
        GATEWAY_AGENT_TIMEOUT_SECS
    })
}
