use super::context;
use super::*;
pub use context::known_context_window_for;
use zorai_shared::providers::*;

pub struct ProviderDef {
    pub id: &'static str,
    pub name: &'static str,
    pub default_base_url: &'static str,
    pub default_model: &'static str,
    pub supported_transports: &'static [&'static str],
    pub default_transport: &'static str,
    pub supported_auth_sources: &'static [&'static str],
    pub default_auth_source: &'static str,
    #[allow(dead_code)]
    pub native_base_url: Option<&'static str>,
}

pub const CHAT_ONLY_TRANSPORTS: &[&str] = &["chat_completions"];
pub const RESPONSES_AND_CHAT_TRANSPORTS: &[&str] = &["responses", "chat_completions"];
pub const RESPONSES_CHAT_AND_ANTHROPIC_TRANSPORTS: &[&str] =
    &["responses", "chat_completions", "anthropic_messages"];
pub const NATIVE_AND_CHAT_TRANSPORTS: &[&str] = &["native_assistant", "chat_completions"];
pub const CHAT_AND_ANTHROPIC_TRANSPORTS: &[&str] = &["chat_completions", "anthropic_messages"];
pub const API_KEY_ONLY_AUTH_SOURCES: &[&str] = &["api_key"];
pub const OPENAI_AUTH_SOURCES: &[&str] = &["chatgpt_subscription", "api_key"];
pub const GITHUB_COPILOT_AUTH_SOURCES: &[&str] = &["github_copilot", "api_key"];
pub const DEFAULT_CUSTOM_MODEL_CONTEXT_WINDOW: u32 = 264_000;

fn normalize_model_lookup_value(value: &str) -> String {
    value.trim().to_lowercase()
}

pub fn default_custom_model_context_window() -> u32 {
    DEFAULT_CUSTOM_MODEL_CONTEXT_WINDOW
}

pub fn resolve_context_window_for_provider_auth(
    provider_id: &str,
    auth_source: &str,
    model_id: &str,
    custom_model_name: &str,
) -> Option<u32> {
    let lookup_values = [model_id, custom_model_name]
        .into_iter()
        .map(normalize_model_lookup_value)
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();

    if lookup_values.is_empty() {
        return None;
    }

    known_models_for_provider_auth(provider_id, auth_source)
        .into_iter()
        .find(|model| {
            let id = normalize_model_lookup_value(&model.id);
            let name = model
                .name
                .as_deref()
                .map(normalize_model_lookup_value)
                .unwrap_or_default();
            lookup_values
                .iter()
                .any(|value| value == &id || (!name.is_empty() && value == &name))
        })
        .and_then(|model| model.context_window)
}

pub fn model_uses_context_window_override(
    provider_id: &str,
    auth_source: &str,
    model_id: &str,
    custom_model_name: &str,
) -> bool {
    if provider_id == PROVIDER_ID_CUSTOM {
        return true;
    }

    (model_id.trim().len() > 0 || custom_model_name.trim().len() > 0)
        && resolve_context_window_for_provider_auth(
            provider_id,
            auth_source,
            model_id,
            custom_model_name,
        )
        .is_none()
}

pub fn provider_uses_configurable_base_url(provider_id: &str) -> bool {
    matches!(provider_id, PROVIDER_ID_CUSTOM | PROVIDER_ID_AZURE_OPENAI)
}

pub fn provider_base_url_is_customized(provider_id: &str, base_url: &str) -> bool {
    let trimmed = base_url.trim();
    if trimmed.is_empty() {
        return false;
    }
    match find_by_id(provider_id) {
        Some(def) => trimmed != def.default_base_url && def.native_base_url != Some(trimmed),
        None => true,
    }
}

