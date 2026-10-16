import { expect, test } from "vitest";
import { buildDaemonAgentConfig } from "../agentDaemonConfig";
import {
  DEFAULT_AGENT_SETTINGS,
  normalizeAgentBackend,
  normalizeAgentSettingsFromSource,
} from "./settings";

test("Muse defaults use Model API settings", () => {
  expect(DEFAULT_AGENT_SETTINGS.muse).toMatchObject({
    base_url: "https://api.meta.ai/v1",
    model: "muse-spark-1.3",
    api_transport: "responses",
  });
});

test("xAI defaults use hosted settings", () => {
  expect(DEFAULT_AGENT_SETTINGS.xai).toMatchObject({
    base_url: "https://api.x.ai/v1",
    model: "grok-4.7",
    api_transport: "responses",
  });
});

test("agent runtime is daemon-only and legacy/external backend values normalize away", () => {
  expect(normalizeAgentBackend("daemon")).toBe("daemon");
  expect(normalizeAgentBackend("legacy")).toBe("daemon");
  expect(normalizeAgentBackend("hermes")).toBe("daemon");
  expect(normalizeAgentBackend("openclaw")).toBe("daemon");

  const normalized = normalizeAgentSettingsFromSource({
    agent_backend: "openclaw",
  } as any);
  expect(normalized.agent_backend).toBe("daemon");
  expect(buildDaemonAgentConfig(normalized).agent_backend).toBe("daemon");
});

test("xAI settings normalize and serialize for audio flows", () => {
  const normalized = normalizeAgentSettingsFromSource({
    active_provider: "xai",
    xai: {
      base_url: "https://api.x.ai/v1",
      model: "grok-4-voice",
      api_key: "sk-xai",
      api_transport: "chat_completions",
    },
    audio_stt_provider: "xai",
    audio_stt_model: "grok-4-voice",
    audio_tts_provider: "xai",
    audio_tts_model: "grok-4-voice",
  });

  expect(normalized.active_provider).toBe("xai");
  expect(normalized.audio_stt_provider).toBe("xai");
  expect(normalized.audio_tts_provider).toBe("xai");
  expect(normalized.xai).toMatchObject({
    base_url: "https://api.x.ai/v1",
    model: "grok-4-voice",
    api_key: "sk-xai",
    api_transport: "chat_completions",
  });

  const daemonConfig = buildDaemonAgentConfig({
    ...DEFAULT_AGENT_SETTINGS,
    active_provider: "xai",
    xai: {
      ...DEFAULT_AGENT_SETTINGS.xai,
      api_key: "sk-xai",
      model: "grok-4-voice",
    },
    audio_stt_provider: "xai",
    audio_stt_model: "grok-4-voice",
    audio_tts_provider: "xai",
    audio_tts_model: "grok-4-voice",
  });

  expect(daemonConfig.providers?.xai).toMatchObject({
    base_url: "https://api.x.ai/v1",
    model: "grok-4-voice",
  });
});

test("auxiliary providers inherit OpenRouter when primary provider is OpenRouter", () => {
  const normalized = normalizeAgentSettingsFromSource({
    active_provider: "openrouter",
    openrouter: {
      ...DEFAULT_AGENT_SETTINGS.openrouter,
      api_key: "sk-or",
    },
  } as any);

  expect(normalized.audio_stt_provider).toBe("openrouter");
  expect(normalized.audio_tts_provider).toBe("openrouter");
  expect(normalized.image_generation_provider).toBe("openrouter");
  expect(normalized.semantic_embedding_provider).toBe("openrouter");
  expect(normalized.image_generation_model).toBe("openai/gpt-image-1");
  expect(normalized.semantic_embedding_model).toBe("openai/text-embedding-3-small");
});

test("image generation settings prefer nested daemon config over legacy flat keys", () => {
  const normalized = normalizeAgentSettingsFromSource({
    image: {
      generation: {
        provider: "openai",
        model: "gpt-image-1",
      },
    },
    image_generation_provider: "xai",
    image_generation_model: "grok-imagine-image",
  } as any);

  expect(normalized.image_generation_provider).toBe("openai");
  expect(normalized.image_generation_model).toBe("gpt-image-1");
});

test("participant observer restore window defaults and preserves disabled sentinel", () => {
  expect(DEFAULT_AGENT_SETTINGS.participant_observer_restore_window_hours).toBe(24);

  const normalized = normalizeAgentSettingsFromSource({
    participant_observer_restore_window_hours: 0,
  });

  expect(normalized.participant_observer_restore_window_hours).toBe(0);
});

test("auto thread title defaults to off and rejects unknown providers", () => {
  expect(DEFAULT_AGENT_SETTINGS.auto_thread_title).toBe("off");
  expect(normalizeAgentSettingsFromSource({ auto_thread_title: "rarog" } as any).auto_thread_title).toBe("rarog");
  expect(normalizeAgentSettingsFromSource({ auto_thread_title: "WELES" } as any).auto_thread_title).toBe("weles");
  expect(normalizeAgentSettingsFromSource({ auto_thread_title: "mystery" } as any).auto_thread_title).toBe("off");
  expect(buildDaemonAgentConfig(DEFAULT_AGENT_SETTINGS).auto_thread_title).toBe("off");
});
