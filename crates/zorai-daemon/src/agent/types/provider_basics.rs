use super::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiType {
    #[serde(rename = "openai", alias = "open_ai")]
    OpenAI,
    Anthropic,
}

impl ApiType {
    /// The client User-Agent string used by coding-plan providers.
    pub fn sdk_user_agent(self) -> &'static str {
        "zorai/1.0.0"
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ApiTransport {
    NativeAssistant,
    #[default]
    Responses,
    AnthropicMessages,
    ChatCompletions,
}

impl ApiTransport {
    /// The snake_case wire/storage string for this transport (matches serde).
    pub fn as_snake_str(self) -> &'static str {
        match self {
            Self::NativeAssistant => "native_assistant",
            Self::Responses => "responses",
            Self::AnthropicMessages => "anthropic_messages",
            Self::ChatCompletions => "chat_completions",
        }
    }

    /// Parse a snake_case transport string (DB/config), returning `None` if unknown.
    pub fn from_snake_str(value: &str) -> Option<Self> {
        match value.trim() {
            "native_assistant" => Some(Self::NativeAssistant),
            "responses" => Some(Self::Responses),
            "anthropic_messages" => Some(Self::AnthropicMessages),
            "chat_completions" => Some(Self::ChatCompletions),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AuthSource {
    #[default]
    ApiKey,
    ChatgptSubscription,
    GithubCopilot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeTransportKind {
    AlibabaAssistantApi,
    ClaudeCodeCli,
    CursorCli,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthMethod {
    Bearer,
    #[serde(rename = "x-api-key", alias = "x_api_key")]
    XApiKey,
    #[serde(rename = "xi-api-key", alias = "xi_api_key")]
    XiApiKey,
}

impl AuthMethod {
    /// Apply the appropriate auth header to a request builder.
    pub fn apply(self, req: reqwest::RequestBuilder, api_key: &str) -> reqwest::RequestBuilder {
        match self {
            Self::Bearer => req.header("Authorization", format!("Bearer {}", api_key)),
            Self::XApiKey => req.header("x-api-key", api_key),
            Self::XiApiKey => req.header("xi-api-key", api_key),
        }
    }
}

/// Input modalities a model supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Modality {
    Text,
    Image,
    Video,
    Audio,
    Embedding,
}

/// Shorthand constants for common modality sets.
pub const TEXT_ONLY: &[Modality] = &[Modality::Text];
pub const EMBEDDING_ONLY: &[Modality] = &[Modality::Embedding];
pub const TEXT_AUDIO: &[Modality] = &[Modality::Text, Modality::Audio];
pub const TEXT_IMAGE: &[Modality] = &[Modality::Text, Modality::Image];
pub const TEXT_IMAGE_AUDIO: &[Modality] = &[Modality::Text, Modality::Image, Modality::Audio];
pub const MULTIMODAL: &[Modality] = &[
    Modality::Text,
    Modality::Image,
    Modality::Video,
    Modality::Audio,
];

pub const XAI_MODELS: &[ModelDefinition] = &[
    ModelDefinition {
        id: "grok-4.6",
        name: "Grok 4.6",
        context_window: 500_000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "grok-4.5",
        name: "Grok 4.5",
        context_window: 500_000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "grok-4.3",
        name: "Grok 4.3",
        context_window: 1_000_000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "grok-build-0.1",
        name: "Grok Build 0.1",
        context_window: 256_000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "grok-4",
        name: "Grok 4",
        context_window: 256_000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "grok-code-fast-1",
        name: "Grok Code Fast 1",
        context_window: 256_000,
        modalities: TEXT_ONLY,
    },
];

pub const CLAUDE_CODE_MODELS: &[ModelDefinition] = &[
    ModelDefinition {
        id: "opus",
        name: "Claude Opus (CLI default)",
        context_window: 1_000_000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "sonnet",
        name: "Claude Sonnet (CLI)",
        context_window: 200_000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "haiku",
        name: "Claude Haiku (CLI)",
        context_window: 200_000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "fable",
        name: "Fable (CLI)",
        context_window: 1_000_000,
        modalities: TEXT_IMAGE,
    },
];

pub const CURSOR_MODELS: &[ModelDefinition] = &[
    ModelDefinition {
        id: "auto",
        name: "Auto",
        context_window: 200_000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "composer-2.5",
        name: "Composer 2.5",
        context_window: 200_000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "grok-4.7",
        name: "Grok 4.7",
        context_window: 200_000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "grok-4.6",
        name: "Grok 4.6",
        context_window: 200_000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "grok-4.5",
        name: "Grok 4.5",
        context_window: 200_000,
        modalities: TEXT_IMAGE,
    },
];

pub const DEEPSEEK_MODELS: &[ModelDefinition] = &[
    ModelDefinition {
        id: "deepseek-v4-pro",
        name: "DeepSeek V4 Pro",
        context_window: 1_048_576,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "deepseek-v4-flash",
        name: "DeepSeek V4 Flash",
        context_window: 1_048_576,
        modalities: TEXT_ONLY,
    },
];

pub const GITHUB_COPILOT_MODELS: &[ModelDefinition] = &[
    ModelDefinition {
        id: "gpt-5.4",
        name: "GPT-5.4",
        context_window: 400_000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "gpt-5.5",
        name: "GPT-5.5",
        context_window: 400_000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "claude-haiku-4.5",
        name: "Claude Haiku 4.5",
        context_window: 160_000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "claude-opus-4.5",
        name: "Claude Opus 4.5",
        context_window: 160_000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "claude-opus-4.6",
        name: "Claude Opus 4.6",
        context_window: 192_000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "claude-opus-4.6-fast",
        name: "Claude Opus 4.6 (fast mode) (Preview)",
        context_window: 192_000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "claude-opus-4.7",
        name: "Claude Opus 4.7",
        context_window: 192_000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "claude-sonnet-4",
        name: "Claude Sonnet 4",
        context_window: 144_000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "claude-sonnet-4.5",
        name: "Claude Sonnet 4.5",
        context_window: 160_000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "claude-sonnet-4.6",
        name: "Claude Sonnet 4.6",
        context_window: 160_000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "gemini-2.5-pro",
        name: "Gemini 2.5 Pro",
        context_window: 173_000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "gemini-3-flash-preview",
        name: "Gemini 3 Flash (Preview)",
        context_window: 173_000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "gemini-3.1-pro-preview",
        name: "Gemini 3.1 Pro (Preview)",
        context_window: 173_000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "gpt-4.1",
        name: "GPT-4.1",
        context_window: 128_000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "gpt-4o",
        name: "GPT-4o",
        context_window: 128_000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "gpt-5-mini",
        name: "GPT-5 mini",
        context_window: 192_000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "gpt-5.1",
        name: "GPT-5.1",
        context_window: 192_000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "gpt-5.1-codex",
        name: "GPT-5.1-Codex",
        context_window: 256_000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "gpt-5.1-codex-max",
        name: "GPT-5.1-Codex-Max",
        context_window: 256_000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "gpt-5.1-codex-mini",
        name: "GPT-5.1-Codex-Mini (Preview)",
        context_window: 256_000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "gpt-5.2",
        name: "GPT-5.2",
        context_window: 192_000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "gpt-5.2-codex",
        name: "GPT-5.2-Codex",
        context_window: 400_000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "gpt-5.3-codex",
        name: "GPT-5.3-Codex",
        context_window: 400_000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "gpt-5.4-mini",
        name: "GPT-5.4 mini",
        context_window: 400_000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "grok-code-fast-1",
        name: "Grok Code Fast 1",
        context_window: 173_000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "raptor-mini",
        name: "Raptor mini (Preview)",
        context_window: 264_000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "goldeneye",
        name: "Goldeneye",
        context_window: 524_000,
        modalities: TEXT_IMAGE,
    },
];

/// Look up the modalities for a model by provider and model ID.
/// Returns TEXT_ONLY if the model is not in the known list.
pub fn model_modalities(provider_id: &str, model_id: &str) -> &'static [Modality] {
    get_provider_definition(provider_id)
        .and_then(|def| def.models.iter().find(|m| m.id == model_id))
        .map(|m| m.modalities)
        .unwrap_or(TEXT_ONLY)
}

/// Check if a model supports a specific modality.
pub fn model_supports(provider_id: &str, model_id: &str, modality: Modality) -> bool {
    model_modalities(provider_id, model_id).contains(&modality)
}

/// Return model names from the same provider that support the given modality.
pub fn models_supporting(provider_id: &str, modality: Modality) -> Vec<&'static str> {
    get_provider_definition(provider_id)
        .map(|def| {
            def.models
                .iter()
                .filter(|m| m.modalities.contains(&modality))
                .map(|m| m.id)
                .collect()
        })
        .unwrap_or_default()
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelDefinition {
    pub id: &'static str,
    pub name: &'static str,
    pub context_window: u32,
    /// Input modalities this model accepts.
    pub modalities: &'static [Modality],
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderDefinition {
    pub id: &'static str,
    pub name: &'static str,
    pub default_base_url: &'static str,
    pub default_model: &'static str,
    pub api_type: ApiType,
    pub auth_method: AuthMethod,
    pub models: &'static [ModelDefinition],
    pub supports_model_fetch: bool,
    pub anthropic_base_url: Option<&'static str>,
    pub supported_transports: &'static [ApiTransport],
    pub default_transport: ApiTransport,
    pub native_transport_kind: Option<NativeTransportKind>,
    pub native_base_url: Option<&'static str>,
    pub supports_response_continuity: bool,
}