pub const PROVIDERS: &[ProviderDef] = &[
    ProviderDef {
        id: PROVIDER_ID_OPENAI,
        name: "OpenAI / ChatGPT",
        default_base_url: "https://api.openai.com/v1",
        default_model: "gpt-5.5",
        supported_transports: RESPONSES_AND_CHAT_TRANSPORTS,
        default_transport: "responses",
        supported_auth_sources: OPENAI_AUTH_SOURCES,
        default_auth_source: "api_key",
        native_base_url: None,
    },
    ProviderDef {
        id: PROVIDER_ID_DEEPSEEK,
        name: "DeepSeek",
        default_base_url: "https://api.deepseek.com",
        default_model: "deepseek-v4-pro",
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: "chat_completions",
        supported_auth_sources: API_KEY_ONLY_AUTH_SOURCES,
        default_auth_source: "api_key",
        native_base_url: None,
    },
    ProviderDef {
        id: PROVIDER_ID_XAI,
        name: "xAI",
        default_base_url: "https://api.x.ai/v1",
        default_model: "grok-4.6",
        supported_transports: RESPONSES_AND_CHAT_TRANSPORTS,
        default_transport: "responses",
        supported_auth_sources: API_KEY_ONLY_AUTH_SOURCES,
        default_auth_source: "api_key",
        native_base_url: None,
    },
    ProviderDef {
        id: PROVIDER_ID_ELEVENLABS,
        name: "ElevenLabs",
        default_base_url: "https://api.elevenlabs.io",
        default_model: "scribe_v2",
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: "chat_completions",
        supported_auth_sources: API_KEY_ONLY_AUTH_SOURCES,
        default_auth_source: "api_key",
        native_base_url: None,
    },
    ProviderDef {
        id: PROVIDER_ID_AZURE_OPENAI,
        name: "Azure OpenAI",
        default_base_url: "https://YOUR-RESOURCE-NAME.openai.azure.com/openai/v1",
        default_model: "",
        supported_transports: RESPONSES_AND_CHAT_TRANSPORTS,
        default_transport: "responses",
        supported_auth_sources: API_KEY_ONLY_AUTH_SOURCES,
        default_auth_source: "api_key",
        native_base_url: None,
    },
    ProviderDef {
        id: PROVIDER_ID_ANTHROPIC,
        name: "Anthropic",
        default_base_url: "https://api.anthropic.com",
        default_model: "claude-opus-4-7",
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: "chat_completions",
        supported_auth_sources: API_KEY_ONLY_AUTH_SOURCES,
        default_auth_source: "api_key",
        native_base_url: None,
    },
    ProviderDef {
        id: PROVIDER_ID_GITHUB_COPILOT,
        name: "GitHub Copilot",
        default_base_url: "https://api.githubcopilot.com",
        default_model: "gpt-5.4",
        supported_transports: RESPONSES_CHAT_AND_ANTHROPIC_TRANSPORTS,
        default_transport: "responses",
        supported_auth_sources: GITHUB_COPILOT_AUTH_SOURCES,
        default_auth_source: "github_copilot",
        native_base_url: None,
    },
    ProviderDef {
        id: PROVIDER_ID_GROQ,
        name: "Groq",
        default_base_url: "https://api.groq.com/openai/v1",
        default_model: "llama-3.3-70b-versatile",
        supported_transports: RESPONSES_AND_CHAT_TRANSPORTS,
        default_transport: "responses",
        supported_auth_sources: API_KEY_ONLY_AUTH_SOURCES,
        default_auth_source: "api_key",
        native_base_url: None,
    },
    ProviderDef {
        id: PROVIDER_ID_OLLAMA,
        name: "Ollama",
        default_base_url: "http://localhost:11434/v1",
        default_model: "llama3.1",
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: "chat_completions",
        supported_auth_sources: API_KEY_ONLY_AUTH_SOURCES,
        default_auth_source: "api_key",
        native_base_url: None,
    },
    ProviderDef {
        id: PROVIDER_ID_TOGETHER,
        name: "Together",
        default_base_url: "https://api.together.xyz/v1",
        default_model: "meta-llama/Llama-3.3-70B-Instruct-Turbo",
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: "chat_completions",
        supported_auth_sources: API_KEY_ONLY_AUTH_SOURCES,
        default_auth_source: "api_key",
        native_base_url: None,
    },
    ProviderDef {
        id: PROVIDER_ID_ARCEE,
        name: "Arcee",
        default_base_url: "https://api.arcee.ai/api/v1",
        default_model: "trinity-large-thinking",
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: "chat_completions",
        supported_auth_sources: API_KEY_ONLY_AUTH_SOURCES,
        default_auth_source: "api_key",
        native_base_url: None,
    },
    ProviderDef {
        id: PROVIDER_ID_NVIDIA,
        name: "NVIDIA",
        default_base_url: "https://integrate.api.nvidia.com/v1",
        default_model: "minimaxai/minimax-m2.7",
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: "chat_completions",
        supported_auth_sources: API_KEY_ONLY_AUTH_SOURCES,
        default_auth_source: "api_key",
        native_base_url: None,
    },
    ProviderDef {
        id: PROVIDER_ID_POOLSIDE,
        name: "Poolside",
        default_base_url: "https://inference.poolside.ai/v1",
        default_model: "poolside/laguna-m.1",
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: "chat_completions",
        supported_auth_sources: API_KEY_ONLY_AUTH_SOURCES,
        default_auth_source: "api_key",
        native_base_url: None,
    },
    ProviderDef {
        id: PROVIDER_ID_OPENROUTER,
        name: "OpenRouter",
        default_base_url: "https://openrouter.ai/api/v1",
        default_model: "arcee-ai/trinity-large-thinking",
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: "chat_completions",
        supported_auth_sources: API_KEY_ONLY_AUTH_SOURCES,
        default_auth_source: "api_key",
        native_base_url: None,
    },
    ProviderDef {
        id: PROVIDER_ID_CEREBRAS,
        name: "Cerebras",
        default_base_url: "https://api.cerebras.ai/v1",
        default_model: "llama-3.3-70b",
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: "chat_completions",
        supported_auth_sources: API_KEY_ONLY_AUTH_SOURCES,
        default_auth_source: "api_key",
        native_base_url: None,
    },
    ProviderDef {
        id: PROVIDER_ID_QWEN,
        name: "Qwen",
        default_base_url: "https://dashscope-intl.aliyuncs.com/compatible-mode/v1",
        default_model: "qwen-max",
        supported_transports: NATIVE_AND_CHAT_TRANSPORTS,
        default_transport: "native_assistant",
        supported_auth_sources: API_KEY_ONLY_AUTH_SOURCES,
        default_auth_source: "api_key",
        native_base_url: Some("https://dashscope-intl.aliyuncs.com/api/v1"),
    },
    ProviderDef {
        id: PROVIDER_ID_KIMI,
        name: "Kimi (Moonshot)",
        default_base_url: "https://api.moonshot.ai/v1",
        default_model: "moonshot-v1-32k",
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: "chat_completions",
        supported_auth_sources: API_KEY_ONLY_AUTH_SOURCES,
        default_auth_source: "api_key",
        native_base_url: None,
    },
    ProviderDef {
        id: PROVIDER_ID_Z_AI,
        name: "Z.AI (GLM)",
        default_base_url: "https://api.z.ai/api/paas/v4",
        default_model: "glm-4-plus",
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: "chat_completions",
        supported_auth_sources: API_KEY_ONLY_AUTH_SOURCES,
        default_auth_source: "api_key",
        native_base_url: None,
    },
    ProviderDef {
        id: PROVIDER_ID_Z_AI_CODING_PLAN,
        name: "Z.AI Coding Plan",
        default_base_url: "https://api.z.ai/api/coding/paas/v4",
        default_model: "glm-5.3",
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: "chat_completions",
        supported_auth_sources: API_KEY_ONLY_AUTH_SOURCES,
        default_auth_source: "api_key",
        native_base_url: None,
    },
    ProviderDef {
        id: PROVIDER_ID_KIMI_CODING_PLAN,
        name: "Kimi Coding Plan",
        default_base_url: "https://api.kimi.com/coding/v1",
        default_model: "k3",
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: "chat_completions",
        supported_auth_sources: API_KEY_ONLY_AUTH_SOURCES,
        default_auth_source: "api_key",
        native_base_url: None,
    },
    ProviderDef {
        id: PROVIDER_ID_MINIMAX,
        name: "MiniMax",
        default_base_url: "https://api.minimax.io/anthropic",
        default_model: "MiniMax-M3",
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: "chat_completions",
        supported_auth_sources: API_KEY_ONLY_AUTH_SOURCES,
        default_auth_source: "api_key",
        native_base_url: None,
    },
    ProviderDef {
        id: PROVIDER_ID_MINIMAX_CODING_PLAN,
        name: "MiniMax Coding Plan",
        default_base_url: "https://api.minimax.io/anthropic",
        default_model: "MiniMax-M3",
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: "chat_completions",
        supported_auth_sources: API_KEY_ONLY_AUTH_SOURCES,
        default_auth_source: "api_key",
        native_base_url: None,
    },
    ProviderDef {
        id: PROVIDER_ID_ALIBABA_CODING_PLAN,
        name: "Alibaba Coding Plan",
        default_base_url: "https://coding-intl.dashscope.aliyuncs.com/v1",
        default_model: "qwen3.6-plus",
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: "chat_completions",
        supported_auth_sources: API_KEY_ONLY_AUTH_SOURCES,
        default_auth_source: "api_key",
        native_base_url: None,
    },
    ProviderDef {
        id: PROVIDER_ID_XIAOMI_MIMO,
        name: "Xiaomi MiMo",
        default_base_url: "https://api.xiaomimimo.com/v1",
        default_model: "mimo-v2.5-pro",
        supported_transports: &["chat_completions", "anthropic_messages"],
        default_transport: "chat_completions",
        supported_auth_sources: API_KEY_ONLY_AUTH_SOURCES,
        default_auth_source: "api_key",
        native_base_url: None,
    },
    ProviderDef {
        id: PROVIDER_ID_XIAOMI_MIMO_TOKEN_PLAN,
        name: "Xiaomi MiMo Token Plan",
        default_base_url: "https://token-plan-ams.xiaomimimo.com/v1",
        default_model: "mimo-v2.5-pro",
        supported_transports: &["chat_completions", "anthropic_messages"],
        default_transport: "chat_completions",
        supported_auth_sources: API_KEY_ONLY_AUTH_SOURCES,
        default_auth_source: "api_key",
        native_base_url: None,
    },
    ProviderDef {
        id: PROVIDER_ID_NOUS_PORTAL,
        name: "Nous Portal",
        default_base_url: "https://inference-api.nousresearch.com/v1",
        default_model: "nousresearch/hermes-4-70b",
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: "chat_completions",
        supported_auth_sources: API_KEY_ONLY_AUTH_SOURCES,
        default_auth_source: "api_key",
        native_base_url: None,
    },
    ProviderDef {
        id: PROVIDER_ID_HERMES_AGENT_API,
        name: "Hermes Agent API",
        default_base_url: "http://localhost:8642/v1",
        default_model: "hermes-agent",
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: "chat_completions",
        supported_auth_sources: API_KEY_ONLY_AUTH_SOURCES,
        default_auth_source: "api_key",
        native_base_url: None,
    },
    ProviderDef {
        id: PROVIDER_ID_QWEN_DEEPINFRA,
        name: "Qwen (DeepInfra)",
        default_base_url: "https://api.deepinfra.com/v1/openai",
        default_model: "Qwen/Qwen2.5-72B-Instruct",
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: "chat_completions",
        supported_auth_sources: API_KEY_ONLY_AUTH_SOURCES,
        default_auth_source: "api_key",
        native_base_url: None,
    },
    ProviderDef {
        id: PROVIDER_ID_HUGGINGFACE,
        name: "Hugging Face",
        default_base_url: "https://router.huggingface.co/v1",
        default_model: "meta-llama/Llama-3.3-70B-Instruct",
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: "chat_completions",
        supported_auth_sources: API_KEY_ONLY_AUTH_SOURCES,
        default_auth_source: "api_key",
        native_base_url: None,
    },
    ProviderDef {
        id: PROVIDER_ID_CHUTES,
        name: "Chutes",
        default_base_url: "https://llm.chutes.ai/v1",
        default_model: "deepseek-ai/DeepSeek-R1",
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: "chat_completions",
        supported_auth_sources: API_KEY_ONLY_AUTH_SOURCES,
        default_auth_source: "api_key",
        native_base_url: None,
    },
    ProviderDef {
        id: PROVIDER_ID_FEATHERLESS,
        name: "Featherless",
        default_base_url: "https://api.featherless.ai/v1",
        default_model: "meta-llama/Llama-3.3-70B-Instruct",
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: "chat_completions",
        supported_auth_sources: API_KEY_ONLY_AUTH_SOURCES,
        default_auth_source: "api_key",
        native_base_url: None,
    },
    ProviderDef {
        id: PROVIDER_ID_OPENCODE_ZEN,
        name: "OpenCode Zen",
        default_base_url: "https://opencode.ai/zen/v1",
        default_model: "claude-sonnet-4-6",
        supported_transports: CHAT_AND_ANTHROPIC_TRANSPORTS,
        default_transport: "chat_completions",
        supported_auth_sources: API_KEY_ONLY_AUTH_SOURCES,
        default_auth_source: "api_key",
        native_base_url: None,
    },
    ProviderDef {
        id: PROVIDER_ID_OPENCODE_GO,
        name: "OpenCode Go",
        default_base_url: "https://opencode.ai/zen/go/v1",
        default_model: "glm-5.1",
        supported_transports: CHAT_AND_ANTHROPIC_TRANSPORTS,
        default_transport: "chat_completions",
        supported_auth_sources: API_KEY_ONLY_AUTH_SOURCES,
        default_auth_source: "api_key",
        native_base_url: None,
    },
    ProviderDef {
        id: PROVIDER_ID_GEMINI,
        name: "Gemini",
        default_base_url: "https://generativelanguage.googleapis.com/v1beta",
        default_model: "gemini-3.8-flash",
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: "chat_completions",
        supported_auth_sources: API_KEY_ONLY_AUTH_SOURCES,
        default_auth_source: "api_key",
        native_base_url: None,
    },
    ProviderDef {
        id: PROVIDER_ID_CUSTOM,
        name: "Custom",
        default_base_url: "",
        default_model: "",
        supported_transports: RESPONSES_AND_CHAT_TRANSPORTS,
        default_transport: "responses",
        supported_auth_sources: API_KEY_ONLY_AUTH_SOURCES,
        default_auth_source: "api_key",
        native_base_url: None,
    },
];

