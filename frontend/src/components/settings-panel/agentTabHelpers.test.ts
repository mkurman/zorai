import { expect, test } from "vitest";
import {
  audioModelOptions,
  audioRemoteModelFetchOutputModalities,
  embeddingModelOptions,
  filterImageGenerationProviderOptions,
  imageGenerationModelOptions,
  imageRemoteModelFetchOutputModalities,
  normalizeImageGenerationModelForProviderChange,
  normalizeAudioModelForProviderChange,
  filterVideoGenerationProviderOptions,
  normalizeVideoGenerationModelForProviderChange,
  videoGenerationModelOptions,
  videoRemoteModelFetchOutputModalities,
  filterMusicGenerationProviderOptions,
  normalizeMusicGenerationModelForProviderChange,
} from "./agentTabHelpers";

test("image generation provider changes normalize unsupported models", () => {
  expect(normalizeImageGenerationModelForProviderChange("openai", "whisper-1")).toBe("gpt-image-1");
  expect(normalizeImageGenerationModelForProviderChange("openrouter", "")).toBe("openai/gpt-image-1");
  expect(normalizeImageGenerationModelForProviderChange("openai", "gpt-image-2")).toBe("gpt-image-2");
  expect(normalizeImageGenerationModelForProviderChange("openrouter", "openai/gpt-image-2")).toBe("openai/gpt-image-2");
});

test("image generation model options keep only image-capable provider models", () => {
  expect(imageGenerationModelOptions("openai").map((model) => model.id)).toContain("gpt-image-1");
  expect(imageGenerationModelOptions("openai").map((model) => model.id)).toContain("gpt-image-2");
  expect(imageGenerationModelOptions("openrouter").map((model) => model.id)).toContain("openai/gpt-image-1");
  expect(imageGenerationModelOptions("openrouter").map((model) => model.id)).toContain("openai/gpt-image-2");
  expect(imageGenerationModelOptions("minimax").map((model) => model.id)).toContain("image-01");
  expect(imageGenerationModelOptions("minimax-coding-plan").map((model) => model.id)).toContain("image-01");
  expect(imageGenerationModelOptions("featherless").length).toBe(0);
});

test("image generation provider options keep only providers with image-capable models", () => {
  expect(
    filterImageGenerationProviderOptions([
      { id: "featherless", label: "Featherless" },
      { id: "openai", label: "OpenAI" },
      { id: "openrouter", label: "OpenRouter" },
      { id: "minimax", label: "MiniMax" },
      { id: "minimax-coding-plan", label: "MiniMax Coding Plan" },
      { id: "custom", label: "Custom" },
    ]).map((provider) => provider.id),
  ).toEqual(["openai", "openrouter", "minimax", "minimax-coding-plan", "custom"]);
});

test("minimax audio model options expose tts only catalogs", () => {
  expect(audioModelOptions("minimax", "tts")?.map((model) => model.id)).toContain("speech-2.8-hd");
  expect(audioModelOptions("minimax-coding-plan", "tts")?.map((model) => model.id)).toContain("speech-2.8-turbo");
  expect(audioModelOptions("minimax", "stt")).toBeUndefined();
});

test("audio model options match tui static catalogs", () => {
  expect(audioModelOptions("openai", "stt")?.map((model) => model.id)).toEqual([
    "gpt-4o-transcribe",
    "gpt-4o-mini-transcribe",
    "gpt-4o-transcribe-diarize",
    "whisper-1",
  ]);
  expect(audioModelOptions("groq", "stt")?.map((model) => model.id)).toEqual([
    "whisper-large-v3-turbo",
    "whisper-large-v3",
  ]);
  expect(audioModelOptions("groq", "tts")?.map((model) => model.id)).toEqual([
    "canopylabs/orpheus-v1-english",
    "canopylabs/orpheus-arabic-saudi",
  ]);
  expect(audioModelOptions("elevenlabs", "stt")?.map((model) => model.id)).toEqual([
    "scribe_v2",
  ]);
  expect(audioModelOptions("elevenlabs", "tts")?.map((model) => model.id)).toEqual([
    "eleven_multilingual_v2",
  ]);
});

test("minimax provider changes normalize tts defaults and image defaults", () => {
  expect(normalizeAudioModelForProviderChange("minimax", "tts", "")).toBe("speech-2.8-hd");
  expect(normalizeAudioModelForProviderChange("minimax-coding-plan", "tts", "speech-2.6-turbo")).toBe("speech-2.6-turbo");
  expect(normalizeAudioModelForProviderChange("minimax", "stt", "whisper-1")).toBe("");
  expect(normalizeImageGenerationModelForProviderChange("minimax", "")).toBe("image-01");
  expect(normalizeImageGenerationModelForProviderChange("minimax-coding-plan", "image-01")).toBe("image-01");
});

test("elevenlabs provider changes normalize stt and tts defaults", () => {
  expect(normalizeAudioModelForProviderChange("elevenlabs", "stt", "")).toBe("scribe_v2");
  expect(normalizeAudioModelForProviderChange("elevenlabs", "tts", "")).toBe("eleven_multilingual_v2");
  expect(normalizeAudioModelForProviderChange("elevenlabs", "stt", "whisper-1")).toBe("scribe_v2");
  expect(normalizeAudioModelForProviderChange("elevenlabs", "tts", "tts-1")).toBe("eleven_multilingual_v2");
});

