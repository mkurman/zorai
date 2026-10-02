use super::*;

use zorai_shared::providers::*;

pub const CHAT_ONLY_TRANSPORTS: &[ApiTransport] = &[ApiTransport::ChatCompletions];
pub const RESPONSES_AND_CHAT_TRANSPORTS: &[ApiTransport] =
    &[ApiTransport::Responses, ApiTransport::ChatCompletions];
pub const RESPONSES_CHAT_AND_ANTHROPIC_TRANSPORTS: &[ApiTransport] = &[
    ApiTransport::Responses,
    ApiTransport::ChatCompletions,
    ApiTransport::AnthropicMessages,
];
pub const NATIVE_AND_CHAT_TRANSPORTS: &[ApiTransport] =
    &[ApiTransport::NativeAssistant, ApiTransport::ChatCompletions];
pub const CHAT_AND_ANTHROPIC_TRANSPORTS: &[ApiTransport] = &[
    ApiTransport::ChatCompletions,
    ApiTransport::AnthropicMessages,
];

pub const PROVIDER_DEFINITIONS: &[ProviderDefinition] = &[
    ProviderDefinition {
        id: PROVIDER_ID_FEATHERLESS,
        name: "Featherless",
        default_base_url: "https://api.featherless.ai/v1",
        default_model: "meta-llama/Llama-3.3-70B-Instruct",
        api_type: ApiType::OpenAI,
        auth_method: AuthMethod::Bearer,
        models: FEATHERLESS_MODELS,
        supports_model_fetch: false,
        anthropic_base_url: None,
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: ApiTransport::ChatCompletions,
        native_transport_kind: None,
        native_base_url: None,
        supports_response_continuity: false,
    },
    ProviderDefinition {
        id: PROVIDER_ID_NVIDIA,
        name: "NVIDIA",
        default_base_url: "https://integrate.api.nvidia.com/v1",
        default_model: "minimaxai/minimax-m2.7",
        api_type: ApiType::OpenAI,
        auth_method: AuthMethod::Bearer,
        models: NVIDIA_MODELS,
        supports_model_fetch: true,
        anthropic_base_url: None,
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: ApiTransport::ChatCompletions,
        native_transport_kind: None,
        native_base_url: None,
        supports_response_continuity: false,
    },
    ProviderDefinition {
        id: PROVIDER_ID_POOLSIDE,
        name: "Poolside",
        default_base_url: "https://inference.poolside.ai/v1",
        default_model: "poolside/laguna-m.1",
        api_type: ApiType::OpenAI,
        auth_method: AuthMethod::Bearer,
        models: POOLSIDE_MODELS,
        supports_model_fetch: true,
        anthropic_base_url: None,
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: ApiTransport::ChatCompletions,
        native_transport_kind: None,
        native_base_url: None,
        supports_response_continuity: false,
    },
    ProviderDefinition {
        id: PROVIDER_ID_OPENAI,
        name: "OpenAI",
        default_base_url: "https://api.openai.com/v1",
        default_model: "gpt-5.5",
        api_type: ApiType::OpenAI,
        auth_method: AuthMethod::Bearer,
        models: OPENAI_MODELS,
        supports_model_fetch: true,
        anthropic_base_url: None,
        supported_transports: RESPONSES_AND_CHAT_TRANSPORTS,
        default_transport: ApiTransport::Responses,
        native_transport_kind: None,
        native_base_url: None,
        supports_response_continuity: true,
    },
    ProviderDefinition {
        id: PROVIDER_ID_DEEPSEEK,
        name: "DeepSeek",
        default_base_url: "https://api.deepseek.com",
        default_model: "deepseek-v4-pro",
        api_type: ApiType::OpenAI,
        auth_method: AuthMethod::Bearer,
        models: DEEPSEEK_MODELS,
        supports_model_fetch: true,
        anthropic_base_url: None,
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: ApiTransport::ChatCompletions,
        native_transport_kind: None,
        native_base_url: None,
        supports_response_continuity: false,
    },
    ProviderDefinition {
        id: PROVIDER_ID_XAI,
        name: "xAI",
        default_base_url: "https://api.x.ai/v1",
        default_model: "grok-4.6",
        api_type: ApiType::OpenAI,
        auth_method: AuthMethod::Bearer,
        models: XAI_MODELS,
        supports_model_fetch: true,
        anthropic_base_url: None,
        supported_transports: RESPONSES_AND_CHAT_TRANSPORTS,
        default_transport: ApiTransport::Responses,
        native_transport_kind: None,
        native_base_url: None,
        supports_response_continuity: false,
    },
    ProviderDefinition {
        id: PROVIDER_ID_ELEVENLABS,
        name: "ElevenLabs",
        default_base_url: "https://api.elevenlabs.io",
        default_model: "scribe_v2",
        api_type: ApiType::OpenAI,
        auth_method: AuthMethod::XiApiKey,
        models: ELEVENLABS_MODELS,
        supports_model_fetch: false,
        anthropic_base_url: None,
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: ApiTransport::ChatCompletions,
        native_transport_kind: None,
        native_base_url: None,
        supports_response_continuity: false,
    },
    ProviderDefinition {
        id: PROVIDER_ID_AZURE_OPENAI,
        name: "Azure OpenAI",
        default_base_url: "https://YOUR-RESOURCE-NAME.openai.azure.com/openai/v1",
        default_model: "",
        api_type: ApiType::OpenAI,
        auth_method: AuthMethod::Bearer,
        models: EMPTY_MODELS,
        supports_model_fetch: true,
        anthropic_base_url: None,
        supported_transports: RESPONSES_AND_CHAT_TRANSPORTS,
        default_transport: ApiTransport::Responses,
        native_transport_kind: None,
        native_base_url: None,
        supports_response_continuity: true,
    },
    ProviderDefinition {
        id: PROVIDER_ID_ANTHROPIC,
        name: "Anthropic",
        default_base_url: "https://api.anthropic.com",
        default_model: "claude-opus-4-7",
        api_type: ApiType::Anthropic,
        auth_method: AuthMethod::XApiKey,
        models: ANTHROPIC_MODELS,
        supports_model_fetch: false,
        anthropic_base_url: None,
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: ApiTransport::ChatCompletions,
        native_transport_kind: None,
        native_base_url: None,
        supports_response_continuity: false,
    },
    ProviderDefinition {
        id: PROVIDER_ID_GITHUB_COPILOT,
        name: "GitHub Copilot",
        default_base_url: "https://api.githubcopilot.com",
        default_model: "gpt-5.4",
        api_type: ApiType::OpenAI,
        auth_method: AuthMethod::Bearer,
        models: GITHUB_COPILOT_MODELS,
        supports_model_fetch: true,
        anthropic_base_url: None,
        supported_transports: RESPONSES_CHAT_AND_ANTHROPIC_TRANSPORTS,
        default_transport: ApiTransport::Responses,
        native_transport_kind: None,
        native_base_url: None,
        supports_response_continuity: false,
    },
    ProviderDefinition {
        id: PROVIDER_ID_QWEN,
        name: "Qwen",
        default_base_url: "https://dashscope-intl.aliyuncs.com/compatible-mode/v1",
        default_model: "qwen-max",
        api_type: ApiType::OpenAI,
        auth_method: AuthMethod::Bearer,
        models: QWEN_MODELS,
        supports_model_fetch: true,
        anthropic_base_url: None,
        supported_transports: NATIVE_AND_CHAT_TRANSPORTS,
        default_transport: ApiTransport::NativeAssistant,
        native_transport_kind: Some(NativeTransportKind::AlibabaAssistantApi),
        native_base_url: Some("https://dashscope-intl.aliyuncs.com/api/v1"),
        supports_response_continuity: false,
    },
    ProviderDefinition {
        id: PROVIDER_ID_QWEN_DEEPINFRA,
        name: "Qwen (DeepInfra)",
        default_base_url: "https://api.deepinfra.com/v1/openai",
        default_model: "Qwen/Qwen2.5-72B-Instruct",
        api_type: ApiType::OpenAI,
        auth_method: AuthMethod::Bearer,
        models: QWEN_MODELS,
        supports_model_fetch: true,
        anthropic_base_url: None,
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: ApiTransport::ChatCompletions,
        native_transport_kind: None,
        native_base_url: None,
        supports_response_continuity: false,
    },
    ProviderDefinition {
        id: PROVIDER_ID_KIMI,
        name: "Kimi (Moonshot)",
        default_base_url: "https://api.moonshot.ai/v1",
        default_model: "moonshot-v1-32k",
        api_type: ApiType::OpenAI,
        auth_method: AuthMethod::Bearer,
        models: KIMI_MODELS,
        supports_model_fetch: true,
        anthropic_base_url: None,
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: ApiTransport::ChatCompletions,
        native_transport_kind: None,
        native_base_url: None,
        supports_response_continuity: false,
    },
    ProviderDefinition {
        id: PROVIDER_ID_KIMI_CODING_PLAN,
        name: "Kimi Coding Plan",
        default_base_url: "https://api.kimi.com/coding/v1",
        default_model: "k3",
        api_type: ApiType::OpenAI,
        auth_method: AuthMethod::Bearer,
        models: KIMI_CODING_MODELS,
        supports_model_fetch: true,
        anthropic_base_url: None,
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: ApiTransport::ChatCompletions,
        native_transport_kind: None,
        native_base_url: None,
        supports_response_continuity: false,
    },
    ProviderDefinition {
        id: PROVIDER_ID_Z_AI,
        name: "Z.AI (GLM)",
        default_base_url: "https://api.z.ai/api/paas/v4",
        default_model: "glm-4-plus",
        api_type: ApiType::OpenAI,
        auth_method: AuthMethod::Bearer,
        models: ZAI_MODELS,
        supports_model_fetch: false,
        anthropic_base_url: None,
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: ApiTransport::ChatCompletions,
        native_transport_kind: None,
        native_base_url: None,
        supports_response_continuity: false,
    },
    ProviderDefinition {
        id: PROVIDER_ID_Z_AI_CODING_PLAN,
        name: "Z.AI Coding Plan",
        default_base_url: "https://api.z.ai/api/coding/paas/v4",
        default_model: "glm-5.3",
        api_type: ApiType::OpenAI,
        auth_method: AuthMethod::Bearer,
        models: ZAI_CODING_MODELS,
        supports_model_fetch: false,
        anthropic_base_url: None,
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: ApiTransport::ChatCompletions,
        native_transport_kind: None,
        native_base_url: None,
        supports_response_continuity: false,
    },
    ProviderDefinition {
        id: PROVIDER_ID_ARCEE,
        name: "Arcee",
        default_base_url: "https://api.arcee.ai/api/v1",
        default_model: "trinity-large-thinking",
        api_type: ApiType::OpenAI,
        auth_method: AuthMethod::Bearer,
        models: ARCEE_MODELS,
        supports_model_fetch: true,
        anthropic_base_url: None,
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: ApiTransport::ChatCompletions,
        native_transport_kind: None,
        native_base_url: None,
        supports_response_continuity: false,
    },
    ProviderDefinition {
        id: PROVIDER_ID_OPENROUTER,
        name: "OpenRouter",
        default_base_url: "https://openrouter.ai/api/v1",
        default_model: "arcee-ai/trinity-large-thinking",
        api_type: ApiType::OpenAI,
        auth_method: AuthMethod::Bearer,
        models: OPENROUTER_MODELS,
        supports_model_fetch: true,
        anthropic_base_url: None,
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: ApiTransport::ChatCompletions,
        native_transport_kind: None,
        native_base_url: None,
        supports_response_continuity: false,
    },
    ProviderDefinition {
        id: PROVIDER_ID_CEREBRAS,
        name: "Cerebras",
        default_base_url: "https://api.cerebras.ai/v1",
        default_model: "llama-3.3-70b",
        api_type: ApiType::OpenAI,
        auth_method: AuthMethod::Bearer,
        models: CEREBRAS_MODELS,
        supports_model_fetch: true,
        anthropic_base_url: None,
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: ApiTransport::ChatCompletions,
        native_transport_kind: None,
        native_base_url: None,
        supports_response_continuity: false,
    },
    ProviderDefinition {
        id: PROVIDER_ID_TOGETHER,
        name: "Together",
        default_base_url: "https://api.together.xyz/v1",
        default_model: "meta-llama/Llama-3.3-70B-Instruct-Turbo",
        api_type: ApiType::OpenAI,
        auth_method: AuthMethod::Bearer,
        models: TOGETHER_MODELS,
        supports_model_fetch: true,
        anthropic_base_url: None,
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: ApiTransport::ChatCompletions,
        native_transport_kind: None,
        native_base_url: None,
        supports_response_continuity: false,
    },
    ProviderDefinition {
        id: PROVIDER_ID_GROQ,
        name: "Groq",
        default_base_url: "https://api.groq.com/openai/v1",
        default_model: "llama-3.3-70b-versatile",
        api_type: ApiType::OpenAI,
        auth_method: AuthMethod::Bearer,
        models: GROQ_MODELS,
        supports_model_fetch: true,
        anthropic_base_url: None,
        supported_transports: RESPONSES_AND_CHAT_TRANSPORTS,
        default_transport: ApiTransport::Responses,
        native_transport_kind: None,
        native_base_url: None,
        supports_response_continuity: false,
    },
    ProviderDefinition {
        id: PROVIDER_ID_OLLAMA,
        name: "Ollama",
        default_base_url: "http://localhost:11434/v1",
        default_model: "llama3.1",
        api_type: ApiType::OpenAI,
        auth_method: AuthMethod::Bearer,
        models: OLLAMA_MODELS,
        supports_model_fetch: true,
        anthropic_base_url: None,
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: ApiTransport::ChatCompletions,
        native_transport_kind: None,
        native_base_url: None,
        supports_response_continuity: false,
    },
    ProviderDefinition {
        id: PROVIDER_ID_CHUTES,
        name: "Chutes",
        default_base_url: "https://llm.chutes.ai/v1",
        default_model: "deepseek-ai/DeepSeek-R1",
        api_type: ApiType::OpenAI,
        auth_method: AuthMethod::Bearer,
        models: CHUTES_MODELS,
        supports_model_fetch: true,
        anthropic_base_url: None,
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: ApiTransport::ChatCompletions,
        native_transport_kind: None,
        native_base_url: None,
        supports_response_continuity: false,
    },
    ProviderDefinition {
        id: PROVIDER_ID_HUGGINGFACE,
        name: "Hugging Face",
        default_base_url: "https://router.huggingface.co/v1",
        default_model: "meta-llama/Llama-3.3-70B-Instruct",
        api_type: ApiType::OpenAI,
        auth_method: AuthMethod::Bearer,
        models: HUGGINGFACE_MODELS,
        supports_model_fetch: true,
        anthropic_base_url: None,
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: ApiTransport::ChatCompletions,
        native_transport_kind: None,
        native_base_url: None,
        supports_response_continuity: false,
    },
    ProviderDefinition {
        id: PROVIDER_ID_MINIMAX,
        name: "MiniMax",
        default_base_url: "https://api.minimax.io/anthropic",
        default_model: "MiniMax-M3",
        api_type: ApiType::Anthropic,
        auth_method: AuthMethod::XApiKey,
        models: MINIMAX_MODELS,
        supports_model_fetch: true,
        anthropic_base_url: None,
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: ApiTransport::ChatCompletions,
        native_transport_kind: None,
        native_base_url: None,
        supports_response_continuity: false,
    },
    ProviderDefinition {
        id: PROVIDER_ID_MINIMAX_CODING_PLAN,
        name: "MiniMax Coding Plan",
        default_base_url: "https://api.minimax.io/anthropic",
        default_model: "MiniMax-M3",
        api_type: ApiType::Anthropic,
        auth_method: AuthMethod::XApiKey,
        models: MINIMAX_MODELS,
        supports_model_fetch: true,
        anthropic_base_url: None,
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: ApiTransport::ChatCompletions,
        native_transport_kind: None,
        native_base_url: None,
        supports_response_continuity: false,
    },
    ProviderDefinition {
        id: PROVIDER_ID_ALIBABA_CODING_PLAN,
        name: "Alibaba Coding Plan",
        default_base_url: "https://coding-intl.dashscope.aliyuncs.com/v1",
        default_model: "qwen3.6-plus",
        api_type: ApiType::OpenAI,
        auth_method: AuthMethod::Bearer,
        models: ALIBABA_CODING_MODELS,
        supports_model_fetch: false,
        anthropic_base_url: None,
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: ApiTransport::ChatCompletions,
        native_transport_kind: None,
        native_base_url: None,
        supports_response_continuity: false,
    },
    ProviderDefinition {
        id: PROVIDER_ID_ALIBABA_TOKEN_PLAN,
        name: "Alibaba Token Plan",
        default_base_url: "https://token-plan.ap-southeast-1.maas.aliyuncs.com/compatible-mode/v1",
        default_model: "qwen3.7-max",
        api_type: ApiType::OpenAI,
        auth_method: AuthMethod::Bearer,
        models: ALIBABA_TOKEN_MODELS,
        supports_model_fetch: false,
        anthropic_base_url: None,
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: ApiTransport::ChatCompletions,
        native_transport_kind: None,
        native_base_url: None,
        supports_response_continuity: false,
    },
    ProviderDefinition {
        id: PROVIDER_ID_XIAOMI_MIMO,
        name: "Xiaomi MiMo",
        default_base_url: "https://api.xiaomimimo.com/v1",
        default_model: "mimo-v2.5-pro",
        api_type: ApiType::OpenAI,
        auth_method: AuthMethod::Bearer,
        models: XIAOMI_MIMO_MODELS,
        supports_model_fetch: true,
        anthropic_base_url: Some("https://api.xiaomimimo.com/anthropic"),
        supported_transports: &[
            ApiTransport::ChatCompletions,
            ApiTransport::AnthropicMessages,
        ],
        default_transport: ApiTransport::ChatCompletions,
        native_transport_kind: None,
        native_base_url: None,
        supports_response_continuity: false,
    },
    ProviderDefinition {
        id: PROVIDER_ID_XIAOMI_MIMO_TOKEN_PLAN,
        name: "Xiaomi MiMo Token Plan",
        default_base_url: "https://token-plan-ams.xiaomimimo.com/v1",
        default_model: "mimo-v2.5-pro",
        api_type: ApiType::OpenAI,
        auth_method: AuthMethod::Bearer,
        models: XIAOMI_MIMO_MODELS,
        supports_model_fetch: true,
        anthropic_base_url: Some("https://token-plan-ams.xiaomimimo.com/anthropic"),
        supported_transports: &[
            ApiTransport::ChatCompletions,
            ApiTransport::AnthropicMessages,
        ],
        default_transport: ApiTransport::ChatCompletions,
        native_transport_kind: None,
        native_base_url: None,
        supports_response_continuity: false,
    },
    ProviderDefinition {
        id: PROVIDER_ID_NOUS_PORTAL,
        name: "Nous Portal",
        default_base_url: "https://inference-api.nousresearch.com/v1",
        default_model: "nousresearch/hermes-4-70b",
        api_type: ApiType::OpenAI,
        auth_method: AuthMethod::Bearer,
        models: NOUS_PORTAL_MODELS,
        supports_model_fetch: true,
        anthropic_base_url: None,
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: ApiTransport::ChatCompletions,
        native_transport_kind: None,
        native_base_url: None,
        supports_response_continuity: false,
    },
    ProviderDefinition {
        id: PROVIDER_ID_HERMES_AGENT_API,
        name: "Hermes Agent API",
        default_base_url: "http://localhost:8642/v1",
        default_model: "hermes-agent",
        api_type: ApiType::OpenAI,
        auth_method: AuthMethod::Bearer,
        models: HERMES_AGENT_API_MODELS,
        supports_model_fetch: true,
        anthropic_base_url: None,
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: ApiTransport::ChatCompletions,
        native_transport_kind: None,
        native_base_url: None,
        supports_response_continuity: false,
    },
    ProviderDefinition {
        id: PROVIDER_ID_OPENCODE_ZEN,
        name: "OpenCode Zen",
        default_base_url: "https://opencode.ai/zen/v1",
        default_model: "claude-sonnet-4-6",
        api_type: ApiType::Anthropic,
        auth_method: AuthMethod::Bearer,
        models: OPENCODE_ZEN_MODELS,
        supports_model_fetch: true,
        anthropic_base_url: None,
        supported_transports: CHAT_AND_ANTHROPIC_TRANSPORTS,
        default_transport: ApiTransport::ChatCompletions,
        native_transport_kind: None,
        native_base_url: None,
        supports_response_continuity: false,
    },
    ProviderDefinition {
        id: PROVIDER_ID_OPENCODE_GO,
        name: "OpenCode Go",
        default_base_url: "https://opencode.ai/zen/go/v1",
        default_model: "glm-5.1",
        api_type: ApiType::Anthropic,
        auth_method: AuthMethod::Bearer,
        models: OPENCODE_GO_MODELS,
        supports_model_fetch: true,
        anthropic_base_url: None,
        supported_transports: CHAT_AND_ANTHROPIC_TRANSPORTS,
        default_transport: ApiTransport::ChatCompletions,
        native_transport_kind: None,
        native_base_url: None,
        supports_response_continuity: false,
    },
    ProviderDefinition {
        id: PROVIDER_ID_CLAUDE_CODE_CLI,
        name: "Claude Code (Local CLI)",
        default_base_url: "",
        default_model: "opus",
        api_type: ApiType::OpenAI,
        auth_method: AuthMethod::Bearer,
        models: CLAUDE_CODE_MODELS,
        supports_model_fetch: false,
        anthropic_base_url: None,
        supported_transports: &[ApiTransport::NativeAssistant],
        default_transport: ApiTransport::NativeAssistant,
        native_transport_kind: Some(NativeTransportKind::ClaudeCodeCli),
        native_base_url: None,
        supports_response_continuity: false,
    },
    ProviderDefinition {
        id: PROVIDER_ID_CURSOR,
        name: "Cursor",
        default_base_url: "",
        default_model: "auto",
        api_type: ApiType::OpenAI,
        auth_method: AuthMethod::Bearer,
        models: CURSOR_MODELS,
        supports_model_fetch: false,
        anthropic_base_url: None,
        supported_transports: &[ApiTransport::NativeAssistant],
        default_transport: ApiTransport::NativeAssistant,
        native_transport_kind: Some(NativeTransportKind::CursorCli),
        native_base_url: None,
        supports_response_continuity: false,
    },
    ProviderDefinition {
        id: PROVIDER_ID_CUSTOM,
        name: "Custom",
        default_base_url: "",
        default_model: "",
        api_type: ApiType::OpenAI,
        auth_method: AuthMethod::Bearer,
        models: EMPTY_MODELS,
        supports_model_fetch: false,
        anthropic_base_url: None,
        supported_transports: RESPONSES_AND_CHAT_TRANSPORTS,
        default_transport: ApiTransport::Responses,
        native_transport_kind: None,
        native_base_url: None,
        supports_response_continuity: true,
    },
];

