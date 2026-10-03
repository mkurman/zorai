use super::*;

use zorai_shared::providers::{
    PROVIDER_ID_ALIBABA_CODING_PLAN, PROVIDER_ID_ALIBABA_TOKEN_PLAN, PROVIDER_ID_GITHUB_COPILOT,
    PROVIDER_ID_HUGGINGFACE, PROVIDER_ID_KIMI_CODING_PLAN, PROVIDER_ID_MINIMAX,
    PROVIDER_ID_MINIMAX_CODING_PLAN, PROVIDER_ID_OPENAI, PROVIDER_ID_OPENCODE_GO,
    PROVIDER_ID_OPENCODE_ZEN, PROVIDER_ID_OPENROUTER, PROVIDER_ID_QWEN, PROVIDER_ID_QWEN_DEEPINFRA,
    PROVIDER_ID_Z_AI, PROVIDER_ID_Z_AI_CODING_PLAN,
};

const OPENROUTER_ATTRIBUTION_URL: &str = "https://zorai.app";
const OPENROUTER_ATTRIBUTION_TITLE: &str = "Zorai";
const OPENROUTER_ATTRIBUTION_CATEGORIES: &str = "cli-agent,personal-agent";
const OPENCODE_GO_CLIENT: &str = "zorai";
const OPENCODE_GO_USER_AGENT: &str = "zorai-daemon";

pub(crate) fn normalize_huggingface_provider_selector(value: &str) -> Option<String> {
    let value = value.trim().trim_start_matches(':').to_ascii_lowercase();
    if value.is_empty() || value == "auto" {
        return None;
    }
    let valid = value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'));
    valid.then_some(value)
}

pub(crate) fn huggingface_routed_model(
    provider: &str,
    model: &str,
    selector: Option<&str>,
) -> String {
    let model = model.trim();
    if provider != PROVIDER_ID_HUGGINGFACE {
        return model.to_string();
    }
    let Some(selector) = selector.and_then(normalize_huggingface_provider_selector) else {
        return model.to_string();
    };
    if model
        .rsplit_once(':')
        .and_then(|(_, suffix)| normalize_huggingface_provider_selector(suffix))
        .is_some()
    {
        model.to_string()
    } else {
        format!("{model}:{selector}")
    }
}

pub(crate) fn build_chat_completion_url(base_url: &str) -> String {
    let base = base_url.trim_end_matches('/');
    let lower = base.to_lowercase();

    if lower == "https://api.githubcopilot.com" || lower == "http://api.githubcopilot.com" {
        return format!("{base}/chat/completions");
    }
    if lower == "https://api.deepseek.com" || lower == "http://api.deepseek.com" {
        return format!("{base}/chat/completions");
    }
    if lower == "https://models.github.ai" || lower == "http://models.github.ai" {
        return format!("{base}/inference/chat/completions");
    }
    if lower.ends_with("/inference") && lower.contains("models.github.ai") {
        return format!("{base}/chat/completions");
    }

    if lower.ends_with("/v1")
        || lower.ends_with("/v2")
        || lower.ends_with("/v3")
        || lower.ends_with("/v4")
        || lower.ends_with("/api/v1")
        || lower.ends_with("/openai/v1")
        || lower.ends_with("/compatible-mode/v1")
    {
        return format!("{base}/chat/completions");
    }

    format!("{base}/v1/chat/completions")
}

pub(crate) fn build_responses_url(base_url: &str) -> String {
    let base = base_url.trim_end_matches('/');
    let lower = base.to_lowercase();

    if lower == "https://api.githubcopilot.com" || lower == "http://api.githubcopilot.com" {
        return format!("{base}/responses");
    }
    if lower == "https://models.github.ai" || lower == "http://models.github.ai" {
        return format!("{base}/inference/responses");
    }
    if lower.ends_with("/inference") && lower.contains("models.github.ai") {
        return format!("{base}/responses");
    }

    if lower.ends_with("/v1")
        || lower.ends_with("/v2")
        || lower.ends_with("/v3")
        || lower.ends_with("/v4")
        || lower.ends_with("/api/v1")
        || lower.ends_with("/openai/v1")
        || lower.ends_with("/compatible-mode/v1")
    {
        return format!("{base}/responses");
    }

    format!("{base}/v1/responses")
}

