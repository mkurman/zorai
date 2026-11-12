import type { AgentProviderId, ModelDefinition, ProviderAuthState } from "../../lib/agentStore";
import {
  AGENT_PROVIDER_IDS,
  getDefaultModelForProvider,
  providerSupportsAudioTool,
} from "../../lib/agentStore";

const OPENAI_STT_MODELS: ModelDefinition[] = [
  { id: "gpt-4o-transcribe", name: "GPT-4o Transcribe", contextWindow: 128000, modalities: ["audio"] },
  { id: "gpt-4o-mini-transcribe", name: "GPT-4o Mini Transcribe", contextWindow: 128000, modalities: ["audio"] },
  { id: "gpt-4o-transcribe-diarize", name: "GPT-4o Transcribe Diarize", contextWindow: 16000, modalities: ["audio"] },
  { id: "whisper-1", name: "Whisper 1", contextWindow: 0, modalities: ["audio"] },
];

const OPENAI_TTS_MODELS: ModelDefinition[] = [
  { id: "gpt-4o-mini-tts", name: "GPT-4o Mini TTS", contextWindow: 128000, modalities: ["audio"] },
  { id: "tts-1", name: "TTS 1", contextWindow: 0, modalities: ["audio"] },
  { id: "tts-1-hd", name: "TTS 1 HD", contextWindow: 0, modalities: ["audio"] },
];

const XAI_AUDIO_MODELS: ModelDefinition[] = [
  { id: "grok-4.3", name: "Grok 4.3", contextWindow: 1_000_000, modalities: ["audio"] },
];

const ELEVENLABS_STT_MODELS: ModelDefinition[] = [
  { id: "scribe_v2", name: "Scribe v2", contextWindow: 0, modalities: ["audio"] },
];

const ELEVENLABS_TTS_MODELS: ModelDefinition[] = [
  { id: "eleven_multilingual_v2", name: "Eleven Multilingual v2", contextWindow: 0, modalities: ["audio"] },
];

const GROQ_STT_MODELS: ModelDefinition[] = [
  { id: "whisper-large-v3-turbo", name: "Whisper Large V3 Turbo", contextWindow: 0, modalities: ["audio"] },
  { id: "whisper-large-v3", name: "Whisper Large V3", contextWindow: 0, modalities: ["audio"] },
];

const GROQ_TTS_MODELS: ModelDefinition[] = [
  { id: "canopylabs/orpheus-v1-english", name: "CanopyLabs Orpheus V1 English", contextWindow: 0, modalities: ["audio"] },
  { id: "canopylabs/orpheus-arabic-saudi", name: "CanopyLabs Orpheus Arabic Saudi", contextWindow: 0, modalities: ["audio"] },
];

const XIAOMI_TTS_MODELS: ModelDefinition[] = [
  { id: "mimo-v2-tts", name: "MiMo V2 TTS", contextWindow: 128000, modalities: ["audio"] },
  { id: "mimo-v2.5-tts", name: "MiMo V2.5 TTS", contextWindow: 128000, modalities: ["audio"] },
  { id: "mimo-v2.5-tts-voiceclone", name: "MiMo V2.5 TTS VoiceClone", contextWindow: 128000, modalities: ["audio"] },
  { id: "mimo-v2.5-tts-voicedesign", name: "MiMo V2.5 TTS VoiceDesign", contextWindow: 128000, modalities: ["audio"] },
];

const XIAOMI_STT_MODELS: ModelDefinition[] = [
  { id: "mimo-v2.5-asr", name: "MiMo V2.5 ASR", contextWindow: 128000, modalities: ["audio"] },
];

const MINIMAX_TTS_MODELS: ModelDefinition[] = [
  { id: "speech-2.8-hd", name: "MiniMax Speech 2.8 HD", contextWindow: 0, modalities: ["audio"] },
  { id: "speech-2.8-turbo", name: "MiniMax Speech 2.8 Turbo", contextWindow: 0, modalities: ["audio"] },
  { id: "speech-2.6-hd", name: "MiniMax Speech 2.6 HD", contextWindow: 0, modalities: ["audio"] },
  { id: "speech-2.6-turbo", name: "MiniMax Speech 2.6 Turbo", contextWindow: 0, modalities: ["audio"] },
  { id: "speech-02-hd", name: "MiniMax Speech 02 HD", contextWindow: 0, modalities: ["audio"] },
  { id: "speech-02-turbo", name: "MiniMax Speech 02 Turbo", contextWindow: 0, modalities: ["audio"] },
  { id: "speech-01-hd", name: "MiniMax Speech 01 HD", contextWindow: 0, modalities: ["audio"] },
  { id: "speech-01-turbo", name: "MiniMax Speech 01 Turbo", contextWindow: 0, modalities: ["audio"] },
];

