import type {
  AgentProviderConfig,
  AgentProviderId,
  ApiTransportMode,
  ApiType,
  AuthSource,
  ModelDefinition,
  Modality,
  ProviderDefinition,
} from "./types.ts";
import { normalizeOpenRouterProviderSlugs } from "../openrouterProviderRouting";

export type AudioToolEndpoint = "stt" | "tts";

type AuthSourceSupportOptions = {
  daemonOwnedAuthAvailable?: boolean;
};

const API_KEY_ONLY_AUTH_SOURCES: AuthSource[] = ["api_key"];
const OPENAI_AUTH_SOURCES: AuthSource[] = ["chatgpt_subscription", "api_key"];
const GITHUB_COPILOT_AUTH_SOURCES: AuthSource[] = ["github_copilot", "api_key"];

const M_MULTI: Modality[] = ["text", "image", "video", "audio"];
const M_TI: Modality[] = ["text", "image"];
const M_TA: Modality[] = ["text", "audio"];
const OPENAI_API_MODELS: ModelDefinition[] = [
  { id: "gpt-5.5", name: "GPT-5.5", contextWindow: 1_000_000, modalities: M_MULTI },
  { id: "gpt-6-astra", name: "GPT-6 Astra", contextWindow: 1_050_000, modalities: M_TI },
  { id: "gpt-5.6-sol", name: "GPT-5.6 Sol", contextWindow: 1_000_000, modalities: M_TI },
  { id: "gpt-5.6-terra", name: "GPT-5.6 Terra", contextWindow: 1_000_000, modalities: M_TI },
  { id: "gpt-5.6-luna", name: "GPT-5.6 Luna", contextWindow: 1_000_000, modalities: M_TI },
  { id: "gpt-5.4", name: "GPT-5.4", contextWindow: 1_000_000, modalities: M_MULTI },
  { id: "gpt-5.4-mini", name: "GPT-5.4 Mini", contextWindow: 400_000, modalities: M_TI },
  { id: "gpt-5.4-nano", name: "GPT-5.4 Nano", contextWindow: 400_000, modalities: M_TI },
  { id: "gpt-5.3-codex", name: "GPT-5.3 Codex", contextWindow: 400_000, modalities: M_TI },
  { id: "gpt-5.3-codex-spark", name: "GPT-5.3 Codex Spark", contextWindow: 128_000 },
  { id: "gpt-5.2-codex", name: "GPT-5.2 Codex", contextWindow: 400_000, modalities: M_TI },
  { id: "gpt-5.2", name: "GPT-5.2", contextWindow: 400_000, modalities: M_TI },
  { id: "gpt-5.1-codex-max", name: "GPT-5.1 Codex Max", contextWindow: 400_000, modalities: M_TI },
  { id: "gpt-5.1-codex", name: "GPT-5.1 Codex", contextWindow: 400_000, modalities: M_TI },
  { id: "gpt-5.1-codex-mini", name: "GPT-5.1 Codex Mini", contextWindow: 400_000, modalities: M_TI },
  { id: "gpt-5.1", name: "GPT-5.1", contextWindow: 400_000, modalities: M_TI },
  { id: "gpt-5-codex", name: "GPT-5 Codex", contextWindow: 400_000, modalities: M_TI },
  { id: "gpt-5-codex-mini", name: "GPT-5 Codex Mini", contextWindow: 200_000, modalities: M_TI },
  { id: "gpt-5", name: "GPT-5", contextWindow: 400_000, modalities: M_TI },
  { id: "codex-mini-latest", name: "Codex Mini Latest", contextWindow: 200_000, modalities: M_TI },
  { id: "o3", name: "o3", contextWindow: 200_000, modalities: M_TI },
  { id: "o4-mini", name: "o4 Mini", contextWindow: 200_000, modalities: M_TI },
];

const OPENAI_CHATGPT_SUBSCRIPTION_MODELS: ModelDefinition[] = [
  { id: "gpt-5.5", name: "GPT-5.5", contextWindow: 1_000_000, modalities: M_TI },
  { id: "gpt-6-astra", name: "GPT-6 Astra", contextWindow: 1_050_000, modalities: M_TI },
  { id: "gpt-5.6-sol", name: "GPT-5.6 Sol", contextWindow: 1_000_000, modalities: M_TI },
  { id: "gpt-5.6-sol-pro", name: "GPT-5.6 Sol Pro", contextWindow: 1_000_000, modalities: M_TI },
  { id: "gpt-5.6-terra", name: "GPT-5.6 Terra", contextWindow: 1_000_000, modalities: M_TI },
  { id: "gpt-5.6-luna", name: "GPT-5.6 Luna", contextWindow: 1_000_000, modalities: M_TI },
  { id: "gpt-5.4", name: "GPT-5.4", contextWindow: 1_000_000, modalities: M_TI },
  { id: "gpt-5.4-mini", name: "GPT-5.4 Mini", contextWindow: 400_000, modalities: M_TI },
  { id: "gpt-5.3-codex", name: "GPT-5.3 Codex", contextWindow: 400_000, modalities: M_TI },
  { id: "gpt-5.3-codex-spark", name: "GPT-5.3 Codex Spark", contextWindow: 128_000 },
  { id: "gpt-5.2-codex", name: "GPT-5.2 Codex", contextWindow: 400_000, modalities: M_TI },
  { id: "gpt-5.2", name: "GPT-5.2", contextWindow: 400_000, modalities: M_TI },
  { id: "gpt-5.1-codex-max", name: "GPT-5.1 Codex Max", contextWindow: 400_000, modalities: M_TI },
  { id: "gpt-5.1-codex-mini", name: "GPT-5.1 Codex Mini", contextWindow: 400_000, modalities: M_TI },
];

const XAI_MODELS: ModelDefinition[] = [
  { id: "grok-4.6", name: "Grok 4.6", contextWindow: 500_000, modalities: M_TI },
  { id: "grok-4.5", name: "Grok 4.5", contextWindow: 500_000, modalities: M_TI },
  { id: "grok-4.3", name: "Grok 4.3", contextWindow: 1_000_000, modalities: M_TI },
  { id: "grok-build-0.1", name: "Grok Build 0.1", contextWindow: 256_000, modalities: M_TI },
  { id: "grok-4", name: "Grok 4", contextWindow: 256_000, modalities: M_TI },
  { id: "grok-code-fast-1", name: "Grok Code Fast 1", contextWindow: 256_000 },
];

const ELEVENLABS_MODELS: ModelDefinition[] = [
  { id: "scribe_v2", name: "Scribe v2", contextWindow: 0, modalities: M_TA },
  { id: "eleven_multilingual_v2", name: "Eleven Multilingual v2", contextWindow: 0, modalities: M_TA },
];

const DEEPSEEK_MODELS: ModelDefinition[] = [
  { id: "deepseek-v4-pro", name: "DeepSeek V4 Pro", contextWindow: 1_048_576 },
  { id: "deepseek-v4-flash", name: "DeepSeek V4 Flash", contextWindow: 1_048_576 },
];

const QWEN_MODELS: ModelDefinition[] = [
  { id: "qwen-max", name: "Qwen Max", contextWindow: 32768, modalities: M_TI },
  { id: "qwen-plus", name: "Qwen Plus", contextWindow: 32768, modalities: M_TI },
  { id: "qwen-turbo", name: "Qwen Turbo", contextWindow: 8192 },
  { id: "qwen-long", name: "Qwen Long", contextWindow: 1_000_000 },
];

