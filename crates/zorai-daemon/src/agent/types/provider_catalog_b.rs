use super::*;

pub const ALIBABA_CODING_MODELS: &[ModelDefinition] = &[
    ModelDefinition {
        id: "qwen3-coder-plus",
        name: "Qwen3 Coder Plus",
        context_window: 997952,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "qwen3-coder-next",
        name: "Qwen3 Coder Next",
        context_window: 204800,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "qwen3.6-plus",
        name: "Qwen3.6 Plus",
        context_window: 983616,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "qwen3.8-flash",
        name: "Qwen3.8 Flash",
        context_window: 1_000_000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "glm-5",
        name: "GLM-5",
        context_window: 202752,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "kimi-k2.6",
        name: "Kimi K2.6",
        context_window: 262144,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "kimi-k2.5",
        name: "Kimi K2.5",
        context_window: 262144,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "MiniMax-M2.5",
        name: "MiniMax M2.5",
        context_window: 205000,
        modalities: TEXT_ONLY,
    },
];

pub const ALIBABA_TOKEN_MODELS: &[ModelDefinition] = &[
    ModelDefinition {
        id: "qwen3.8-max",
        name: "Qwen3.8 Max",
        context_window: 1_000_000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "qwen3.7-max",
        name: "Qwen3.7 Max",
        context_window: 1_000_000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "qwen3.7-plus",
        name: "Qwen3.7 Plus",
        context_window: 1_000_000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "qwen3.6-flash",
        name: "Qwen3.6 Flash",
        context_window: 1_000_000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "glm-5.2",
        name: "GLM-5.2",
        context_window: 1_000_000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "deepseek-v4-pro",
        name: "DeepSeek V4 Pro",
        context_window: 1_000_000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "wan2.7-image",
        name: "Wan2.7 Image",
        context_window: 32_768,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "wan2.7-image-pro",
        name: "Wan2.7 Image Pro",
        context_window: 32_768,
        modalities: TEXT_IMAGE,
    },
];

pub const ANTHROPIC_MODELS: &[ModelDefinition] = &[
    ModelDefinition {
        id: "claude-opus-5-5",
        name: "Claude Opus 5.5",
        context_window: 1_000_000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "claude-fable-5-2",
        name: "Claude Fable 5.2",
        context_window: 1_000_000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "claude-fable-5-1",
        name: "Claude Fable 5.1",
        context_window: 1_000_000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "claude-fable-5",
        name: "Claude Fable 5",
        context_window: 1_000_000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "claude-opus-5",
        name: "Claude Opus 5",
        context_window: 1_000_000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "claude-opus-4-8",
        name: "Claude Opus 4.8",
        context_window: 1_000_000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "claude-opus-4-7",
        name: "Claude Opus 4.7",
        context_window: 1_000_000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "claude-opus-4-6",
        name: "Claude Opus 4.6",
        context_window: 1_000_000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "claude-opus-4-5-20251101",
        name: "Claude Opus 4.5",
        context_window: 200_000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "claude-sonnet-5-5",
        name: "Claude Sonnet 5.5",
        context_window: 1_000_000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "claude-sonnet-5",
        name: "Claude Sonnet 5",
        context_window: 1_000_000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "claude-sonnet-4-6",
        name: "Claude Sonnet 4.6",
        context_window: 1_000_000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "claude-sonnet-4-5-20250929",
        name: "Claude Sonnet 4.5",
        context_window: 200_000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "claude-haiku-5-5",
        name: "Claude Haiku 5.5",
        context_window: 1_000_000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "claude-haiku-4-5-20251001",
        name: "Claude Haiku 4.5",
        context_window: 200_000,
        modalities: TEXT_IMAGE,
    },
];

pub const XIAOMI_MIMO_MODELS: &[ModelDefinition] = &[
    ModelDefinition {
        id: "mimo-v2.5-pro",
        name: "MiMo V2.5 Pro",
        context_window: 1_000_000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "mimo-v2.5-pro-ultraspeed",
        name: "MiMo V2.5 Pro UltraSpeed",
        context_window: 1_000_000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "mimo-v2.5",
        name: "MiMo V2.5",
        context_window: 1_000_000,
        modalities: MULTIMODAL,
    },
    ModelDefinition {
        id: "mimo-v2.5-asr",
        name: "MiMo V2.5 ASR",
        context_window: 128_000,
        modalities: TEXT_AUDIO,
    },
    ModelDefinition {
        id: "mimo-v2.5-tts-voiceclone",
        name: "MiMo V2.5 TTS VoiceClone",
        context_window: 128_000,
        modalities: TEXT_AUDIO,
    },
    ModelDefinition {
        id: "mimo-v2.5-tts-voicedesign",
        name: "MiMo V2.5 TTS VoiceDesign",
        context_window: 128_000,
        modalities: TEXT_AUDIO,
    },
    ModelDefinition {
        id: "mimo-v2.5-tts",
        name: "MiMo V2.5 TTS",
        context_window: 128_000,
        modalities: TEXT_AUDIO,
    },
    ModelDefinition {
        id: "mimo-v2-pro",
        name: "MiMo V2 Pro",
        context_window: 1_000_000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "mimo-v2-omni",
        name: "MiMo V2 Omni",
        context_window: 256_000,
        modalities: MULTIMODAL,
    },
    ModelDefinition {
        id: "mimo-v2-tts",
        name: "MiMo V2 TTS",
        context_window: 128_000,
        modalities: TEXT_AUDIO,
    },
];