const OPENAI_IMAGE_MODELS: ModelDefinition[] = [
  { id: "gpt-image-1", name: "GPT Image 1", contextWindow: 0, modalities: ["image"] },
  { id: "gpt-image-2", name: "GPT Image 2", contextWindow: 0, modalities: ["image"] },
];

const OPENROUTER_IMAGE_MODELS: ModelDefinition[] = [
  { id: "openai/gpt-image-1", name: "OpenAI GPT Image 1", contextWindow: 0, modalities: ["image"] },
  { id: "openai/gpt-image-2", name: "OpenAI GPT Image 2", contextWindow: 0, modalities: ["image"] },
];

const MINIMAX_IMAGE_MODELS: ModelDefinition[] = [
  { id: "image-01", name: "MiniMax Image 01", contextWindow: 0, modalities: ["image"] },
];

const MINIMAX_VIDEO_MODELS: ModelDefinition[] = [
  { id: "MiniMax-Hailuo-2.3", name: "MiniMax Hailuo 2.3", contextWindow: 0, modalities: ["video"] },
  { id: "MiniMax-Hailuo-02", name: "MiniMax Hailuo 02", contextWindow: 0, modalities: ["video"] },
  { id: "T2V-01-Director", name: "MiniMax T2V 01 Director", contextWindow: 0, modalities: ["video"] },
  { id: "T2V-01", name: "MiniMax T2V 01", contextWindow: 0, modalities: ["video"] },
];

const QWEN_VIDEO_MODELS: ModelDefinition[] = [
  { id: "wan2.7-t2v", name: "Wan 2.7 T2V", contextWindow: 0, modalities: ["video"] },
  { id: "wan2.6-t2v", name: "Wan 2.6 T2V", contextWindow: 0, modalities: ["video"] },
  { id: "wan2.5-t2v-preview", name: "Wan 2.5 T2V Preview", contextWindow: 0, modalities: ["video"] },
  { id: "wan2.2-t2v-plus", name: "Wan 2.2 T2V Plus", contextWindow: 0, modalities: ["video"] },
];

const GEMINI_VIDEO_MODELS: ModelDefinition[] = [
  { id: "gemini-omni-1.2-flash", name: "Gemini Omni 1.2 Flash", contextWindow: 0, modalities: ["video"] },
  { id: "gemini-omni-1.1-flash", name: "Gemini Omni 1.1 Flash", contextWindow: 0, modalities: ["video"] },
];

const VIDEO_GENERATION_PROVIDER_IDS: AgentProviderId[] = ["minimax", "qwen", "gemini", "openrouter"];

const ELEVENLABS_MUSIC_MODELS: ModelDefinition[] = [
  { id: "music_v2", name: "ElevenLabs Music v2", contextWindow: 0, modalities: ["audio"] },
  { id: "music_v2_5", name: "ElevenLabs Music v2.5", contextWindow: 0, modalities: ["audio"] },
  { id: "music_v1", name: "ElevenLabs Music v1", contextWindow: 0, modalities: ["audio"] },
];

const GEMINI_MUSIC_MODELS: ModelDefinition[] = [
  { id: "lyria-3.5", name: "Lyria 3.5 (full songs)", contextWindow: 0, modalities: ["audio"] },
  { id: "lyria-3-clip-preview", name: "Lyria 3 Clip (30 s)", contextWindow: 0, modalities: ["audio"] },
  { id: "lyria-3-pro-preview", name: "Lyria 3 Pro (full songs)", contextWindow: 0, modalities: ["audio"] },
];

const MINIMAX_MUSIC_MODELS: ModelDefinition[] = [
  { id: "music-3.0", name: "MiniMax Music 3.0", contextWindow: 0, modalities: ["audio"] },
  { id: "music-2.6", name: "MiniMax Music 2.6", contextWindow: 0, modalities: ["audio"] },
  { id: "music-3.0-free", name: "MiniMax Music 3.0 (free tier)", contextWindow: 0, modalities: ["audio"] },
  { id: "music-2.6-free", name: "MiniMax Music 2.6 (free tier)", contextWindow: 0, modalities: ["audio"] },
];

const MUSIC_GENERATION_PROVIDER_IDS: AgentProviderId[] = ["elevenlabs", "gemini", "minimax"];

export type ProviderOption = {
  id: AgentProviderId;
  label: string;
};

function isBuiltInProviderId(providerId: string): boolean {
  return AGENT_PROVIDER_IDS.some((id) => id === providerId);
}

export function providerAuthStateIsSelectable(state: ProviderAuthState): boolean {
  return state.authenticated
    || state.provider_id === "custom"
    || state.provider_id === "azure-openai"
    || !isBuiltInProviderId(state.provider_id);
}