const ANTHROPIC_MODELS: ModelDefinition[] = [
  { id: "claude-opus-4-7", name: "Claude Opus 4.7", contextWindow: 1_000_000, modalities: M_TI },
  { id: "claude-opus-4-6", name: "Claude Opus 4.6", contextWindow: 1_000_000, modalities: M_TI },
  { id: "claude-opus-4-5-20251101", name: "Claude Opus 4.5", contextWindow: 200_000, modalities: M_TI },
  { id: "claude-opus-4-1-20250805", name: "Claude Opus 4.1", contextWindow: 200_000, modalities: M_TI },
  { id: "claude-opus-4-20250514", name: "Claude Opus 4", contextWindow: 200_000, modalities: M_TI },
  { id: "claude-sonnet-4-6", name: "Claude Sonnet 4.6", contextWindow: 1_000_000, modalities: M_TI },
  { id: "claude-sonnet-4-5-20250929", name: "Claude Sonnet 4.5", contextWindow: 200_000, modalities: M_TI },
  { id: "claude-sonnet-4-20250514", name: "Claude Sonnet 4", contextWindow: 200_000, modalities: M_TI },
  { id: "claude-3-7-sonnet-20250219", name: "Claude Sonnet 3.7", contextWindow: 200_000, modalities: M_TI },
  { id: "claude-haiku-4-5-20251001", name: "Claude Haiku 4.5", contextWindow: 200_000, modalities: M_TI },
  { id: "claude-3-5-haiku-20241022", name: "Claude Haiku 3.5", contextWindow: 200_000, modalities: M_TI },
  { id: "claude-3-opus-20240229", name: "Claude Opus 3", contextWindow: 200_000, modalities: M_TI },
  { id: "claude-3-haiku-20240307", name: "Claude Haiku 3", contextWindow: 200_000, modalities: M_TI },
];

const CURSOR_MODELS: ModelDefinition[] = [
  { id: "auto", name: "Auto", contextWindow: 200_000, modalities: M_TI },
  { id: "composer-2.5", name: "Composer 2.5", contextWindow: 200_000, modalities: M_TI },
  { id: "grok-4.7", name: "Grok 4.7", contextWindow: 200_000, modalities: M_TI },
  { id: "grok-4.6", name: "Grok 4.6", contextWindow: 200_000, modalities: M_TI },
  { id: "grok-4.5", name: "Grok 4.5", contextWindow: 200_000, modalities: M_TI },
];

const GITHUB_COPILOT_MODELS: ModelDefinition[] = [
  { id: "gpt-5.4", name: "GPT-5.4", contextWindow: 400_000, modalities: M_TI },
  { id: "gpt-5.5", name: "GPT-5.5", contextWindow: 400_000, modalities: M_TI },
  { id: "claude-haiku-4.5", name: "Claude Haiku 4.5", contextWindow: 160_000, modalities: M_TI },
  { id: "claude-opus-4.5", name: "Claude Opus 4.5", contextWindow: 160_000, modalities: M_TI },
  { id: "claude-opus-4.6", name: "Claude Opus 4.6", contextWindow: 192_000, modalities: M_TI },
  { id: "claude-opus-4.6-fast", name: "Claude Opus 4.6 (fast mode) (Preview)", contextWindow: 192_000, modalities: M_TI },
  { id: "claude-sonnet-4", name: "Claude Sonnet 4", contextWindow: 144_000, modalities: M_TI },
  { id: "claude-sonnet-4.5", name: "Claude Sonnet 4.5", contextWindow: 160_000, modalities: M_TI },
  { id: "claude-sonnet-4.6", name: "Claude Sonnet 4.6", contextWindow: 160_000, modalities: M_TI },
  { id: "gemini-2.5-pro", name: "Gemini 2.5 Pro", contextWindow: 173_000, modalities: M_TI },
  { id: "gemini-3-flash-preview", name: "Gemini 3 Flash (Preview)", contextWindow: 173_000, modalities: M_TI },
  { id: "gemini-3.1-pro-preview", name: "Gemini 3.1 Pro (Preview)", contextWindow: 173_000, modalities: M_TI },
  { id: "gpt-4.1", name: "GPT-4.1", contextWindow: 128_000, modalities: M_TI },
  { id: "gpt-4o", name: "GPT-4o", contextWindow: 128_000, modalities: M_TI },
  { id: "gpt-5-mini", name: "GPT-5 mini", contextWindow: 192_000, modalities: M_TI },
  { id: "gpt-5.1", name: "GPT-5.1", contextWindow: 192_000, modalities: M_TI },
  { id: "gpt-5.1-codex", name: "GPT-5.1-Codex", contextWindow: 256_000, modalities: M_TI },
  { id: "gpt-5.1-codex-max", name: "GPT-5.1-Codex-Max", contextWindow: 256_000, modalities: M_TI },
  { id: "gpt-5.1-codex-mini", name: "GPT-5.1-Codex-Mini (Preview)", contextWindow: 256_000, modalities: M_TI },
  { id: "gpt-5.2", name: "GPT-5.2", contextWindow: 192_000, modalities: M_TI },
  { id: "gpt-5.2-codex", name: "GPT-5.2-Codex", contextWindow: 400_000, modalities: M_TI },
  { id: "gpt-5.3-codex", name: "GPT-5.3-Codex", contextWindow: 400_000, modalities: M_TI },
  { id: "gpt-5.4-mini", name: "GPT-5.4 mini", contextWindow: 400_000, modalities: M_TI },
  { id: "grok-code-fast-1", name: "Grok Code Fast 1", contextWindow: 173_000 },
  { id: "raptor-mini", name: "Raptor mini (Preview)", contextWindow: 264_000, modalities: M_TI },
  { id: "goldeneye", name: "Goldeneye", contextWindow: 524_000, modalities: M_TI },
];

const ZAI_MODELS: ModelDefinition[] = [
  { id: "glm-4-plus", name: "GLM-4 Plus", contextWindow: 128000 },
  { id: "glm-5.3-flash", name: "GLM-5.3 Flash", contextWindow: 1_000_000, modalities: M_TI },
  { id: "glm-5.1", name: "GLM-5.1", contextWindow: 204800 },
  { id: "glm-5", name: "GLM-5", contextWindow: 128000 },
  { id: "glm-4", name: "GLM-4", contextWindow: 128000 },
  { id: "glm-4-air", name: "GLM-4 Air", contextWindow: 128000 },
  { id: "glm-4-flash", name: "GLM-4 Flash", contextWindow: 128000 },
];

const ZAI_CODING_MODELS: ModelDefinition[] = [
  { id: "glm-5.3", name: "GLM-5.3", contextWindow: 1_000_000 },
  { id: "glm-5.3-flash", name: "GLM-5.3 Flash", contextWindow: 1_000_000, modalities: M_TI },
  { id: "glm-5-turbo", name: "GLM-5 Turbo", contextWindow: 200_000 },
  { id: "glm-4.7", name: "GLM-4.7", contextWindow: 200_000 },
  { id: "glm-5", name: "GLM-5", contextWindow: 128000 },
  { id: "glm-5.2", name: "GLM-5.2", contextWindow: 204800 },
  { id: "glm-5.1", name: "GLM-5.1", contextWindow: 204800 },
  { id: "glm-4-plus", name: "GLM-4 Plus", contextWindow: 128000 },
  { id: "glm-4", name: "GLM-4", contextWindow: 128000 },
  { id: "glm-4-air", name: "GLM-4 Air", contextWindow: 128000 },
  { id: "glm-4-flash", name: "GLM-4 Flash", contextWindow: 128000 },
];

const ARCEE_MODELS: ModelDefinition[] = [
  { id: "trinity-large-thinking", name: "Trinity Large Thinking", contextWindow: 256_000 },
];

