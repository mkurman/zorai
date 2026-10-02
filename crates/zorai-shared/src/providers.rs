#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProviderRef {
    pub id: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AudioToolKind {
    SpeechToText,
    TextToSpeech,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ModelFeatureCapabilities {
    pub vision: bool,
    pub stt: bool,
    pub tts: bool,
    pub image_generation: bool,
}

pub const PROVIDER_ID_ALIBABA_CODING_PLAN: &str = "alibaba-coding-plan";
pub const PROVIDER_ID_ALIBABA_TOKEN_PLAN: &str = "alibaba-token-plan";
pub const PROVIDER_ID_ANTHROPIC: &str = "anthropic";
pub const PROVIDER_ID_ARCEE: &str = "arcee";
pub const PROVIDER_ID_AZURE_OPENAI: &str = "azure-openai";
pub const PROVIDER_ID_CEREBRAS: &str = "cerebras";
pub const PROVIDER_ID_CHUTES: &str = "chutes";
pub const PROVIDER_ID_CLAUDE_CODE_CLI: &str = "claude-code-cli";
pub const PROVIDER_ID_CURSOR: &str = "cursor";
pub const PROVIDER_ID_CUSTOM: &str = "custom";
pub const PROVIDER_ID_DEEPSEEK: &str = "deepseek";
pub const PROVIDER_ID_ELEVENLABS: &str = "elevenlabs";
pub const PROVIDER_ID_FEATHERLESS: &str = "featherless";
pub const PROVIDER_ID_GITHUB_COPILOT: &str = "github-copilot";
pub const PROVIDER_ID_GROQ: &str = "groq";
pub const PROVIDER_ID_HERMES_AGENT_API: &str = "hermes-agent-api";
pub const PROVIDER_ID_HUGGINGFACE: &str = "huggingface";
pub const PROVIDER_ID_KIMI: &str = "kimi";
pub const PROVIDER_ID_KIMI_CODING_PLAN: &str = "kimi-coding-plan";
pub const PROVIDER_ID_LMSTUDIO: &str = "lmstudio";
pub const PROVIDER_ID_MINIMAX: &str = "minimax";
pub const PROVIDER_ID_MINIMAX_CODING_PLAN: &str = "minimax-coding-plan";
pub const PROVIDER_ID_NVIDIA: &str = "nvidia";
pub const PROVIDER_ID_NOUS_PORTAL: &str = "nous-portal";
pub const PROVIDER_ID_OLLAMA: &str = "ollama";
pub const PROVIDER_ID_OPENAI: &str = "openai";
pub const PROVIDER_ID_CHATGPT_SUBSCRIPTION: &str = "chatgpt_subscription";
pub const PROVIDER_ID_OPENCODE_ZEN: &str = "opencode-zen";
pub const PROVIDER_ID_OPENCODE_GO: &str = "opencode-go";
pub const PROVIDER_ID_OPENROUTER: &str = "openrouter";
pub const PROVIDER_ID_POOLSIDE: &str = "poolside";
pub const PROVIDER_ID_QWEN: &str = "qwen";
pub const PROVIDER_ID_QWEN_DEEPINFRA: &str = "qwen-deepinfra";
pub const PROVIDER_ID_TOGETHER: &str = "together";
pub const PROVIDER_ID_XIAOMI_MIMO: &str = "xiaomi-mimo";
pub const PROVIDER_ID_XIAOMI_MIMO_TOKEN_PLAN: &str = "xiaomi-mimo-token-plan";
pub const PROVIDER_ID_XAI: &str = "xai";
pub const PROVIDER_ID_Z_AI: &str = "z.ai";
pub const PROVIDER_ID_Z_AI_CODING_PLAN: &str = "z.ai-coding-plan";

pub const ALIBABA_CODING_PLAN_PROVIDER: ProviderRef = ProviderRef {
    id: PROVIDER_ID_ALIBABA_CODING_PLAN,
};
pub const ALIBABA_TOKEN_PLAN_PROVIDER: ProviderRef = ProviderRef {
    id: PROVIDER_ID_ALIBABA_TOKEN_PLAN,
};
pub const ANTHROPIC_PROVIDER: ProviderRef = ProviderRef {
    id: PROVIDER_ID_ANTHROPIC,
};
pub const AZURE_OPENAI_PROVIDER: ProviderRef = ProviderRef {
    id: PROVIDER_ID_AZURE_OPENAI,
};
pub const CUSTOM_PROVIDER: ProviderRef = ProviderRef {
    id: PROVIDER_ID_CUSTOM,
};
pub const DEEPSEEK_PROVIDER: ProviderRef = ProviderRef {
    id: PROVIDER_ID_DEEPSEEK,
};
pub const ELEVENLABS_PROVIDER: ProviderRef = ProviderRef {
    id: PROVIDER_ID_ELEVENLABS,
};
pub const GITHUB_COPILOT_PROVIDER: ProviderRef = ProviderRef {
    id: PROVIDER_ID_GITHUB_COPILOT,
};
pub const HERMES_AGENT_API_PROVIDER: ProviderRef = ProviderRef {
    id: PROVIDER_ID_HERMES_AGENT_API,
};
pub const MINIMAX_PROVIDER: ProviderRef = ProviderRef {
    id: PROVIDER_ID_MINIMAX,
};
pub const MINIMAX_CODING_PLAN_PROVIDER: ProviderRef = ProviderRef {
    id: PROVIDER_ID_MINIMAX_CODING_PLAN,
};
pub const NVIDIA_PROVIDER: ProviderRef = ProviderRef {
    id: PROVIDER_ID_NVIDIA,
};
pub const NOUS_PORTAL_PROVIDER: ProviderRef = ProviderRef {
    id: PROVIDER_ID_NOUS_PORTAL,
};
pub const OPENAI_PROVIDER: ProviderRef = ProviderRef {
    id: PROVIDER_ID_OPENAI,
};
pub const OPENCODE_ZEN_PROVIDER: ProviderRef = ProviderRef {
    id: PROVIDER_ID_OPENCODE_ZEN,
};
pub const OPENROUTER_PROVIDER: ProviderRef = ProviderRef {
    id: PROVIDER_ID_OPENROUTER,
};
pub const POOLSIDE_PROVIDER: ProviderRef = ProviderRef {
    id: PROVIDER_ID_POOLSIDE,
};
pub const QWEN_PROVIDER: ProviderRef = ProviderRef {
    id: PROVIDER_ID_QWEN,
};
pub const XIAOMI_MIMO_PROVIDER: ProviderRef = ProviderRef {
    id: PROVIDER_ID_XIAOMI_MIMO,
};
pub const XIAOMI_MIMO_TOKEN_PLAN_PROVIDER: ProviderRef = ProviderRef {
    id: PROVIDER_ID_XIAOMI_MIMO_TOKEN_PLAN,
};

fn metadata_pointer<'a>(
    metadata: Option<&'a serde_json::Value>,
    pointer: &str,
) -> Option<&'a serde_json::Value> {
    metadata
        .and_then(|value| value.pointer(&format!("/architecture{pointer}")))
        .or_else(|| metadata.and_then(|value| value.pointer(pointer)))
}