export function selectableProviderAuthStates(
  providerAuthStates: ProviderAuthState[],
): ProviderAuthState[] {
  return providerAuthStates.filter(providerAuthStateIsSelectable);
}

export function buildProviderOptions(
  builtInProviderOptions: readonly ProviderOption[],
  providerAuthStates: ProviderAuthState[],
): { allProviderOptions: ProviderOption[]; providerOptions: ProviderOption[] } {
  const allProviderOptions: ProviderOption[] = [
    ...builtInProviderOptions,
    ...providerAuthStates
      .filter((state) => !builtInProviderOptions.some((provider) => provider.id === state.provider_id))
      .map((state) => ({ id: state.provider_id as AgentProviderId, label: state.provider_name || state.provider_id })),
  ];
  const providerOptions = allProviderOptions.filter((provider) => {
    const authState = providerAuthStates.find((state) => state.provider_id === provider.id);
    if (authState) {
      return providerAuthStateIsSelectable(authState);
    }
    return provider.id === "custom" || provider.id === "azure-openai" || !isBuiltInProviderId(provider.id);
  });

  return { allProviderOptions, providerOptions };
}

export function audioModelOptions(
  providerId: AgentProviderId,
  kind: "stt" | "tts",
): ModelDefinition[] | undefined {
  if (providerId === "openai" || providerId === "azure-openai") {
    return kind === "stt" ? OPENAI_STT_MODELS : OPENAI_TTS_MODELS;
  }
  if (providerId === "xai") {
    return XAI_AUDIO_MODELS;
  }
  if (providerId === "elevenlabs") {
    return kind === "stt" ? ELEVENLABS_STT_MODELS : ELEVENLABS_TTS_MODELS;
  }
  if (providerId === "groq") {
    return kind === "stt" ? GROQ_STT_MODELS : GROQ_TTS_MODELS;
  }
  if ((providerId === "minimax" || providerId === "minimax-coding-plan") && kind === "tts") {
    return MINIMAX_TTS_MODELS;
  }
  if (providerId === "xiaomi-mimo-token-plan") {
    return kind === "stt" ? XIAOMI_STT_MODELS : XIAOMI_TTS_MODELS;
  }
  return undefined;
}

export function normalizeAudioModelForProviderChange(
  providerId: AgentProviderId,
  kind: "stt" | "tts",
  currentModel: string,
): string {
  const normalizedCurrentModel = currentModel.trim();
  const knownAudioModels = audioModelOptions(providerId, kind) ?? [];
  if (knownAudioModels.length > 0 && normalizedCurrentModel.length === 0) {
    return knownAudioModels[0]?.id ?? "";
  }
  if (knownAudioModels.length > 0) {
    return knownAudioModels.some((model) => model.id === normalizedCurrentModel)
      ? normalizedCurrentModel
      : knownAudioModels[0]?.id ?? "";
  }
  if (providerId === "custom") {
    return normalizedCurrentModel || getDefaultModelForProvider(providerId);
  }
  return "";
}

export function filterAudioProviderOptions(
  providerOptions: ProviderOption[],
  kind: "stt" | "tts",
): ProviderOption[] {
  return providerOptions.filter((provider) => providerSupportsAudioTool(provider.id, kind));
}

export function audioRemoteModelFetchOutputModalities(
  endpoint: "stt" | "tts",
  providerId: AgentProviderId,
): string | undefined {
  if (providerId !== "openrouter") {
    return undefined;
  }
  if (endpoint === "tts") return "speech";
  if (endpoint === "stt") return "transcription";
  return undefined;
}

export function imageRemoteModelFetchOutputModalities(providerId: AgentProviderId): string | undefined {
  return providerId === "openrouter" ? "image" : undefined;
}

export function imageGenerationModelOptions(providerId: AgentProviderId): ModelDefinition[] {
  if (providerId === "openai" || providerId === "azure-openai" || providerId === "custom") {
    return OPENAI_IMAGE_MODELS;
  }
  if (providerId === "openrouter") {
    return OPENROUTER_IMAGE_MODELS;
  }
  if (providerId === "minimax" || providerId === "minimax-coding-plan") {
    return MINIMAX_IMAGE_MODELS;
  }
  return [];
}

export function videoRemoteModelFetchOutputModalities(providerId: AgentProviderId): string | undefined {
  return providerId === "openrouter" ? "video" : undefined;
}

export function videoGenerationModelOptions(providerId: AgentProviderId): ModelDefinition[] {
  if (providerId === "minimax") {
    return MINIMAX_VIDEO_MODELS;
  }
  if (providerId === "qwen") {
    return QWEN_VIDEO_MODELS;
  }
  if (providerId === "gemini") {
    return GEMINI_VIDEO_MODELS;
  }
  return [];
}