const NVIDIA_MODELS: ModelDefinition[] = [
  { id: "minimaxai/minimax-m2.7", name: "MiniMax M2.7", contextWindow: 205_000 },
];

const POOLSIDE_MODELS: ModelDefinition[] = [
  { id: "poolside/laguna-m.1", name: "Poolside Laguna M.1", contextWindow: 256_000 },
];

const KIMI_MODELS: ModelDefinition[] = [
  { id: "moonshot-v1-32k", name: "Moonshot V1 32K", contextWindow: 32768 },
  { id: "moonshot-v1-8k", name: "Moonshot V1 8K", contextWindow: 8192 },
  { id: "moonshot-v1-128k", name: "Moonshot V1 128K", contextWindow: 131072 },
];

const KIMI_CODING_MODELS: ModelDefinition[] = [
  { id: "k3", name: "Kimi K3", contextWindow: 262144 },
  { id: "kimi-for-coding", name: "Kimi for Coding", contextWindow: 262144 },
  { id: "kimi-k2.7-code", name: "Kimi K2.7 Code", contextWindow: 262144 },
  { id: "kimi-k2.6", name: "Kimi K2.6", contextWindow: 262144 },
  { id: "kimi-k2.5", name: "Kimi K2.5", contextWindow: 262144 },
  { id: "kimi-k2-turbo-preview", name: "Kimi K2 Turbo Preview", contextWindow: 262144 },
];

const MINIMAX_MODELS: ModelDefinition[] = [
  { id: "MiniMax-M3", name: "MiniMax M3", contextWindow: 1_000_000 },
  { id: "MiniMax-M2.7", name: "MiniMax M2.7", contextWindow: 205_000 },
  { id: "MiniMax-M2.7-highspeed", name: "MiniMax M2.7 Highspeed", contextWindow: 204_800 },
  { id: "MiniMax-M2.5", name: "MiniMax M2.5", contextWindow: 204_800 },
  { id: "MiniMax-M2.5-highspeed", name: "MiniMax M2.5 Highspeed", contextWindow: 204_800 },
  { id: "MiniMax-M2.1", name: "MiniMax M2.1", contextWindow: 204_800 },
  { id: "MiniMax-M2.1-highspeed", name: "MiniMax M2.1 Highspeed", contextWindow: 204_800 },
  { id: "MiniMax-M2", name: "MiniMax M2", contextWindow: 204_800 },
];

const ALIBABA_CODING_MODELS: ModelDefinition[] = [
  { id: "qwen3.6-plus", name: "Qwen3.6 Plus", contextWindow: 983616 },
  { id: "qwen3.8-flash", name: "Qwen3.8 Flash", contextWindow: 1_000_000, modalities: M_TI },
  { id: "qwen3-coder-plus", name: "Qwen3 Coder Plus", contextWindow: 997952 },
  { id: "qwen3-coder-next", name: "Qwen3 Coder Next", contextWindow: 204800 },
  { id: "glm-5", name: "GLM-5", contextWindow: 202752 },
  { id: "kimi-k2.6", name: "Kimi K2.6", contextWindow: 262144 },
  { id: "kimi-k2.5", name: "Kimi K2.5", contextWindow: 262144 },
  { id: "MiniMax-M2.5", name: "MiniMax M2.5", contextWindow: 205000 },
];

const ALIBABA_CODING_COMPAT_MODELS: ModelDefinition[] = [
  { id: "qwen3.5-plus", name: "Qwen3.5 Plus", contextWindow: 983616 },
];

const ALIBABA_TOKEN_MODELS: ModelDefinition[] = [
  { id: "qwen3.7-max", name: "Qwen3.7 Max", contextWindow: 1_000_000 },
  { id: "qwen3.7-plus", name: "Qwen3.7 Plus", contextWindow: 1_000_000, modalities: M_TI },
  { id: "qwen3.6-plus", name: "Qwen3.6 Plus", contextWindow: 983_616, modalities: M_TI },
  { id: "qwen3.6-flash", name: "Qwen3.6 Flash", contextWindow: 1_000_000, modalities: M_TI },
  { id: "qwen-image-2.0", name: "Qwen Image 2.0", contextWindow: 32_768, modalities: M_TI },
  { id: "qwen-image-2.0-pro", name: "Qwen Image 2.0 Pro", contextWindow: 32_768, modalities: M_TI },
  { id: "wan2.7-image", name: "Wan2.7 Image", contextWindow: 32_768, modalities: M_TI },
  { id: "wan2.7-image-pro", name: "Wan2.7 Image Pro", contextWindow: 32_768, modalities: M_TI },
  { id: "deepseek-v4-pro", name: "DeepSeek V4 Pro", contextWindow: 1_000_000 },
  { id: "deepseek-v4-flash", name: "DeepSeek V4 Flash", contextWindow: 1_000_000 },
  { id: "deepseek-v3.2", name: "DeepSeek V3.2", contextWindow: 1_000_000 },
  { id: "kimi-k2.6", name: "Kimi K2.6", contextWindow: 262_144, modalities: M_TI },
  { id: "kimi-k2.5", name: "Kimi K2.5", contextWindow: 262_144, modalities: M_TI },
  { id: "glm-5.1", name: "GLM-5.1", contextWindow: 202_752 },
  { id: "glm-5", name: "GLM-5", contextWindow: 202_752 },
  { id: "MiniMax-M2.5", name: "MiniMax M2.5", contextWindow: 205_000 },
];

const XIAOMI_MIMO_TOKEN_PLAN_MODELS: ModelDefinition[] = [
  { id: "mimo-v2.5-pro", name: "MiMo V2.5 Pro", contextWindow: 1_000_000 },
  { id: "mimo-v2.5-pro-ultraspeed", name: "MiMo V2.5 Pro UltraSpeed", contextWindow: 1_000_000 },
  { id: "mimo-v2.5", name: "MiMo V2.5", contextWindow: 1_000_000, modalities: M_MULTI },
  { id: "mimo-v2.5-asr", name: "MiMo V2.5 ASR", contextWindow: 128_000, modalities: M_TA },
  { id: "mimo-v2.5-tts-voiceclone", name: "MiMo V2.5 TTS VoiceClone", contextWindow: 128_000, modalities: M_TA },
  { id: "mimo-v2.5-tts-voicedesign", name: "MiMo V2.5 TTS VoiceDesign", contextWindow: 128_000, modalities: M_TA },
  { id: "mimo-v2.5-tts", name: "MiMo V2.5 TTS", contextWindow: 128_000, modalities: M_TA },
  { id: "mimo-v2-pro", name: "MiMo V2 Pro", contextWindow: 1_000_000 },
  { id: "mimo-v2-omni", name: "MiMo V2 Omni", contextWindow: 256_000, modalities: M_MULTI },
  { id: "mimo-v2-tts", name: "MiMo V2 TTS", contextWindow: 128_000, modalities: M_TA },
];

const NOUS_PORTAL_MODELS: ModelDefinition[] = [
  { id: "nousresearch/hermes-4-70b", name: "Nous: Hermes 4 70B", contextWindow: 131_072 },
  { id: "nousresearch/hermes-4-405b", name: "Nous: Hermes 4 405B", contextWindow: 131_072 },
  { id: "nousresearch/hermes-3-llama-3.1-70b", name: "Nous: Hermes 3 70B Instruct", contextWindow: 131_072 },
  { id: "nousresearch/hermes-3-llama-3.1-405b", name: "Nous: Hermes 3 405B Instruct", contextWindow: 131_072 },
];

const HERMES_AGENT_API_MODELS: ModelDefinition[] = [
  { id: "hermes-agent", name: "Hermes Agent", contextWindow: 128_000 },
];