/// Find a provider definition by its id.
pub fn find_by_id(id: &str) -> Option<&'static ProviderDef> {
    PROVIDERS.iter().find(|p| p.id == id)
}

/// Find a provider definition by its display name.
#[allow(dead_code)]
pub fn find_by_name(name: &str) -> Option<&'static ProviderDef> {
    PROVIDERS.iter().find(|p| p.name == name)
}

pub fn supported_transports_for(provider: &str) -> &'static [&'static str] {
    find_by_id(provider)
        .map(|def| def.supported_transports)
        .unwrap_or(CHAT_ONLY_TRANSPORTS)
}

pub fn default_transport_for(provider: &str) -> &'static str {
    find_by_id(provider)
        .map(|def| def.default_transport)
        .unwrap_or("chat_completions")
}

pub fn fixed_transport_for_model(provider: &str, model: &str) -> Option<&'static str> {
    zorai_shared::providers::fixed_api_transport_for_model(provider, model)
}

pub fn uses_fixed_anthropic_messages(provider: &str, model: &str) -> bool {
    matches!(
        provider,
        PROVIDER_ID_ANTHROPIC | PROVIDER_ID_MINIMAX | PROVIDER_ID_MINIMAX_CODING_PLAN
    ) || ((provider == PROVIDER_ID_OPENCODE_ZEN || provider == PROVIDER_ID_OPENCODE_GO)
        && model.starts_with("claude"))
}