pub fn get_provider_definition(id: &str) -> Option<&'static ProviderDefinition> {
    custom_provider_definition(id).or_else(|| PROVIDER_DEFINITIONS.iter().find(|p| p.id == id))
}

pub fn all_provider_definitions() -> Vec<&'static ProviderDefinition> {
    let mut definitions = PROVIDER_DEFINITIONS.iter().collect::<Vec<_>>();
    definitions.extend(custom_provider_definitions());
    definitions
}

pub fn provider_supports_transport(provider_id: &str, transport: ApiTransport) -> bool {
    get_provider_definition(provider_id)
        .map(|definition| definition.supported_transports.contains(&transport))
        .unwrap_or(matches!(
            transport,
            ApiTransport::Responses
                | ApiTransport::AnthropicMessages
                | ApiTransport::ChatCompletions
        ))
}

pub fn default_api_transport_for_provider(provider_id: &str) -> ApiTransport {
    get_provider_definition(provider_id)
        .map(|definition| definition.default_transport)
        .unwrap_or(ApiTransport::ChatCompletions)
}

pub fn fixed_api_transport_for_model(provider_id: &str, model: &str) -> Option<ApiTransport> {
    match zorai_shared::providers::fixed_api_transport_for_model(provider_id, model) {
        Some("native_assistant") => Some(ApiTransport::NativeAssistant),
        Some("responses") => Some(ApiTransport::Responses),
        Some("anthropic_messages") => Some(ApiTransport::AnthropicMessages),
        Some("chat_completions") => Some(ApiTransport::ChatCompletions),
        Some(_) | None => None,
    }
}

