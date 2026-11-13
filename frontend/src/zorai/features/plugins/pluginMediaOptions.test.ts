import { describe, expect, it } from "vitest";
import { mediaOptions } from "./pluginMediaOptions";

const provider = (extra: Record<string, unknown> = {}) => ({ model: "m", base_url: "https://x", api_key: "", auth_source: "api_key", ...extra });

describe("media options offered to plugin panels", () => {
  const settings = {
    active_provider: "openai",
    openai: provider({ api_key: "sk-1", model: "gpt-5.4" }),
    openrouter: provider(),
    image_generation_provider: "openai",
    image_generation_model: "gpt-image-1",
    audio_tts_provider: "openrouter",
    audio_tts_model: "x-ai/grok-voice-tts-1.0",
    audio_tts_voice: "eve",
  };

  it("lists only providers that can make images, ready ones first, with their known models", () => {
    const options = mediaOptions("image", settings, []);
    expect(options.current).toEqual({ provider: "openai", model: "gpt-image-1" });
    expect(options.providers.map((entry) => [entry.id, entry.ready])).toEqual([["openai", true], ["openrouter", false]]);
    expect(options.providers[0].models.some((model) => model.id === "gpt-image-1")).toBe(true);
  });

  it("counts a provider signed in through the daemon as ready even without an API key", () => {
    const options = mediaOptions("image", settings, [{ provider_id: "openrouter", authenticated: true }]);
    expect(options.providers.every((entry) => entry.ready)).toBe(true);
  });

  it("reports the voice for speech so panels can show what cloud narration will sound like", () => {
    expect(mediaOptions("tts", settings, []).current).toEqual({ provider: "openrouter", model: "x-ai/grok-voice-tts-1.0", voice: "eve" });
  });
});
