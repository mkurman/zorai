import { describe, expect, it } from "vitest";
import { buildDaemonAgentConfig } from "./agentDaemonConfig";
import { DEFAULT_AGENT_SETTINGS } from "./agentStore/settings";

describe("daemon agent config audio wiring", () => {
  it("serializes web search provider settings", () => {
    const daemonConfig = buildDaemonAgentConfig({
      ...DEFAULT_AGENT_SETTINGS,
      enable_web_search_tool: true,
      search_provider: "duckduckgo",
      duckduckgo_region: "pl-pl",
      duckduckgo_safe_search: "off",
      firecrawl_api_key: "fc-key",
      exa_api_key: "exa-key",
      tavily_api_key: "tavily-key",
      search_max_results: 12,
      search_timeout_secs: 45,
    });

    expect(daemonConfig.tools.web_search).toBe(true);
    expect(daemonConfig.search_provider).toBe("duckduckgo");
    expect(daemonConfig.duckduckgo_region).toBe("pl-pl");
    expect(daemonConfig.duckduckgo_safe_search).toBe("off");
    expect(daemonConfig.firecrawl_api_key).toBe("fc-key");
    expect(daemonConfig.exa_api_key).toBe("exa-key");
    expect(daemonConfig.tavily_api_key).toBe("tavily-key");
    expect(daemonConfig.search_max_results).toBe(12);
    expect(daemonConfig.search_timeout_secs).toBe(45);
  });

  it("always serializes gateway default agents so the daemon receives every routing key, including cleared platform overrides", () => {
    const daemonConfig = buildDaemonAgentConfig({
      ...DEFAULT_AGENT_SETTINGS,
      gateway_default_agent: "swarog",
      telegram_default_agent: "weles",
    });

    expect(daemonConfig.gateway).toMatchObject({
      default_agent: "swarog",
      slack_default_agent: "",
      discord_default_agent: "",
      telegram_default_agent: "weles",
      whatsapp_default_agent: "",
    });
    expect(buildDaemonAgentConfig({ ...DEFAULT_AGENT_SETTINGS, gateway_default_agent: "" }).gateway.default_agent).toBe("rarog");
  });

  it("serializes participant observer restore window settings", () => {
    const daemonConfig = buildDaemonAgentConfig({
      ...DEFAULT_AGENT_SETTINGS,
      participant_observer_restore_window_hours: 12,
      auto_refresh_interval_secs: 120,
    });

    expect(daemonConfig.participant_observer_restore_window_hours).toBe(12);
    expect(daemonConfig.auto_refresh_interval_secs).toBe(120);
  });

  it("serializes canonical nested audio settings and referenced audio providers", () => {
    const daemonConfig = buildDaemonAgentConfig({
      ...DEFAULT_AGENT_SETTINGS,
      active_provider: "openai",
      openai: {
        ...DEFAULT_AGENT_SETTINGS.openai,
        api_key: "sk-active",
      },
      audio_stt_provider: "openai",
      audio_stt_model: "gpt-4o-transcribe",
      audio_stt_language: "pl",
      audio_tts_provider: "custom",
      audio_tts_model: "sonic-voice",
      audio_tts_voice: "alloy",
      custom: {
        ...DEFAULT_AGENT_SETTINGS.custom,
        base_url: "https://audio.example/v1",
        model: "fallback-audio-model",
        api_key: "sk-audio",
      },
    });

    expect(daemonConfig.audio).toEqual({
      stt: {
        enabled: true,
        provider: "openai",
        model: "gpt-4o-transcribe",
        language: "pl",
      },
      tts: {
        enabled: true,
        provider: "custom",
        model: "sonic-voice",
        voice: "alloy",
        auto_speak: false,
      },
    });

    expect(daemonConfig.providers?.openai).toMatchObject({
      base_url: DEFAULT_AGENT_SETTINGS.openai.base_url,
      model: DEFAULT_AGENT_SETTINGS.openai.model,
    });
    expect(daemonConfig.providers?.custom).toMatchObject({
      base_url: "https://audio.example/v1",
      model: "fallback-audio-model",
    });
  });

  it("serializes canonical nested image generation settings and referenced image providers", () => {
    const daemonConfig = buildDaemonAgentConfig({
      ...DEFAULT_AGENT_SETTINGS,
      active_provider: "openai",
      openai: {
        ...DEFAULT_AGENT_SETTINGS.openai,
        api_key: "sk-active",
      },
      image_generation_provider: "xai",
      image_generation_model: "grok-imagine-image",
      xai: {
        ...DEFAULT_AGENT_SETTINGS.xai,
        api_key: "sk-image",
        model: "grok-imagine-image",
      },
    } as any);

    expect(daemonConfig.image).toEqual({
      generation: {
        provider: "xai",
        model: "grok-imagine-image",
      },
    });

    expect(daemonConfig.providers?.xai).toMatchObject({
      base_url: DEFAULT_AGENT_SETTINGS.xai.base_url,
      model: "grok-imagine-image",
    });
  });
});