pub fn provider_uses_configurable_base_url(provider_id: &str) -> bool {
    matches!(provider_id, PROVIDER_ID_CUSTOM | PROVIDER_ID_AZURE_OPENAI)
}

/// Local/runtime catalogs (Ollama, LM Studio, Hugging Face) are fetched live.
/// The static model list is a starter set, not an allowlist.
pub fn provider_allows_unlisted_models(provider_id: &str) -> bool {
    matches!(
        provider_id,
        PROVIDER_ID_OLLAMA
            | PROVIDER_ID_LMSTUDIO
            | PROVIDER_ID_HUGGINGFACE
            | PROVIDER_ID_CUSTOM
            | PROVIDER_ID_CURSOR
    )
}

pub fn provider_uses_local_subscription_cli(provider_id: &str) -> bool {
    matches!(
        provider_id,
        PROVIDER_ID_CLAUDE_CODE_CLI | PROVIDER_ID_CURSOR
    )
}

pub fn provider_base_url_is_customized(provider_id: &str, base_url: &str) -> bool {
    let trimmed = base_url.trim();
    if trimmed.is_empty() {
        return false;
    }
    match get_provider_definition(provider_id) {
        Some(def) => {
            trimmed != def.default_base_url
                && def.anthropic_base_url != Some(trimmed)
                && def.native_base_url != Some(trimmed)
        }
        None => true,
    }
}