fn json_array_contains(value: Option<&serde_json::Value>, needle: &str) -> bool {
    value
        .and_then(|value| value.as_array())
        .map(|items| {
            items.iter().any(|item| {
                item.as_str()
                    .map(str::trim)
                    .map(|value| value.eq_ignore_ascii_case(needle))
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

fn modality_side_has_term(value: Option<&serde_json::Value>, side: &str, needle: &str) -> bool {
    let Some(value) = value.and_then(|value| value.as_str()) else {
        return false;
    };
    let normalized = value.trim().to_ascii_lowercase();
    let Some((input, output)) = normalized.split_once("->") else {
        return false;
    };
    let directional = match side {
        "input" => input,
        "output" => output,
        _ => return false,
    };

    directional
        .split('+')
        .map(str::trim)
        .any(|token| token == needle)
}

fn model_id_has_image_generation_heuristic(model_id: &str) -> bool {
    let normalized = model_id.trim().to_ascii_lowercase();
    !normalized.is_empty()
        && [
            "gpt-image",
            "dall-e",
            "imagen",
            "recraft",
            "stable-diffusion",
            "sdxl",
            "flux",
        ]
        .iter()
        .any(|needle| normalized.contains(needle))
}

pub fn derive_model_feature_capabilities(
    _provider_id: &str,
    model_id: &str,
    metadata: Option<&serde_json::Value>,
    pricing_image: bool,
) -> ModelFeatureCapabilities {
    let input_modalities = metadata_pointer(metadata, "/input_modalities");
    let output_modalities = metadata_pointer(metadata, "/output_modalities");
    let modality = metadata_pointer(metadata, "/modality");

    let vision = json_array_contains(input_modalities, "image")
        || modality_side_has_term(modality, "input", "image");
    let stt = json_array_contains(input_modalities, "audio")
        || modality_side_has_term(modality, "input", "audio");
    let tts = json_array_contains(output_modalities, "audio")
        || modality_side_has_term(modality, "output", "audio");
    let image_generation = json_array_contains(output_modalities, "image")
        || modality_side_has_term(modality, "output", "image")
        || pricing_image
        || model_id_has_image_generation_heuristic(model_id);

    ModelFeatureCapabilities {
        vision,
        stt,
        tts,
        image_generation,
    }
}

pub fn fixed_api_transport_for_model(provider_id: &str, model_id: &str) -> Option<&'static str> {
    let model_id = model_id.trim();
    if model_id.is_empty() {
        return None;
    }

    match (provider_id, model_id) {
        (PROVIDER_ID_GITHUB_COPILOT, "gemini-3.1-pro-preview") => Some("chat_completions"),
        _ => None,
    }
}

pub fn provider_supports_audio_tool(provider_id: &str, kind: AudioToolKind) -> bool {
    match kind {
        AudioToolKind::SpeechToText => matches!(
            provider_id,
            PROVIDER_ID_CUSTOM
                | PROVIDER_ID_OPENAI
                | PROVIDER_ID_AZURE_OPENAI
                | PROVIDER_ID_ELEVENLABS
                | PROVIDER_ID_GROQ
                | PROVIDER_ID_OPENROUTER
                | PROVIDER_ID_XAI
                | PROVIDER_ID_XIAOMI_MIMO
                | PROVIDER_ID_XIAOMI_MIMO_TOKEN_PLAN
        ),
        AudioToolKind::TextToSpeech => matches!(
            provider_id,
            PROVIDER_ID_CUSTOM
                | PROVIDER_ID_OPENAI
                | PROVIDER_ID_AZURE_OPENAI
                | PROVIDER_ID_ELEVENLABS
                | PROVIDER_ID_GROQ
                | PROVIDER_ID_MINIMAX
                | PROVIDER_ID_MINIMAX_CODING_PLAN
                | PROVIDER_ID_OPENROUTER
                | PROVIDER_ID_XAI
                | PROVIDER_ID_XIAOMI_MIMO
                | PROVIDER_ID_XIAOMI_MIMO_TOKEN_PLAN
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openrouter_is_marked_as_audio_capable_for_both_tools() {
        assert!(provider_supports_audio_tool(
            PROVIDER_ID_OPENROUTER,
            AudioToolKind::SpeechToText,
        ));
        assert!(provider_supports_audio_tool(
            PROVIDER_ID_OPENROUTER,
            AudioToolKind::TextToSpeech,
        ));
    }

    #[test]
    fn anthropic_is_not_marked_as_direct_audio_tool_provider() {
        assert!(!provider_supports_audio_tool(
            PROVIDER_ID_ANTHROPIC,
            AudioToolKind::SpeechToText,
        ));
        assert!(!provider_supports_audio_tool(
            PROVIDER_ID_ANTHROPIC,
            AudioToolKind::TextToSpeech,
        ));
    }

    #[test]
    fn xai_is_marked_as_audio_capable_for_both_tools() {
        assert!(provider_supports_audio_tool(
            PROVIDER_ID_XAI,
            AudioToolKind::SpeechToText,
        ));
        assert!(provider_supports_audio_tool(
            PROVIDER_ID_XAI,
            AudioToolKind::TextToSpeech,
        ));
    }

    #[test]
    fn elevenlabs_is_marked_as_audio_capable_for_both_tools() {
        assert!(provider_supports_audio_tool(
            PROVIDER_ID_ELEVENLABS,
            AudioToolKind::SpeechToText,
        ));
        assert!(provider_supports_audio_tool(
            PROVIDER_ID_ELEVENLABS,
            AudioToolKind::TextToSpeech,
        ));
    }

    #[test]
    fn xiaomi_is_marked_as_audio_capable_provider() {
        assert!(provider_supports_audio_tool(
            PROVIDER_ID_XIAOMI_MIMO_TOKEN_PLAN,
            AudioToolKind::SpeechToText,
        ));
        assert!(provider_supports_audio_tool(
            PROVIDER_ID_XIAOMI_MIMO_TOKEN_PLAN,
            AudioToolKind::TextToSpeech,
        ));
    }

    #[test]
    fn minimax_providers_are_marked_as_tts_only_audio_providers() {
        for provider_id in [PROVIDER_ID_MINIMAX, PROVIDER_ID_MINIMAX_CODING_PLAN] {
            assert!(!provider_supports_audio_tool(
                provider_id,
                AudioToolKind::SpeechToText,
            ));
            assert!(provider_supports_audio_tool(
                provider_id,
                AudioToolKind::TextToSpeech,
            ));
        }
    }

    #[test]
    fn derive_model_feature_capabilities_preserves_directional_audio_semantics() {
        let metadata = serde_json::json!({
            "architecture": {
                "input_modalities": ["text", "audio"],
                "output_modalities": ["text"],
                "modality": "text+audio->text",
            }
        });

        let features = derive_model_feature_capabilities(
            PROVIDER_ID_OPENROUTER,
            "openai/gpt-audio",
            Some(&metadata),
            false,
        );

        assert!(features.stt);
        assert!(!features.tts);
        assert!(!features.image_generation);
    }

    #[test]
    fn derive_model_feature_capabilities_detects_image_generation_from_output_or_pricing() {
        let metadata = serde_json::json!({
            "architecture": {
                "input_modalities": ["text"],
                "output_modalities": ["image"],
            }
        });

        let features = derive_model_feature_capabilities(
            PROVIDER_ID_OPENROUTER,
            "openai/gpt-image",
            Some(&metadata),
            true,
        );

        assert!(features.image_generation);
        assert!(!features.stt);
        assert!(!features.tts);
    }

    #[test]
    fn github_copilot_gemini_31_is_forced_to_chat_completions() {
        assert_eq!(
            fixed_api_transport_for_model(PROVIDER_ID_GITHUB_COPILOT, "gemini-3.1-pro-preview",),
            Some("chat_completions")
        );
        assert_eq!(
            fixed_api_transport_for_model(PROVIDER_ID_GITHUB_COPILOT, "gpt-5.4"),
            None
        );
    }
}