pub fn supported_auth_sources_for(provider: &str) -> &'static [&'static str] {
    find_by_id(provider)
        .map(|def| def.supported_auth_sources)
        .unwrap_or(API_KEY_ONLY_AUTH_SOURCES)
}

pub fn default_auth_source_for(provider: &str) -> &'static str {
    find_by_id(provider)
        .map(|def| def.default_auth_source)
        .unwrap_or("api_key")
}

pub fn supports_model_fetch_for(provider: &str) -> bool {
    !matches!(
        provider,
        PROVIDER_ID_CUSTOM
            | PROVIDER_ID_FEATHERLESS
            | PROVIDER_ID_KIMI_CODING_PLAN
            | PROVIDER_ID_Z_AI
            | PROVIDER_ID_Z_AI_CODING_PLAN
            | PROVIDER_ID_HUGGINGFACE
            | PROVIDER_ID_ANTHROPIC
            | PROVIDER_ID_ALIBABA_CODING_PLAN
            | PROVIDER_ID_XIAOMI_MIMO
            | PROVIDER_ID_XIAOMI_MIMO_TOKEN_PLAN
    )
}

pub fn default_model_for_provider_auth(provider: &str, auth_source: &str) -> String {
    known_models_for_provider_auth(provider, auth_source)
        .first()
        .map(|model| model.id.clone())
        .unwrap_or_else(|| {
            find_by_id(provider)
                .map(|def| def.default_model.to_string())
                .unwrap_or_default()
        })
}