pub const NOUS_PORTAL_MODELS: &[ModelDefinition] = &[
    ModelDefinition {
        id: "nousresearch/hermes-4-70b",
        name: "Nous: Hermes 4 70B",
        context_window: 131072,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "nousresearch/hermes-4-405b",
        name: "Nous: Hermes 4 405B",
        context_window: 131072,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "nousresearch/hermes-3-llama-3.1-70b",
        name: "Nous: Hermes 3 70B Instruct",
        context_window: 131072,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "nousresearch/hermes-3-llama-3.1-405b",
        name: "Nous: Hermes 3 405B Instruct",
        context_window: 131072,
        modalities: TEXT_ONLY,
    },
];

pub const HERMES_AGENT_API_MODELS: &[ModelDefinition] = &[ModelDefinition {
    id: "hermes-agent",
    name: "Hermes Agent",
    context_window: 128_000,
    modalities: TEXT_ONLY,
}];

pub const ELEVENLABS_MODELS: &[ModelDefinition] = &[
    ModelDefinition {
        id: "scribe_v2",
        name: "Scribe v2",
        context_window: 0,
        modalities: TEXT_AUDIO,
    },
    ModelDefinition {
        id: "eleven_multilingual_v2",
        name: "Eleven Multilingual v2",
        context_window: 0,
        modalities: TEXT_AUDIO,
    },
];

pub const OPENCODE_ZEN_MODELS: &[ModelDefinition] = &[
    ModelDefinition {
        id: "claude-opus-4-6",
        name: "Claude Opus 4.6",
        context_window: 200000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "claude-sonnet-4-5",
        name: "Claude Sonnet 4.5",
        context_window: 200000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "claude-sonnet-4",
        name: "Claude Sonnet 4",
        context_window: 200000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "gpt-5.4",
        name: "GPT-5.4",
        context_window: 128000,
        modalities: MULTIMODAL,
    },
    ModelDefinition {
        id: "gpt-5.4-mini",
        name: "GPT-5.4 Mini",
        context_window: 128000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "gpt-5.3-codex",
        name: "GPT-5.3 Codex",
        context_window: 128000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "minimax-m2.5",
        name: "MiniMax M2.5",
        context_window: 205000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "glm-5",
        name: "GLM-5",
        context_window: 128000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "kimi-k2.6",
        name: "Kimi K2.6",
        context_window: 262144,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "kimi-k2.5",
        name: "Kimi K2.5",
        context_window: 262144,
        modalities: TEXT_ONLY,
    },
];

pub const OPENCODE_GO_MODELS: &[ModelDefinition] = &[
    ModelDefinition {
        id: "glm-5.2",
        name: "GLM-5.2",
        context_window: 1_000_000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "glm-5.1",
        name: "GLM-5.1",
        context_window: 204_800,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "glm-5",
        name: "GLM-5",
        context_window: 202_752,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "kimi-k2.7-code",
        name: "Kimi K2.7 Code",
        context_window: 262_144,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "kimi-k2.6",
        name: "Kimi K2.6",
        context_window: 262_144,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "deepseek-v4-pro",
        name: "DeepSeek V4 Pro",
        context_window: 1_000_000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "deepseek-v4-flash",
        name: "DeepSeek V4 Flash",
        context_window: 1_000_000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "mimo-v2.5",
        name: "MiMo V2.5",
        context_window: 1_000_000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "mimo-v2.5-pro",
        name: "MiMo V2.5 Pro",
        context_window: 1_000_000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "minimax-m3",
        name: "MiniMax M3",
        context_window: 1_000_000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "minimax-m2.7",
        name: "MiniMax M2.7",
        context_window: 205_000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "minimax-m2.5",
        name: "MiniMax M2.5",
        context_window: 205_000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "qwen3.7-max",
        name: "Qwen3.7 Max",
        context_window: 1_000_000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "qwen3.7-plus",
        name: "Qwen3.7 Plus",
        context_window: 1_000_000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "qwen3.6-plus",
        name: "Qwen3.6 Plus",
        context_window: 983_616,
        modalities: TEXT_IMAGE,
    },
];