pub(crate) fn normalize_reasoning_effort(effort: &str) -> Option<String> {
    match effort.trim().to_ascii_lowercase().as_str() {
        "" | "off" | "none" => None,
        "minimal" => Some("low".to_string()),
        "low" => Some("low".to_string()),
        "medium" => Some("medium".to_string()),
        "high" => Some("high".to_string()),
        "xhigh" => Some("xhigh".to_string()),
        other => Some(other.to_string()),
    }
}

fn copilot_reasoning_summary(effort: &str) -> Option<&'static str> {
    normalize_reasoning_effort(effort).map(|_| "auto")
}

pub(crate) fn build_openai_responses_body(
    provider: &str,
    config: &ProviderConfig,
    system_prompt: &str,
    messages: &[ApiMessage],
    tools: &[ToolDefinition],
    previous_response_id: Option<&str>,
    codex_auth: bool,
) -> serde_json::Value {
    let request = build_openai_responses_request(
        provider,
        config,
        system_prompt,
        messages,
        tools,
        previous_response_id,
        codex_auth,
    );
    serde_json::to_value(request).expect("responses request should serialize")
}

pub(crate) fn build_openai_responses_request(
    provider: &str,
    config: &ProviderConfig,
    system_prompt: &str,
    messages: &[ApiMessage],
    tools: &[ToolDefinition],
    previous_response_id: Option<&str>,
    codex_auth: bool,
) -> OpenAiResponsesCreateRequest {
    let previous_response_id = (!codex_auth)
        .then_some(previous_response_id)
        .flatten()
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned);
    let mut text = config
        .response_schema
        .as_ref()
        .map(|schema| OpenAiResponsesTextConfig {
            format: Some(OpenAiResponsesTextFormat {
                format_type: "json_schema".to_string(),
                name: "structured_output".to_string(),
                strict: true,
                schema: schema.clone(),
            }),
            verbosity: None,
        });

    if codex_auth {
        text.get_or_insert(OpenAiResponsesTextConfig {
            format: None,
            verbosity: None,
        })
        .verbosity = Some("high".to_string());
    }

    OpenAiResponsesCreateRequest {
        model: huggingface_routed_model(
            provider,
            &config.model,
            config.huggingface_provider.as_deref(),
        ),
        instructions: Some(system_prompt.to_string()),
        input: messages_to_responses_input(provider, messages, previous_response_id.as_deref())
            .into_iter()
            .map(|item| {
                serde_json::from_value(item)
                    .expect("responses input translation should match typed protocol")
            })
            .collect(),
        previous_response_id,
        tools: tools
            .iter()
            .map(|tool| OpenAiResponsesTool {
                tool_type: tool.tool_type.clone(),
                name: tool.function.name.clone(),
                description: tool.function.description.clone(),
                parameters: tool.function.parameters.clone(),
            })
            .collect(),
        tool_choice: (!tools.is_empty()).then_some(OpenAiResponsesToolChoice::Auto),
        text,
        reasoning: normalize_reasoning_effort(&config.reasoning_effort).map(|effort| {
            OpenAiResponsesReasoning {
                effort,
                summary: (provider == PROVIDER_ID_GITHUB_COPILOT)
                    .then(|| copilot_reasoning_summary(&config.reasoning_effort))
                    .flatten()
                    .map(ToOwned::to_owned),
            }
        }),
        store: codex_auth.then_some(false),
        include: if codex_auth {
            vec!["reasoning.encrypted_content".to_string()]
        } else {
            Vec::new()
        },
        stream: true,
    }
}

pub(crate) fn openai_reasoning_supported(provider: &str, model: &str) -> bool {
    matches!(
        provider,
        PROVIDER_ID_OPENAI
            | PROVIDER_ID_OPENROUTER
            | PROVIDER_ID_QWEN
            | PROVIDER_ID_QWEN_DEEPINFRA
            | PROVIDER_ID_OPENCODE_ZEN
            | PROVIDER_ID_Z_AI
            | PROVIDER_ID_Z_AI_CODING_PLAN
    ) || model.starts_with('o')
        || model.starts_with("gpt-5")
        || model.starts_with("gpt-6")
}