const OPENCODE_ZEN_MODELS: ModelDefinition[] = [
  { id: "claude-opus-4-6", name: "Claude Opus 4.6", contextWindow: 200000, modalities: M_TI },
  { id: "claude-sonnet-4-5", name: "Claude Sonnet 4.5", contextWindow: 200000, modalities: M_TI },
  { id: "claude-sonnet-4", name: "Claude Sonnet 4", contextWindow: 200000, modalities: M_TI },
  { id: "gpt-5.4", name: "GPT-5.4", contextWindow: 128000, modalities: M_MULTI },
  { id: "gpt-5.3-codex", name: "GPT-5.3 Codex", contextWindow: 128000, modalities: M_TI },
  { id: "minimax-m2.5", name: "MiniMax M2.5", contextWindow: 205000 },
  { id: "glm-5", name: "GLM-5", contextWindow: 128000 },
  { id: "kimi-k2.6", name: "Kimi K2.6", contextWindow: 262144 },
  { id: "kimi-k2.5", name: "Kimi K2.5", contextWindow: 262144 },
];

const OPENCODE_GO_MODELS: ModelDefinition[] = [
  { id: "glm-5.1", name: "GLM-5.1", contextWindow: 204_800 },
  { id: "glm-5", name: "GLM-5", contextWindow: 202_752 },
  { id: "kimi-k2.7-code", name: "Kimi K2.7 Code", contextWindow: 262_144 },
  { id: "kimi-k2.6", name: "Kimi K2.6", contextWindow: 262_144 },
  { id: "deepseek-v4-pro", name: "DeepSeek V4 Pro", contextWindow: 1_000_000 },
  { id: "deepseek-v4-flash", name: "DeepSeek V4 Flash", contextWindow: 1_000_000 },
  { id: "mimo-v2.5", name: "MiMo V2.5", contextWindow: 1_000_000 },
  { id: "mimo-v2.5-pro", name: "MiMo V2.5 Pro", contextWindow: 1_000_000 },
  { id: "minimax-m3", name: "MiniMax M3", contextWindow: 1_000_000 },
  { id: "minimax-m2.7", name: "MiniMax M2.7", contextWindow: 205_000 },
  { id: "minimax-m2.5", name: "MiniMax M2.5", contextWindow: 205_000 },
  { id: "qwen3.7-max", name: "Qwen3.7 Max", contextWindow: 1_000_000 },
  { id: "qwen3.7-plus", name: "Qwen3.7 Plus", contextWindow: 1_000_000, modalities: M_TI },
  { id: "qwen3.6-plus", name: "Qwen3.6 Plus", contextWindow: 983_616, modalities: M_TI },
];

const GROQ_MODELS: ModelDefinition[] = [
  { id: "llama-3.3-70b-versatile", name: "Llama 3.3 70B Versatile", contextWindow: 128000 },
  { id: "llama-3.1-8b-instant", name: "Llama 3.1 8B", contextWindow: 128000 },
];

const OLLAMA_MODELS: ModelDefinition[] = [
  { id: "llama3.1", name: "Llama 3.1", contextWindow: 128000 },
  { id: "llama3.2", name: "Llama 3.2", contextWindow: 128000 },
  { id: "qwen2.5", name: "Qwen 2.5", contextWindow: 128000 },
  { id: "codellama", name: "Code Llama", contextWindow: 16384 },
];

const EMPTY_MODELS: ModelDefinition[] = [];
const CHAT_ONLY_TRANSPORTS: ApiTransportMode[] = ["chat_completions"];
const RESPONSES_AND_CHAT_TRANSPORTS: ApiTransportMode[] = ["responses", "chat_completions"];
const RESPONSES_CHAT_AND_ANTHROPIC_TRANSPORTS: ApiTransportMode[] = [
  "responses",
  "chat_completions",
  "anthropic_messages",
];
const NATIVE_AND_CHAT_TRANSPORTS: ApiTransportMode[] = ["native_assistant", "chat_completions"];
const CHAT_AND_ANTHROPIC_TRANSPORTS: ApiTransportMode[] = ["chat_completions", "anthropic_messages"];
export const DEFAULT_PROVIDER_CONTEXT_WINDOW = 128_000;
export const DEFAULT_CUSTOM_MODEL_CONTEXT_WINDOW = 264_000;

function getCompatibilityModels(
  providerId: AgentProviderId,
): ModelDefinition[] {
  if (providerId === "alibaba-coding-plan") {
    return ALIBABA_CODING_COMPAT_MODELS;
  }
  return EMPTY_MODELS;
}

function normalizeModelLookupValue(value: string | undefined): string {
  return (value ?? "").trim().toLowerCase();
}

export function normalizeAgentProviderId(value: unknown): AgentProviderId {
  if (typeof value !== "string") {
    return "openai";
  }
  const normalized = value.trim();
  return normalized ? (normalized as AgentProviderId) : "openai";
}

export function providerSupportsAudioTool(
  providerId: AgentProviderId,
  kind: AudioToolEndpoint,
): boolean {
  if (kind === "stt") {
    return providerId === "custom"
      || providerId === "openai"
      || providerId === "azure-openai"
      || providerId === "elevenlabs"
      || providerId === "groq"
      || providerId === "openrouter"
      || providerId === "xai"
      || providerId === "xiaomi-mimo-token-plan";
  }
  return providerId === "custom"
    || providerId === "openai"
    || providerId === "azure-openai"
    || providerId === "elevenlabs"
    || providerId === "groq"
    || providerId === "minimax"
    || providerId === "minimax-coding-plan"
    || providerId === "openrouter"
    || providerId === "xai"
    || providerId === "xiaomi-mimo-token-plan";
}

export function normalizeApiTransport(
  providerId: AgentProviderId,
  value: unknown,
): ApiTransportMode {
  const normalized = value === "native_assistant"
    ? "native_assistant"
    : value === "anthropic_messages"
      ? "anthropic_messages"
      : value === "chat_completions"
        ? "chat_completions"
        : "responses";
  return getSupportedApiTransports(providerId).includes(normalized)
    ? normalized
    : getDefaultApiTransport(providerId);
}

export function normalizeAuthSource(
  providerId: AgentProviderId,
  value: unknown,
  options?: AuthSourceSupportOptions,
): AuthSource {
  const normalized = value === "chatgpt_subscription"
    ? "chatgpt_subscription"
    : value === "github_copilot"
      ? "github_copilot"
      : "api_key";
  return getSupportedAuthSources(providerId, options).includes(normalized)
    ? normalized
    : getDefaultAuthSource(providerId, options);
}