pub const ARCEE_MODELS: &[ModelDefinition] = &[ModelDefinition {
    id: "trinity-large-thinking",
    name: "Trinity Large Thinking",
    context_window: 256000,
    modalities: TEXT_ONLY,
}];

pub const OPENROUTER_MODELS: &[ModelDefinition] = &[
    ModelDefinition {
        id: "arcee-ai/trinity-large-thinking",
        name: "Claude Sonnet 4",
        context_window: 200000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "anthropic/claude-3.5-sonnet",
        name: "Claude 3.5 Sonnet",
        context_window: 200000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "openai/gpt-4o",
        name: "GPT-4o",
        context_window: 128000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "google/gemini-pro-1.5",
        name: "Gemini Pro 1.5",
        context_window: 1000000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "meta-llama/llama-3.3-70b-instruct",
        name: "Llama 3.3 70B",
        context_window: 128000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "deepseek/deepseek-chat",
        name: "DeepSeek Chat",
        context_window: 64000,
        modalities: TEXT_ONLY,
    },
];

pub const GROQ_MODELS: &[ModelDefinition] = &[
    ModelDefinition {
        id: "llama-3.3-70b-versatile",
        name: "Llama 3.3 70B Versatile",
        context_window: 128000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "llama-3.3-70b-specdec",
        name: "Llama 3.3 70B Speculative",
        context_window: 8192,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "llama-3.1-8b-instant",
        name: "Llama 3.1 8B",
        context_window: 128000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "mixtral-8x7b-32768",
        name: "Mixtral 8x7B",
        context_window: 32768,
        modalities: TEXT_ONLY,
    },
];

pub const CEREBRAS_MODELS: &[ModelDefinition] = &[
    ModelDefinition {
        id: "llama-3.3-70b",
        name: "Llama 3.3 70B",
        context_window: 128000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "llama-3.1-8b",
        name: "Llama 3.1 8B",
        context_window: 128000,
        modalities: TEXT_ONLY,
    },
];

pub const TOGETHER_MODELS: &[ModelDefinition] = &[
    ModelDefinition {
        id: "meta-llama/Llama-3.3-70B-Instruct-Turbo",
        name: "Llama 3.3 70B Turbo",
        context_window: 128000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "meta-llama/Llama-3.2-90B-Vision-Instruct-Turbo",
        name: "Llama 3.2 90B Vision",
        context_window: 131072,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "Qwen/Qwen2.5-72B-Instruct-Turbo",
        name: "Qwen 2.5 72B Turbo",
        context_window: 32768,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "deepseek-ai/DeepSeek-V3",
        name: "DeepSeek V3",
        context_window: 128000,
        modalities: TEXT_ONLY,
    },
];

pub const OLLAMA_MODELS: &[ModelDefinition] = &[
    ModelDefinition {
        id: "llama3.1",
        name: "Llama 3.1",
        context_window: 128000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "llama3.2",
        name: "Llama 3.2",
        context_window: 128000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "qwen2.5",
        name: "Qwen 2.5",
        context_window: 128000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "codellama",
        name: "Code Llama",
        context_window: 16384,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "mistral",
        name: "Mistral",
        context_window: 32768,
        modalities: TEXT_ONLY,
    },
];

pub const CHUTES_MODELS: &[ModelDefinition] = &[ModelDefinition {
    id: "deepseek-ai/DeepSeek-R1",
    name: "DeepSeek R1",
    context_window: 128000,
    modalities: TEXT_ONLY,
}];

pub const HUGGINGFACE_MODELS: &[ModelDefinition] = &[
    ModelDefinition {
        id: "meta-llama/Llama-3.3-70B-Instruct",
        name: "Llama 3.3 70B",
        context_window: 128000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "Qwen/Qwen2.5-72B-Instruct",
        name: "Qwen 2.5 72B",
        context_window: 32768,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "mistralai/Mistral-7B-Instruct-v0.3",
        name: "Mistral 7B",
        context_window: 32768,
        modalities: TEXT_ONLY,
    },
];

pub const FEATHERLESS_MODELS: &[ModelDefinition] = &[
    ModelDefinition {
        id: "meta-llama/Llama-3.3-70B-Instruct",
        name: "Llama 3.3 70B",
        context_window: 128000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "Qwen/Qwen2.5-72B-Instruct",
        name: "Qwen 2.5 72B",
        context_window: 32768,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "mistralai/Mistral-Small-24B-Instruct-2501",
        name: "Mistral Small 24B",
        context_window: 32768,
        modalities: TEXT_ONLY,
    },
];

pub const EMPTY_MODELS: &[ModelDefinition] = &[];