pub(crate) fn dashscope_openai_uses_enable_thinking(provider: &str, model: &str) -> bool {
    matches!(
        provider,
        PROVIDER_ID_QWEN | PROVIDER_ID_ALIBABA_CODING_PLAN | PROVIDER_ID_ALIBABA_TOKEN_PLAN
    ) && matches!(
        model,
        "qwen3.6-plus" | "qwen3.5-plus" | "qwen3-max-2026-01-23" | "glm-4.7" | "glm-5"
    )
}

pub(crate) fn is_dashscope_coding_plan_anthropic_base_url(base_url: &str) -> bool {
    let lower = base_url.trim().to_ascii_lowercase();
    lower.contains("dashscope.aliyuncs.com") && lower.contains("/apps/anthropic")
}

/// Providers whose "coding plan" tiers gate access behind SDK-identification
/// headers (User-Agent, x-stainless-*).  Without them the gateway returns 405.
pub(crate) fn needs_coding_plan_sdk_headers(provider: &str) -> bool {
    matches!(
        provider,
        PROVIDER_ID_ALIBABA_CODING_PLAN
            | PROVIDER_ID_KIMI_CODING_PLAN
            | PROVIDER_ID_MINIMAX
            | PROVIDER_ID_MINIMAX_CODING_PLAN
    )
}

pub(crate) fn apply_dashscope_coding_plan_sdk_headers(
    req: reqwest::RequestBuilder,
    provider: &str,
    _base_url: &str,
    api_type: ApiType,
) -> reqwest::RequestBuilder {
    if !needs_coding_plan_sdk_headers(provider) {
        return req;
    }

    let sdk_version = match api_type {
        ApiType::Anthropic => "0.73.0",
        ApiType::OpenAI | ApiType::Gemini => "4.3.0",
    };
    req.header("User-Agent", api_type.sdk_user_agent())
        .header("x-stainless-lang", "js")
        .header("x-stainless-package-version", sdk_version)
        .header("x-stainless-os", std::env::consts::OS)
        .header("x-stainless-arch", std::env::consts::ARCH)
        .header("x-stainless-runtime", "node")
        .header("x-stainless-runtime-version", "v22.0.0")
}

pub(crate) fn anthropic_thinking_budget(effort: &str) -> Option<u32> {
    match effort.trim().to_ascii_lowercase().as_str() {
        "" | "off" | "none" => None,
        "minimal" => Some(512),
        "low" => Some(1024),
        "medium" => Some(4096),
        "high" => Some(8192),
        "xhigh" => Some(16384),
        "max" => Some(32768),
        _ => Some(4096),
    }
}

pub(crate) fn build_openai_auth_request<'a>(
    client: &'a reqwest::Client,
    url: &str,
    provider: &str,
    config: &ProviderConfig,
    copilot_initiator: CopilotInitiator,
    force_connection_close: bool,
) -> reqwest::RequestBuilder {
    maybe_force_connection_close(
        apply_openai_auth_headers(
            client.post(url).header("Content-Type", "application/json"),
            provider,
            config,
            copilot_initiator,
        ),
        force_connection_close,
    )
}

pub(crate) fn apply_openrouter_attribution_headers(
    req: reqwest::RequestBuilder,
    provider: &str,
) -> reqwest::RequestBuilder {
    if provider != PROVIDER_ID_OPENROUTER {
        return req;
    }

    req.header("HTTP-Referer", OPENROUTER_ATTRIBUTION_URL)
        .header("X-OpenRouter-Title", OPENROUTER_ATTRIBUTION_TITLE)
        .header("X-OpenRouter-Categories", OPENROUTER_ATTRIBUTION_CATEGORIES)
}

fn resolve_opencode_go_session_id(session_id: Option<&str>) -> String {
    session_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string())
}

/// OpenCode Go requires a stable `x-opencode-session` for prompt-cache routing
/// and a specific client identity (no broad User-Agent).
pub(crate) fn apply_opencode_go_headers(
    req: reqwest::RequestBuilder,
    provider: &str,
    session_id: Option<&str>,
) -> reqwest::RequestBuilder {
    if provider != PROVIDER_ID_OPENCODE_GO {
        return req;
    }

    req.header("User-Agent", OPENCODE_GO_USER_AGENT)
        .header("x-opencode-client", OPENCODE_GO_CLIENT)
        .header(
            "x-opencode-session",
            resolve_opencode_go_session_id(session_id),
        )
}