pub fn supports_response_continuity(provider_id: &str) -> bool {
    get_provider_definition(provider_id)
        .map(|definition| definition.supports_response_continuity)
        .unwrap_or(false)
}

fn is_alibaba_coding_plan_anthropic_url(base_url: &str) -> bool {
    let lower = base_url.trim().to_ascii_lowercase();
    lower.contains("dashscope.aliyuncs.com") && lower.contains("/apps/anthropic")
}

fn is_alibaba_token_plan_anthropic_url(base_url: &str) -> bool {
    let lower = base_url.trim().to_ascii_lowercase();
    lower.contains("maas.aliyuncs.com") && lower.contains("/apps/anthropic")
}

fn is_xiaomi_mimo_token_plan_anthropic_url(base_url: &str) -> bool {
    let lower = base_url.trim().to_ascii_lowercase();
    lower == "https://token-plan-ams.xiaomimimo.com/anthropic"
        || lower == "http://token-plan-ams.xiaomimimo.com/anthropic"
        || lower.starts_with("https://token-plan-ams.xiaomimimo.com/anthropic/")
        || lower.starts_with("http://token-plan-ams.xiaomimimo.com/anthropic/")
}

fn is_xiaomi_mimo_anthropic_url(base_url: &str) -> bool {
    let lower = base_url.trim().to_ascii_lowercase();
    lower == "https://api.xiaomimimo.com/anthropic"
        || lower == "http://api.xiaomimimo.com/anthropic"
        || lower.starts_with("https://api.xiaomimimo.com/anthropic/")
        || lower.starts_with("http://api.xiaomimimo.com/anthropic/")
}