export function normalizeProviderConfig(
  providerId: AgentProviderId,
  fallback: AgentProviderConfig,
  value: Partial<AgentProviderConfig> | undefined,
): AgentProviderConfig {
  const auth_source = normalizeAuthSource(providerId, value?.auth_source ?? fallback.auth_source);
  const requestedModel = typeof value?.model === "string" ? value.model.trim() : fallback.model.trim();
  const resolvedModel = requestedModel
    ? resolveProviderModelDefinition(providerId, auth_source, requestedModel)
    : undefined;
  const matchesKnownModel = Boolean(resolvedModel);
  const model = requestedModel
    ? requestedModel
    : getDefaultModelForProvider(providerId, auth_source);
  const custom_model_name = typeof value?.custom_model_name === "string"
    ? value.custom_model_name.trim()
    : "";
  const base_url = !providerUsesConfigurableBaseUrl(providerId)
    ? fallback.base_url
    : (value?.base_url ?? fallback.base_url);
  return {
    ...fallback,
    ...(value ?? {}),
    base_url,
    model,
    custom_model_name: matchesKnownModel ? "" : (custom_model_name || (requestedModel && !matchesKnownModel ? requestedModel : "")),
    assistant_id: typeof value?.assistant_id === "string" ? value.assistant_id : fallback.assistant_id,
    api_transport: normalizeApiTransport(providerId, value?.api_transport ?? fallback.api_transport),
    auth_source,
    context_window_tokens:
      typeof value?.context_window_tokens === "number" && Number.isFinite(value.context_window_tokens)
        ? Math.max(1000, Math.trunc(value.context_window_tokens))
        : null,
    openrouter_provider_order:
      providerId === "openrouter"
        ? normalizeOpenRouterProviderSlugs(value?.openrouter_provider_order)
        : undefined,
    openrouter_provider_ignore:
      providerId === "openrouter"
        ? normalizeOpenRouterProviderSlugs(value?.openrouter_provider_ignore)
        : undefined,
    openrouter_allow_fallbacks:
      providerId === "openrouter" && typeof value?.openrouter_allow_fallbacks === "boolean"
        ? value.openrouter_allow_fallbacks
        : null,
    openrouter_response_cache_enabled:
      providerId === "openrouter" && typeof value?.openrouter_response_cache_enabled === "boolean"
        ? value.openrouter_response_cache_enabled
        : false,
  };
}

export function getModelModalities(model: ModelDefinition | undefined): Modality[] {
  return model?.modalities ?? ["text"];
}

export function modelSupports(model: ModelDefinition | undefined, modality: Modality): boolean {
  return getModelModalities(model).includes(modality);
}