pub(crate) fn insert_opencode_go_headers(
    headers: &mut reqwest::header::HeaderMap,
    provider: &str,
    session_id: Option<&str>,
) {
    if provider != PROVIDER_ID_OPENCODE_GO {
        return;
    }

    let session = resolve_opencode_go_session_id(session_id);
    if let Ok(value) = reqwest::header::HeaderValue::from_str(OPENCODE_GO_USER_AGENT) {
        headers.insert(reqwest::header::USER_AGENT, value);
    }
    if let Ok(value) = reqwest::header::HeaderValue::from_str(OPENCODE_GO_CLIENT) {
        headers.insert("x-opencode-client", value);
    }
    if let Ok(value) = reqwest::header::HeaderValue::from_str(&session) {
        headers.insert("x-opencode-session", value);
    }
}

fn apply_openrouter_response_cache_headers(
    req: reqwest::RequestBuilder,
    provider: &str,
    config: &ProviderConfig,
) -> reqwest::RequestBuilder {
    if provider != PROVIDER_ID_OPENROUTER || !config.openrouter_response_cache_enabled {
        return req;
    }
    req.header("X-OpenRouter-Cache", "true")
}

pub(crate) fn maybe_force_connection_close(
    req: reqwest::RequestBuilder,
    force_connection_close: bool,
) -> reqwest::RequestBuilder {
    if force_connection_close {
        req.header(reqwest::header::CONNECTION, "close")
    } else {
        req
    }
}

pub(crate) fn apply_openai_auth_headers(
    req: reqwest::RequestBuilder,
    provider: &str,
    config: &ProviderConfig,
    copilot_initiator: CopilotInitiator,
) -> reqwest::RequestBuilder {
    if provider == PROVIDER_ID_GITHUB_COPILOT {
        let req = req
            .header("Accept", "application/json")
            .header("Openai-Intent", "conversation-edits")
            .header("x-initiator", copilot_initiator.as_header_value())
            .header("User-Agent", "zorai-daemon");
        if let Some(resolved) = super::super::copilot_auth::resolve_github_copilot_auth(
            &config.api_key,
            config.auth_source,
        ) {
            if let Some(token) = resolved
                .access_token
                .as_deref()
                .filter(|token| !token.trim().is_empty())
            {
                return req
                    .header("Authorization", format!("Bearer {token}"))
                    .header("editor-version", "zorai/0.1.10");
            }
        }
        return req;
    }

    if !config.api_key.is_empty() {
        let auth_method = get_provider_definition(provider)
            .map(|d| d.auth_method)
            .unwrap_or(AuthMethod::Bearer);
        apply_openrouter_response_cache_headers(
            apply_openrouter_attribution_headers(auth_method.apply(req, &config.api_key), provider),
            provider,
            config,
        )
    } else {
        apply_openrouter_response_cache_headers(
            apply_openrouter_attribution_headers(req, provider),
            provider,
            config,
        )
    }
}

pub(crate) fn build_native_assistant_base_url(
    provider: &str,
    config: &ProviderConfig,
) -> Option<String> {
    let preferred =
        get_provider_definition(provider).and_then(|definition| definition.native_base_url);
    preferred
        .or_else(|| (!config.base_url.trim().is_empty()).then_some(config.base_url.as_str()))
        .map(|url| url.trim_end_matches('/').to_string())
}

pub(crate) fn api_message_to_text(message: &ApiMessage) -> Option<String> {
    match &message.content {
        ApiContent::Text(text) => Some(text.clone()),
        ApiContent::Blocks(blocks) => {
            let combined = blocks
                .iter()
                .filter_map(|block| {
                    block
                        .get("text")
                        .and_then(|value| value.as_str())
                        .or_else(|| {
                            block.get("type").and_then(|value| value.as_str()).and_then(
                                |block_type| match block_type {
                                    "input_image" => Some("[image attachment]"),
                                    "input_audio" => Some("[audio attachment]"),
                                    _ => None,
                                },
                            )
                        })
                        .map(ToOwned::to_owned)
                })
                .collect::<Vec<_>>()
                .join("\n");
            (!combined.trim().is_empty()).then_some(combined)
        }
    }
}