export function embeddingModelOptions(providerId: AgentProviderId): ModelDefinition[] {
  if (providerId === "openai" || providerId === "azure-openai" || providerId === "custom") {
    return [
      { id: "text-embedding-3-small", name: "Text Embedding 3 Small", contextWindow: 8192, modalities: ["embedding"] },
      { id: "text-embedding-3-large", name: "Text Embedding 3 Large", contextWindow: 8192, modalities: ["embedding"] },
    ];
  }
  if (providerId === "openrouter") {
    return [
      { id: "openai/text-embedding-3-small", name: "OpenAI Text Embedding 3 Small", contextWindow: 8192, modalities: ["embedding"] },
      { id: "openai/text-embedding-3-large", name: "OpenAI Text Embedding 3 Large", contextWindow: 8192, modalities: ["embedding"] },
    ];
  }
  return [];
}

export function normalizeEmbeddingModelForProviderChange(
  providerId: AgentProviderId,
  currentModel: string,
): string {
  const normalizedCurrentModel = currentModel.trim();
  const knownEmbeddingModels = embeddingModelOptions(providerId);
  if (knownEmbeddingModels.length > 0 && normalizedCurrentModel.length === 0) {
    return knownEmbeddingModels[0]?.id ?? "";
  }
  if (knownEmbeddingModels.length > 0) {
    return knownEmbeddingModels.some((model) => model.id === normalizedCurrentModel)
      ? normalizedCurrentModel
      : knownEmbeddingModels[0]?.id ?? "";
  }
  return "";
}

export function filterEmbeddingProviderOptions(providerOptions: ProviderOption[]): ProviderOption[] {
  return providerOptions.filter((provider) => embeddingModelOptions(provider.id).length > 0);
}

export function normalizeImageGenerationModelForProviderChange(
  providerId: AgentProviderId,
  currentModel: string,
): string {
  const normalizedCurrentModel = currentModel.trim();
  const knownImageModels = imageGenerationModelOptions(providerId);
  if (knownImageModels.length > 0 && normalizedCurrentModel.length === 0) {
    return knownImageModels[0]?.id ?? "";
  }
  if (knownImageModels.length > 0) {
    return knownImageModels.some((model) => model.id === normalizedCurrentModel)
      ? normalizedCurrentModel
      : knownImageModels[0]?.id ?? "";
  }
  if (providerId === "custom") {
    return normalizedCurrentModel || "gpt-image-1";
  }
  return "";
}

export function filterImageGenerationProviderOptions(providerOptions: ProviderOption[]): ProviderOption[] {
  return providerOptions.filter((provider) =>
    provider.id === "custom" || imageGenerationModelOptions(provider.id).length > 0);
}

export function normalizeVideoGenerationModelForProviderChange(
  providerId: AgentProviderId,
  currentModel: string,
): string {
  const normalizedCurrentModel = currentModel.trim();
  const knownVideoModels = videoGenerationModelOptions(providerId);
  if (knownVideoModels.length > 0) {
    return knownVideoModels.some((model) => model.id === normalizedCurrentModel)
      ? normalizedCurrentModel
      : knownVideoModels[0]?.id ?? "";
  }
  if (providerId === "openrouter") {
    return normalizedCurrentModel;
  }
  return "";
}

export function filterVideoGenerationProviderOptions(providerOptions: ProviderOption[]): ProviderOption[] {
  return providerOptions.filter((provider) => VIDEO_GENERATION_PROVIDER_IDS.includes(provider.id));
}

export function musicGenerationModelOptions(providerId: AgentProviderId): ModelDefinition[] {
  if (providerId === "elevenlabs") return ELEVENLABS_MUSIC_MODELS;
  if (providerId === "gemini") return GEMINI_MUSIC_MODELS;
  if (providerId === "minimax") return MINIMAX_MUSIC_MODELS;
  return [];
}

export function normalizeMusicGenerationModelForProviderChange(providerId: AgentProviderId, currentModel: string): string {
  const knownMusicModels = musicGenerationModelOptions(providerId);
  const normalizedCurrentModel = currentModel.trim();
  return knownMusicModels.some((model) => model.id === normalizedCurrentModel)
    ? normalizedCurrentModel
    : knownMusicModels[0]?.id ?? "";
}

export function filterMusicGenerationProviderOptions(providerOptions: ProviderOption[]): ProviderOption[] {
  return providerOptions.filter((provider) => MUSIC_GENERATION_PROVIDER_IDS.includes(provider.id));
}

export function normalizeLlmStreamTimeoutInput(value: string): number | null {
  const parsed = Number.parseInt(value, 10);
  if (Number.isNaN(parsed)) {
    return null;
  }
  return Math.min(1800, Math.max(30, parsed));
}