export const PROVIDER_DEFINITIONS: ProviderDefinition[] = [
  { id: "featherless", name: "Featherless", defaultBaseUrl: "https://api.featherless.ai/v1", defaultModel: "meta-llama/Llama-3.3-70B-Instruct", apiType: "openai", authMethod: "bearer", models: [], supportsModelFetch: false, supportedTransports: CHAT_ONLY_TRANSPORTS, defaultTransport: "chat_completions", supportedAuthSources: API_KEY_ONLY_AUTH_SOURCES, defaultAuthSource: "api_key", supportsResponseContinuity: false },
  { id: "anthropic", name: "Anthropic", defaultBaseUrl: "https://api.anthropic.com", defaultModel: "claude-opus-4-7", apiType: "anthropic", authMethod: "x-api-key", models: ANTHROPIC_MODELS, supportsModelFetch: false, supportedTransports: CHAT_ONLY_TRANSPORTS, defaultTransport: "chat_completions", supportedAuthSources: API_KEY_ONLY_AUTH_SOURCES, defaultAuthSource: "api_key", supportsResponseContinuity: false },
  { id: "openai", name: "OpenAI / ChatGPT", defaultBaseUrl: "https://api.openai.com/v1", defaultModel: "gpt-5.5", apiType: "openai", authMethod: "bearer", models: OPENAI_API_MODELS, supportsModelFetch: true, supportedTransports: RESPONSES_AND_CHAT_TRANSPORTS, defaultTransport: "responses", supportedAuthSources: OPENAI_AUTH_SOURCES, defaultAuthSource: "api_key", supportsResponseContinuity: true },
  { id: "deepseek", name: "DeepSeek", defaultBaseUrl: "https://api.deepseek.com", defaultModel: "deepseek-v4-pro", apiType: "openai", authMethod: "bearer", models: DEEPSEEK_MODELS, supportsModelFetch: true, supportedTransports: CHAT_ONLY_TRANSPORTS, defaultTransport: "chat_completions", supportedAuthSources: API_KEY_ONLY_AUTH_SOURCES, defaultAuthSource: "api_key", supportsResponseContinuity: false },
  { id: "xai", name: "xAI", defaultBaseUrl: "https://api.x.ai/v1", defaultModel: "grok-4.6", apiType: "openai", authMethod: "bearer", models: XAI_MODELS, supportsModelFetch: true, supportedTransports: RESPONSES_AND_CHAT_TRANSPORTS, defaultTransport: "responses", supportedAuthSources: API_KEY_ONLY_AUTH_SOURCES, defaultAuthSource: "api_key", supportsResponseContinuity: true },
  { id: "elevenlabs", name: "ElevenLabs", defaultBaseUrl: "https://api.elevenlabs.io", defaultModel: "scribe_v2", apiType: "openai", authMethod: "xi-api-key", models: ELEVENLABS_MODELS, supportsModelFetch: false, supportedTransports: CHAT_ONLY_TRANSPORTS, defaultTransport: "chat_completions", supportedAuthSources: API_KEY_ONLY_AUTH_SOURCES, defaultAuthSource: "api_key", supportsResponseContinuity: false },
  { id: "azure-openai", name: "Azure OpenAI", defaultBaseUrl: "https://YOUR-RESOURCE-NAME.openai.azure.com/openai/v1", defaultModel: "", apiType: "openai", authMethod: "bearer", models: EMPTY_MODELS, supportsModelFetch: true, supportedTransports: RESPONSES_AND_CHAT_TRANSPORTS, defaultTransport: "responses", supportedAuthSources: API_KEY_ONLY_AUTH_SOURCES, defaultAuthSource: "api_key", supportsResponseContinuity: true },
  { id: "github-copilot", name: "GitHub Copilot", defaultBaseUrl: "https://api.githubcopilot.com", defaultModel: "gpt-5.4", apiType: "openai", authMethod: "bearer", models: GITHUB_COPILOT_MODELS, supportsModelFetch: true, supportedTransports: RESPONSES_CHAT_AND_ANTHROPIC_TRANSPORTS, defaultTransport: "responses", supportedAuthSources: GITHUB_COPILOT_AUTH_SOURCES, defaultAuthSource: "github_copilot", supportsResponseContinuity: true },
  { id: "qwen", name: "Qwen", defaultBaseUrl: "https://dashscope-intl.aliyuncs.com/compatible-mode/v1", defaultModel: "qwen-max", apiType: "openai", authMethod: "bearer", models: QWEN_MODELS, supportsModelFetch: true, supportedTransports: NATIVE_AND_CHAT_TRANSPORTS, defaultTransport: "native_assistant", supportedAuthSources: API_KEY_ONLY_AUTH_SOURCES, defaultAuthSource: "api_key", nativeTransportKind: "alibaba_assistant_api", nativeBaseUrl: "https://dashscope-intl.aliyuncs.com/api/v1", supportsResponseContinuity: false },
  { id: "qwen-deepinfra", name: "Qwen (DeepInfra)", defaultBaseUrl: "https://api.deepinfra.com/v1/openai", defaultModel: "Qwen/Qwen2.5-72B-Instruct", apiType: "openai", authMethod: "bearer", models: [], supportsModelFetch: true, supportedTransports: CHAT_ONLY_TRANSPORTS, defaultTransport: "chat_completions", supportedAuthSources: API_KEY_ONLY_AUTH_SOURCES, defaultAuthSource: "api_key", supportsResponseContinuity: false },
  { id: "kimi", name: "Kimi (Moonshot)", defaultBaseUrl: "https://api.moonshot.ai/v1", defaultModel: "moonshot-v1-32k", apiType: "openai", authMethod: "bearer", models: KIMI_MODELS, supportsModelFetch: true, supportedTransports: CHAT_ONLY_TRANSPORTS, defaultTransport: "chat_completions", supportedAuthSources: API_KEY_ONLY_AUTH_SOURCES, defaultAuthSource: "api_key", supportsResponseContinuity: false },
  { id: "kimi-coding-plan", name: "Kimi Coding Plan", defaultBaseUrl: "https://api.kimi.com/coding/v1", defaultModel: "k3", apiType: "openai", authMethod: "bearer", models: KIMI_CODING_MODELS, supportsModelFetch: false, supportedTransports: CHAT_ONLY_TRANSPORTS, defaultTransport: "chat_completions", supportedAuthSources: API_KEY_ONLY_AUTH_SOURCES, defaultAuthSource: "api_key", supportsResponseContinuity: false },
  { id: "z.ai", name: "Z.AI (GLM)", defaultBaseUrl: "https://api.z.ai/api/paas/v4", defaultModel: "glm-4-plus", apiType: "openai", authMethod: "bearer", models: ZAI_MODELS, supportsModelFetch: false, supportedTransports: CHAT_ONLY_TRANSPORTS, defaultTransport: "chat_completions", supportedAuthSources: API_KEY_ONLY_AUTH_SOURCES, defaultAuthSource: "api_key", supportsResponseContinuity: false },
  { id: "z.ai-coding-plan", name: "Z.AI Coding Plan", defaultBaseUrl: "https://api.z.ai/api/coding/paas/v4", defaultModel: "glm-5.3", apiType: "openai", authMethod: "bearer", models: ZAI_CODING_MODELS, supportsModelFetch: false, supportedTransports: CHAT_ONLY_TRANSPORTS, defaultTransport: "chat_completions", supportedAuthSources: API_KEY_ONLY_AUTH_SOURCES, defaultAuthSource: "api_key", supportsResponseContinuity: false },
  { id: "arcee", name: "Arcee", defaultBaseUrl: "https://api.arcee.ai/api/v1", defaultModel: "trinity-large-thinking", apiType: "openai", authMethod: "bearer", models: ARCEE_MODELS, supportsModelFetch: true, supportedTransports: CHAT_ONLY_TRANSPORTS, defaultTransport: "chat_completions", supportedAuthSources: API_KEY_ONLY_AUTH_SOURCES, defaultAuthSource: "api_key", supportsResponseContinuity: false },
  { id: "nvidia", name: "NVIDIA", defaultBaseUrl: "https://integrate.api.nvidia.com/v1", defaultModel: "minimaxai/minimax-m2.7", apiType: "openai", authMethod: "bearer", models: NVIDIA_MODELS, supportsModelFetch: true, supportedTransports: CHAT_ONLY_TRANSPORTS, defaultTransport: "chat_completions", supportedAuthSources: API_KEY_ONLY_AUTH_SOURCES, defaultAuthSource: "api_key", supportsResponseContinuity: false },
  { id: "poolside", name: "Poolside", defaultBaseUrl: "https://inference.poolside.ai/v1", defaultModel: "poolside/laguna-m.1", apiType: "openai", authMethod: "bearer", models: POOLSIDE_MODELS, supportsModelFetch: true, supportedTransports: CHAT_ONLY_TRANSPORTS, defaultTransport: "chat_completions", supportedAuthSources: API_KEY_ONLY_AUTH_SOURCES, defaultAuthSource: "api_key", supportsResponseContinuity: false },
  { id: "nous-portal", name: "Nous Portal", defaultBaseUrl: "https://inference-api.nousresearch.com/v1", defaultModel: "nousresearch/hermes-4-70b", apiType: "openai", authMethod: "bearer", models: NOUS_PORTAL_MODELS, supportsModelFetch: true, supportedTransports: CHAT_ONLY_TRANSPORTS, defaultTransport: "chat_completions", supportedAuthSources: API_KEY_ONLY_AUTH_SOURCES, defaultAuthSource: "api_key", supportsResponseContinuity: false },
  { id: "hermes-agent-api", name: "Hermes Agent API", defaultBaseUrl: "http://localhost:8642/v1", defaultModel: "hermes-agent", apiType: "openai", authMethod: "bearer", models: HERMES_AGENT_API_MODELS, supportsModelFetch: true, supportedTransports: CHAT_ONLY_TRANSPORTS, defaultTransport: "chat_completions", supportedAuthSources: API_KEY_ONLY_AUTH_SOURCES, defaultAuthSource: "api_key", supportsResponseContinuity: false },
  { id: "openrouter", name: "OpenRouter", defaultBaseUrl: "https://openrouter.ai/api/v1", defaultModel: "arcee-ai/trinity-large-thinking", apiType: "openai", authMethod: "bearer", models: [], supportsModelFetch: true, supportedTransports: CHAT_ONLY_TRANSPORTS, defaultTransport: "chat_completions", supportedAuthSources: API_KEY_ONLY_AUTH_SOURCES, defaultAuthSource: "api_key", supportsResponseContinuity: false },
  { id: "cerebras", name: "Cerebras", defaultBaseUrl: "https://api.cerebras.ai/v1", defaultModel: "llama-3.3-70b", apiType: "openai", authMethod: "bearer", models: [], supportsModelFetch: true, supportedTransports: CHAT_ONLY_TRANSPORTS, defaultTransport: "chat_completions", supportedAuthSources: API_KEY_ONLY_AUTH_SOURCES, defaultAuthSource: "api_key", supportsResponseContinuity: false },
  { id: "together", name: "Together", defaultBaseUrl: "https://api.together.xyz/v1", defaultModel: "meta-llama/Llama-3.3-70B-Instruct-Turbo", apiType: "openai", authMethod: "bearer", models: [], supportsModelFetch: true, supportedTransports: CHAT_ONLY_TRANSPORTS, defaultTransport: "chat_completions", supportedAuthSources: API_KEY_ONLY_AUTH_SOURCES, defaultAuthSource: "api_key", supportsResponseContinuity: false },
  { id: "groq", name: "Groq", defaultBaseUrl: "https://api.groq.com/openai/v1", defaultModel: "llama-3.3-70b-versatile", apiType: "openai", authMethod: "bearer", models: GROQ_MODELS, supportsModelFetch: true, supportedTransports: RESPONSES_AND_CHAT_TRANSPORTS, defaultTransport: "responses", supportedAuthSources: API_KEY_ONLY_AUTH_SOURCES, defaultAuthSource: "api_key", supportsResponseContinuity: false },
  { id: "ollama", name: "Ollama", defaultBaseUrl: "http://localhost:11434/v1", defaultModel: "llama3.1", apiType: "openai", authMethod: "bearer", models: OLLAMA_MODELS, supportsModelFetch: true, supportedTransports: CHAT_ONLY_TRANSPORTS, defaultTransport: "chat_completions", supportedAuthSources: API_KEY_ONLY_AUTH_SOURCES, defaultAuthSource: "api_key", supportsResponseContinuity: false },
  { id: "chutes", name: "Chutes", defaultBaseUrl: "https://llm.chutes.ai/v1", defaultModel: "deepseek-ai/DeepSeek-R1", apiType: "openai", authMethod: "bearer", models: [], supportsModelFetch: true, supportedTransports: CHAT_ONLY_TRANSPORTS, defaultTransport: "chat_completions", supportedAuthSources: API_KEY_ONLY_AUTH_SOURCES, defaultAuthSource: "api_key", supportsResponseContinuity: false },
  { id: "huggingface", name: "Hugging Face", defaultBaseUrl: "https://router.huggingface.co/v1", defaultModel: "meta-llama/Llama-3.3-70B-Instruct", apiType: "openai", authMethod: "bearer", models: [], supportsModelFetch: true, supportedTransports: CHAT_ONLY_TRANSPORTS, defaultTransport: "chat_completions", supportedAuthSources: API_KEY_ONLY_AUTH_SOURCES, defaultAuthSource: "api_key", supportsResponseContinuity: false },
  { id: "minimax", name: "MiniMax", defaultBaseUrl: "https://api.minimax.io/anthropic", defaultModel: "MiniMax-M3", apiType: "anthropic", authMethod: "x-api-key", models: MINIMAX_MODELS, supportsModelFetch: true, supportedTransports: CHAT_ONLY_TRANSPORTS, defaultTransport: "chat_completions", supportedAuthSources: API_KEY_ONLY_AUTH_SOURCES, defaultAuthSource: "api_key", supportsResponseContinuity: false },
  { id: "minimax-coding-plan", name: "MiniMax Coding Plan", defaultBaseUrl: "https://api.minimax.io/anthropic", defaultModel: "MiniMax-M3", apiType: "anthropic", authMethod: "x-api-key", models: MINIMAX_MODELS, supportsModelFetch: true, supportedTransports: CHAT_ONLY_TRANSPORTS, defaultTransport: "chat_completions", supportedAuthSources: API_KEY_ONLY_AUTH_SOURCES, defaultAuthSource: "api_key", supportsResponseContinuity: false },
  { id: "alibaba-coding-plan", name: "Alibaba Coding Plan", defaultBaseUrl: "https://coding-intl.dashscope.aliyuncs.com/v1", defaultModel: "qwen3.6-plus", apiType: "openai", authMethod: "bearer", models: ALIBABA_CODING_MODELS, supportsModelFetch: false, supportedTransports: CHAT_ONLY_TRANSPORTS, defaultTransport: "chat_completions", supportedAuthSources: API_KEY_ONLY_AUTH_SOURCES, defaultAuthSource: "api_key", supportsResponseContinuity: false },
  { id: "alibaba-token-plan", name: "Alibaba Token Plan", defaultBaseUrl: "https://token-plan.ap-southeast-1.maas.aliyuncs.com/compatible-mode/v1", defaultModel: "qwen3.7-max", apiType: "openai", authMethod: "bearer", models: ALIBABA_TOKEN_MODELS, supportsModelFetch: false, supportedTransports: CHAT_ONLY_TRANSPORTS, defaultTransport: "chat_completions", supportedAuthSources: API_KEY_ONLY_AUTH_SOURCES, defaultAuthSource: "api_key", supportsResponseContinuity: false },
  { id: "xiaomi-mimo-token-plan", name: "Xiaomi MiMo Token Plan", defaultBaseUrl: "https://token-plan-ams.xiaomimimo.com/v1", defaultModel: "mimo-v2.5-pro", apiType: "openai", authMethod: "bearer", models: XIAOMI_MIMO_TOKEN_PLAN_MODELS, supportsModelFetch: false, anthropicBaseUrl: "https://token-plan-ams.xiaomimimo.com/anthropic", supportedTransports: ["chat_completions", "anthropic_messages"], defaultTransport: "chat_completions", supportedAuthSources: API_KEY_ONLY_AUTH_SOURCES, defaultAuthSource: "api_key", supportsResponseContinuity: false },
  { id: "opencode-zen", name: "OpenCode Zen", defaultBaseUrl: "https://opencode.ai/zen/v1", defaultModel: "claude-sonnet-4-5", apiType: "anthropic", authMethod: "bearer", models: OPENCODE_ZEN_MODELS, supportsModelFetch: true, supportedTransports: CHAT_AND_ANTHROPIC_TRANSPORTS, defaultTransport: "chat_completions", supportedAuthSources: API_KEY_ONLY_AUTH_SOURCES, defaultAuthSource: "api_key", supportsResponseContinuity: false },
  { id: "opencode-go", name: "OpenCode Go", defaultBaseUrl: "https://opencode.ai/zen/go/v1", defaultModel: "glm-5.1", apiType: "anthropic", authMethod: "bearer", models: OPENCODE_GO_MODELS, supportsModelFetch: true, supportedTransports: CHAT_AND_ANTHROPIC_TRANSPORTS, defaultTransport: "chat_completions", supportedAuthSources: API_KEY_ONLY_AUTH_SOURCES, defaultAuthSource: "api_key", supportsResponseContinuity: false },
  { id: "cursor", name: "Cursor", defaultBaseUrl: "", defaultModel: "auto", apiType: "openai", authMethod: "bearer", models: CURSOR_MODELS, supportsModelFetch: false, supportedTransports: ["native_assistant"], defaultTransport: "native_assistant", supportedAuthSources: API_KEY_ONLY_AUTH_SOURCES, defaultAuthSource: "api_key", nativeTransportKind: "cursor_cli", supportsResponseContinuity: false },
  { id: "custom", name: "Custom", defaultBaseUrl: "", defaultModel: "", apiType: "openai", authMethod: "bearer", models: EMPTY_MODELS, supportsModelFetch: false, supportedTransports: RESPONSES_AND_CHAT_TRANSPORTS, defaultTransport: "responses", supportedAuthSources: API_KEY_ONLY_AUTH_SOURCES, defaultAuthSource: "api_key", supportsResponseContinuity: true },
];