test("embedding model options match tui static catalogs", () => {
  expect(embeddingModelOptions("openai").map((model) => model.id)).toEqual([
    "text-embedding-3-small",
    "text-embedding-3-large",
  ]);
  expect(embeddingModelOptions("azure-openai").map((model) => model.id)).toEqual([
    "text-embedding-3-small",
    "text-embedding-3-large",
  ]);
  expect(embeddingModelOptions("custom").map((model) => model.id)).toEqual([
    "text-embedding-3-small",
    "text-embedding-3-large",
  ]);
  expect(embeddingModelOptions("openrouter").map((model) => model.id)).toEqual([
    "openai/text-embedding-3-small",
    "openai/text-embedding-3-large",
  ]);
});

test("remote model fetches request TUI modality filters for OpenRouter audio and image pickers", () => {
  expect(audioRemoteModelFetchOutputModalities("stt", "openrouter")).toBe("transcription");
  expect(audioRemoteModelFetchOutputModalities("tts", "openrouter")).toBe("speech");
  expect(audioRemoteModelFetchOutputModalities("stt", "openai")).toBeUndefined();
  expect(audioRemoteModelFetchOutputModalities("tts", "openai")).toBeUndefined();
  expect(imageRemoteModelFetchOutputModalities("openrouter")).toBe("image");
  expect(imageRemoteModelFetchOutputModalities("openai")).toBeUndefined();
});

test("video generation catalogs list only daemon-supported models with the daemon default first", () => {
  expect(videoGenerationModelOptions("minimax").map((model) => model.id)).toEqual([
    "MiniMax-Hailuo-2.3",
    "MiniMax-Hailuo-02",
    "T2V-01-Director",
    "T2V-01",
  ]);
  expect(videoGenerationModelOptions("qwen").map((model) => model.id)).toEqual([
    "wan2.7-t2v",
    "wan2.6-t2v",
    "wan2.5-t2v-preview",
    "wan2.2-t2v-plus",
  ]);
  expect(videoGenerationModelOptions("gemini").map((model) => model.id)).toEqual(["gemini-omni-1.1-flash"]);
  expect(videoGenerationModelOptions("openai")).toEqual([]);
  expect(videoGenerationModelOptions("openrouter")).toEqual([]);
});

test("video generation provider options exclude providers the daemon cannot generate video with", () => {
  expect(
    filterVideoGenerationProviderOptions([
      { id: "openai", label: "OpenAI" },
      { id: "minimax", label: "MiniMax" },
      { id: "qwen", label: "Qwen" },
      { id: "gemini", label: "Gemini" },
      { id: "openrouter", label: "OpenRouter" },
      { id: "custom", label: "Custom" },
      { id: "xai", label: "xAI" },
    ]).map((provider) => provider.id),
  ).toEqual(["minimax", "qwen", "gemini", "openrouter"]);
});

test("video generation provider changes pick a valid catalog model and keep openrouter custom ids", () => {
  expect(normalizeVideoGenerationModelForProviderChange("minimax", "")).toBe("MiniMax-Hailuo-2.3");
  expect(normalizeVideoGenerationModelForProviderChange("qwen", "MiniMax-Hailuo-2.3")).toBe("wan2.7-t2v");
  expect(normalizeVideoGenerationModelForProviderChange("qwen", "wan2.6-t2v")).toBe("wan2.6-t2v");
  expect(normalizeVideoGenerationModelForProviderChange("openrouter", "vendor/video-model")).toBe("vendor/video-model");
  expect(normalizeVideoGenerationModelForProviderChange("", "MiniMax-Hailuo-2.3")).toBe("");
});

test("video remote model fetch asks openrouter for video output models only", () => {
  expect(videoRemoteModelFetchOutputModalities("openrouter")).toBe("video");
  expect(videoRemoteModelFetchOutputModalities("minimax")).toBeUndefined();
});

test("music generation offers only providers the daemon can call and defaults to their full-song model", () => {
  expect(
    filterMusicGenerationProviderOptions([
      { id: "openai", label: "OpenAI" },
      { id: "elevenlabs", label: "ElevenLabs" },
      { id: "gemini", label: "Gemini" },
      { id: "minimax", label: "MiniMax" },
    ]).map((provider) => provider.id),
  ).toEqual(["elevenlabs", "gemini", "minimax"]);
  expect(normalizeMusicGenerationModelForProviderChange("elevenlabs", "")).toBe("music_v2");
  expect(normalizeMusicGenerationModelForProviderChange("minimax", "lyria-3.5")).toBe("music-3.0");
  expect(normalizeMusicGenerationModelForProviderChange("gemini", "music_v2")).toBe("lyria-3.5");
  expect(normalizeMusicGenerationModelForProviderChange("gemini", "lyria-3-clip-preview")).toBe("lyria-3-clip-preview");
  expect(normalizeMusicGenerationModelForProviderChange("", "music_v2")).toBe("");
});