fn is_direct_anthropic_url(base_url: &str) -> bool {
    let lower = base_url.trim().to_ascii_lowercase();
    lower == "https://api.anthropic.com"
        || lower == "http://api.anthropic.com"
        || lower.starts_with("https://api.anthropic.com/")
        || lower.starts_with("http://api.anthropic.com/")
}

pub fn get_provider_api_type(provider_id: &str, model: &str, configured_url: &str) -> ApiType {
    if provider_id == PROVIDER_ID_ANTHROPIC
        || (model.starts_with("claude") && is_direct_anthropic_url(configured_url))
    {
        return ApiType::Anthropic;
    }

    if provider_id == PROVIDER_ID_ALIBABA_CODING_PLAN
        && is_alibaba_coding_plan_anthropic_url(configured_url)
    {
        return ApiType::Anthropic;
    }

    if provider_id == PROVIDER_ID_ALIBABA_TOKEN_PLAN
        && is_alibaba_token_plan_anthropic_url(configured_url)
    {
        return ApiType::Anthropic;
    }

    if provider_id == PROVIDER_ID_XIAOMI_MIMO_TOKEN_PLAN
        && is_xiaomi_mimo_token_plan_anthropic_url(configured_url)
    {
        return ApiType::Anthropic;
    }

    if provider_id == PROVIDER_ID_XIAOMI_MIMO && is_xiaomi_mimo_anthropic_url(configured_url) {
        return ApiType::Anthropic;
    }

    let def = get_provider_definition(provider_id);

    match def {
        Some(d) => {
            if d.anthropic_base_url.is_some() {
                if model.starts_with("claude") {
                    ApiType::Anthropic
                } else {
                    ApiType::OpenAI
                }
            } else if (provider_id == PROVIDER_ID_OPENCODE_ZEN
                || provider_id == PROVIDER_ID_OPENCODE_GO)
                && !model.starts_with("claude")
            {
                ApiType::OpenAI
            } else {
                d.api_type
            }
        }
        None => ApiType::OpenAI,
    }
}

pub fn get_provider_base_url(provider_id: &str, model: &str, configured_url: &str) -> String {
    if !configured_url.is_empty() {
        return configured_url.to_string();
    }

    let def = get_provider_definition(provider_id);

    match def {
        Some(d) => {
            if provider_id == PROVIDER_ID_XIAOMI_MIMO_TOKEN_PLAN
                && matches!(d.anthropic_base_url, Some(url) if configured_url == url)
            {
                d.anthropic_base_url.unwrap().to_string()
            } else if d.anthropic_base_url.is_some() && model.starts_with("claude") {
                d.anthropic_base_url.unwrap().to_string()
            } else {
                d.default_base_url.to_string()
            }
        }
        None => configured_url.to_string(),
    }
}