let hydratedProviderDefinitions: ProviderDefinition[] = PROVIDER_DEFINITIONS;

export interface ProviderCatalogDiagnostic {
  path: string;
  provider_id?: string | null;
  field?: string | null;
  message: string;
}

export interface ProviderCatalogHydrationResult {
  diagnostics: ProviderCatalogDiagnostic[];
}

function normalizeProviderCatalogModel(raw: any): ModelDefinition | null {
  if (!raw || typeof raw.id !== "string") {
    return null;
  }
  return {
    id: raw.id,
    name: typeof raw.name === "string" && raw.name.trim() ? raw.name : raw.id,
    contextWindow:
      typeof raw.context_window === "number" && Number.isFinite(raw.context_window)
        ? Math.max(0, Math.trunc(raw.context_window))
        : 0,
    modalities: Array.isArray(raw.modalities) ? raw.modalities.filter((item: unknown): item is Modality => item === "text" || item === "image" || item === "video" || item === "audio" || item === "embedding") : ["text"],
  };
}

function normalizeProviderCatalogEntry(raw: any): ProviderDefinition | null {
  if (!raw || typeof raw.id !== "string" || typeof raw.name !== "string") {
    return null;
  }
  const supportedTransports = Array.isArray(raw.supported_transports)
    ? raw.supported_transports.filter((item: unknown): item is ApiTransportMode => item === "native_assistant" || item === "responses" || item === "anthropic_messages" || item === "chat_completions")
    : CHAT_ONLY_TRANSPORTS;
  const defaultTransport = supportedTransports.includes(raw.default_transport)
    ? raw.default_transport
    : supportedTransports[0] ?? "chat_completions";
  const supportedAuthSources = Array.isArray(raw.supported_auth_sources)
    ? raw.supported_auth_sources.filter((item: unknown): item is AuthSource => item === "api_key" || item === "chatgpt_subscription" || item === "github_copilot")
    : API_KEY_ONLY_AUTH_SOURCES;
  const defaultAuthSource = supportedAuthSources.includes(raw.default_auth_source)
    ? raw.default_auth_source
    : supportedAuthSources[0] ?? "api_key";

  return {
    id: raw.id,
    name: raw.name,
    defaultBaseUrl: typeof raw.default_base_url === "string" ? raw.default_base_url : "",
    defaultModel: typeof raw.default_model === "string" ? raw.default_model : "",
    apiType: raw.api_type === "anthropic" ? "anthropic" : "openai",
    authMethod: raw.auth_method === "x-api-key" ? "x-api-key" : "bearer",
    models: Array.isArray(raw.models) ? raw.models.map(normalizeProviderCatalogModel).filter(Boolean) as ModelDefinition[] : [],
    supportsModelFetch: Boolean(raw.supports_model_fetch),
    anthropicBaseUrl: typeof raw.anthropic_base_url === "string" ? raw.anthropic_base_url : undefined,
    supportedTransports,
    defaultTransport,
    supportedAuthSources,
    defaultAuthSource,
    nativeTransportKind: raw.native_transport_kind === "alibaba_assistant_api" || raw.native_transport_kind === "cursor_cli"
      ? raw.native_transport_kind
      : undefined,
    nativeBaseUrl: typeof raw.native_base_url === "string" ? raw.native_base_url : undefined,
    supportsResponseContinuity: Boolean(raw.supports_response_continuity),
  };
}

export function hydrateProviderDefinitionsFromCatalog(raw: unknown): ProviderCatalogHydrationResult {
  const catalog = raw && typeof raw === "object" ? raw as any : {};
  const providers = Array.isArray(catalog.providers)
    ? catalog.providers.map(normalizeProviderCatalogEntry).filter(Boolean) as ProviderDefinition[]
    : [];
  if (providers.length > 0) {
    hydratedProviderDefinitions = providers;
  }
  const diagnostics = Array.isArray(catalog.custom_provider_report?.diagnostics)
    ? catalog.custom_provider_report.diagnostics as ProviderCatalogDiagnostic[]
    : [];
  return { diagnostics };
}

export function getProviderDefinition(id: AgentProviderId): ProviderDefinition | undefined {
  return hydratedProviderDefinitions.find((provider) => provider.id === id);
}

export function getSupportedApiTransports(providerId: AgentProviderId): ApiTransportMode[] {
  return getProviderDefinition(providerId)?.supportedTransports ?? CHAT_ONLY_TRANSPORTS;
}

export function getDefaultApiTransport(providerId: AgentProviderId): ApiTransportMode {
  return getProviderDefinition(providerId)?.defaultTransport ?? "chat_completions";
}

export function getSupportedAuthSources(
  providerId: AgentProviderId,
  options?: AuthSourceSupportOptions,
): AuthSource[] {
  const supported = getProviderDefinition(providerId)?.supportedAuthSources ?? API_KEY_ONLY_AUTH_SOURCES;
  if (providerId === "openai" && options?.daemonOwnedAuthAvailable === false) {
    return supported.filter((source) => source !== "chatgpt_subscription");
  }
  return supported;
}

export function getDefaultAuthSource(
  providerId: AgentProviderId,
  options?: AuthSourceSupportOptions,
): AuthSource {
  const defaultSource = getProviderDefinition(providerId)?.defaultAuthSource ?? "api_key";
  return getSupportedAuthSources(providerId, options).includes(defaultSource)
    ? defaultSource
    : "api_key";
}

export function getProviderModels(
  providerId: AgentProviderId,
  auth_source?: AuthSource,
): ModelDefinition[] {
  if (providerId === "openai" && auth_source === "chatgpt_subscription") {
    return OPENAI_CHATGPT_SUBSCRIPTION_MODELS;
  }
  return getProviderDefinition(providerId)?.models ?? [];
}

export function getDefaultModelForProvider(
  providerId: AgentProviderId,
  auth_source?: AuthSource,
): string {
  const models = getProviderModels(providerId, auth_source);
  if (models.length > 0) {
    return models[0].id;
  }
  return getProviderDefinition(providerId)?.defaultModel ?? "";
}

export function getModelDefinition(
  providerId: AgentProviderId,
  modelId: string,
  auth_source?: AuthSource,
): ModelDefinition | undefined {
  const trimmed = modelId.trim();
  if (!trimmed) {
    return undefined;
  }
  return getProviderModels(providerId, auth_source).find((model) => model.id === trimmed);
}

export function resolveProviderModelDefinition(
  providerId: AgentProviderId,
  auth_source: AuthSource | undefined,
  ...candidates: Array<string | undefined>
): ModelDefinition | undefined {
  const normalizedCandidates = candidates
    .map((candidate) => normalizeModelLookupValue(candidate))
    .filter((candidate, index, all) => candidate.length > 0 && all.indexOf(candidate) === index);

  if (normalizedCandidates.length === 0) {
    return undefined;
  }

  const models = [
    ...getProviderModels(providerId, auth_source),
    ...getCompatibilityModels(providerId),
  ];

  return models.find((model) => {
    const normalizedId = normalizeModelLookupValue(model.id);
    const normalizedName = normalizeModelLookupValue(model.name);
    return normalizedCandidates.includes(normalizedId)
      || normalizedCandidates.includes(normalizedName);
  });
}

export function modelUsesContextWindowOverride(
  providerId: AgentProviderId,
  modelId: string,
  custom_model_name?: string,
  auth_source?: AuthSource,
): boolean {
  if (providerId === "custom") {
    return true;
  }

  return Boolean(
    (modelId.trim().length > 0 || (custom_model_name ?? "").trim().length > 0)
    && !resolveProviderModelDefinition(providerId, auth_source, modelId, custom_model_name),
  );
}

export function getEffectiveContextWindow(
  providerId: AgentProviderId,
  config: Pick<AgentProviderConfig, "model" | "custom_model_name" | "context_window_tokens" | "auth_source">,
): number {
  const resolvedModel = resolveProviderModelDefinition(
    providerId,
    config.auth_source,
    config.model,
    config.custom_model_name,
  );
  if (resolvedModel) {
    return resolvedModel.contextWindow;
  }

  if (typeof config.context_window_tokens === "number" && config.context_window_tokens > 0) {
    return Math.max(1000, Math.trunc(config.context_window_tokens));
  }

  return providerId === "custom"
    ? DEFAULT_CUSTOM_MODEL_CONTEXT_WINDOW
    : DEFAULT_PROVIDER_CONTEXT_WINDOW;
}

export function providerSupportsResponseContinuity(providerId: AgentProviderId): boolean {
  return Boolean(getProviderDefinition(providerId)?.supportsResponseContinuity);
}

export function providerUsesConfigurableBaseUrl(providerId: AgentProviderId): boolean {
  return providerId === "custom" || providerId === "azure-openai";
}

function isAlibabaCodingPlanAnthropicBaseUrl(baseUrl: string): boolean {
  const lower = (baseUrl || "").trim().toLowerCase();
  return lower.includes("dashscope.aliyuncs.com") && lower.includes("/apps/anthropic");
}

function isAlibabaTokenPlanAnthropicBaseUrl(baseUrl: string): boolean {
  const lower = (baseUrl || "").trim().toLowerCase();
  return lower.includes("maas.aliyuncs.com") && lower.includes("/apps/anthropic");
}

export function getProviderApiType(
  providerId: AgentProviderId,
  model: string,
  configuredUrl: string = "",
): ApiType {
  const definition = getProviderDefinition(providerId);
  if (!definition) {
    return "openai";
  }
  if (providerId === "alibaba-coding-plan" && isAlibabaCodingPlanAnthropicBaseUrl(configuredUrl)) {
    return "anthropic";
  }
  if (providerId === "alibaba-token-plan" && isAlibabaTokenPlanAnthropicBaseUrl(configuredUrl)) {
    return "anthropic";
  }
  if (definition.anthropicBaseUrl && model.startsWith("claude")) {
    return "anthropic";
  }
  if ((providerId === "opencode-zen" || providerId === "opencode-go") && !model.startsWith("claude")) {
    return "openai";
  }
  return definition.apiType;
}

export function getProviderBaseUrl(
  providerId: AgentProviderId,
  model: string,
  configuredUrl: string,
): string {
  if (configuredUrl) {
    return configuredUrl;
  }

  const definition = getProviderDefinition(providerId);
  if (!definition) {
    return configuredUrl;
  }
  if (definition.anthropicBaseUrl && model.startsWith("claude")) {
    return definition.anthropicBaseUrl;
  }
  return definition.defaultBaseUrl;
}
