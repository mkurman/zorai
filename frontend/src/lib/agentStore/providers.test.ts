import { describe, expect, it } from "vitest";

import {
  getEffectiveContextWindow,
  getDefaultModelForProvider,
  getProviderApiType,
  getProviderDefinition,
  getProviderModels,
  getModelDefinition,
  getModelModalities,
  normalizeApiTransport,
  normalizeAgentProviderId,
  normalizeProviderConfig,
  providerSupportsAudioTool,
} from "./providers.ts";
import { buildDaemonAgentConfig } from "../agentDaemonConfig.ts";
import { normalizeAudioModelForProviderChange } from "../../components/settings-panel/agentTabHelpers.ts";
import { DEFAULT_AGENT_SETTINGS, normalizeAgentSettingsFromSource } from "./settings.ts";

describe("frontend NVIDIA provider catalog", () => {
  it("registers NVIDIA with hosted defaults and fetch support", () => {
    const nvidia = getProviderDefinition("nvidia");

    expect(nvidia).toBeDefined();
    expect(nvidia?.defaultBaseUrl).toBe("https://integrate.api.nvidia.com/v1");
    expect(nvidia?.defaultModel).toBe("minimaxai/minimax-m2.7");
    expect(nvidia?.supportsModelFetch).toBe(true);
  });

  it("recognizes NVIDIA as a valid provider id", () => {
    expect(normalizeAgentProviderId("nvidia")).toBe("nvidia");
    expect(getDefaultModelForProvider("nvidia")).toBe("minimaxai/minimax-m2.7");
  });
});

describe("frontend Poolside provider catalog", () => {
  it("registers Poolside with fetchable chat-completion defaults", () => {
    const poolside = getProviderDefinition("poolside");

    expect(poolside).toBeDefined();
    expect(poolside?.defaultBaseUrl).toBe("https://inference.poolside.ai/v1");
    expect(poolside?.defaultModel).toBe("poolside/laguna-m.1");
    expect(poolside?.supportsModelFetch).toBe(true);
    expect(poolside?.defaultTransport).toBe("chat_completions");
    expect(poolside?.supportedTransports).toEqual(["chat_completions"]);
    expect(poolside?.models.map((model) => [model.id, model.contextWindow])).toEqual([
      ["poolside/laguna-m.1", 256_000],
    ]);
    expect(DEFAULT_AGENT_SETTINGS.poolside.model).toBe("poolside/laguna-m.1");
  });

  it("recognizes Poolside as a valid provider id", () => {
    expect(normalizeAgentProviderId("poolside")).toBe("poolside");
    expect(getDefaultModelForProvider("poolside")).toBe("poolside/laguna-m.1");
  });
});

describe("frontend Chutes provider catalog", () => {
  it("registers Chutes with fetchable OpenAI-compatible defaults", () => {
    const chutes = getProviderDefinition("chutes" as any);

    expect(chutes).toBeDefined();
    expect(chutes?.defaultBaseUrl).toBe("https://llm.chutes.ai/v1");
    expect(chutes?.defaultModel).toBe("deepseek-ai/DeepSeek-R1");
    expect(chutes?.supportsModelFetch).toBe(true);
    expect(DEFAULT_AGENT_SETTINGS.chutes.model).toBe("deepseek-ai/DeepSeek-R1");
  });

  it("recognizes Chutes as a valid provider id", () => {
    expect(normalizeAgentProviderId("chutes")).toBe("chutes");
    expect(getDefaultModelForProvider("chutes" as any)).toBe("deepseek-ai/DeepSeek-R1");
  });
});

describe("frontend DeepSeek provider catalog", () => {
  it("registers DeepSeek with fetchable OpenAI-compatible defaults", () => {
    const deepseek = getProviderDefinition("deepseek" as any);

    expect(deepseek).toBeDefined();
    expect(deepseek?.defaultBaseUrl).toBe("https://api.deepseek.com");
    expect(deepseek?.defaultModel).toBe("deepseek-v4-pro");
    expect(deepseek?.supportsModelFetch).toBe(true);
    expect(deepseek?.defaultTransport).toBe("chat_completions");
    expect(deepseek?.models.map((model) => [model.id, model.contextWindow])).toEqual([
      ["deepseek-v4-pro", 1_048_576],
      ["deepseek-v4-flash", 1_048_576],
    ]);
    expect(DEFAULT_AGENT_SETTINGS.deepseek.model).toBe("deepseek-v4-pro");
  });

  it("recognizes DeepSeek as a valid provider id", () => {
    expect(normalizeAgentProviderId("deepseek")).toBe("deepseek");
    expect(getDefaultModelForProvider("deepseek" as any)).toBe("deepseek-v4-pro");
  });
});

describe("frontend xAI provider catalog", () => {
  it("registers xAI with hosted defaults and responses transport", () => {
    const xai = getProviderDefinition("xai" as any);

    expect(xai).toBeDefined();
    expect(xai?.defaultBaseUrl).toBe("https://api.x.ai/v1");
    expect(xai?.defaultModel).toBe("grok-4.6");
    expect(xai?.models.map((model) => model.id)).toEqual([
      "grok-4.6",
      "grok-4.5",
      "grok-4.3",
      "grok-build-0.1",
      "grok-4",
      "grok-code-fast-1",
    ]);
    expect(xai?.supportsModelFetch).toBe(true);
    expect(xai?.defaultTransport).toBe("responses");
  });

  it("recognizes xAI as a valid provider id", () => {
    expect(normalizeAgentProviderId("xai")).toBe("xai");
    expect(getDefaultModelForProvider("xai" as any)).toBe("grok-4.6");
  });
});

describe("frontend MiniMax provider catalog", () => {
  it("registers current MiniMax models with remote fetch support", () => {
    for (const providerId of ["minimax", "minimax-coding-plan"] as const) {
      const provider = getProviderDefinition(providerId);

      expect(provider?.supportsModelFetch).toBe(true);
      expect(provider?.models.map((model) => model.id)).toEqual([
        "MiniMax-M3",
        "MiniMax-M2.7",
        "MiniMax-M2.7-highspeed",
        "MiniMax-M2.5",
        "MiniMax-M2.5-highspeed",
        "MiniMax-M2.1",
        "MiniMax-M2.1-highspeed",
        "MiniMax-M2",
      ]);
      expect(getEffectiveContextWindow(providerId, {
        model: "MiniMax-M3",
        custom_model_name: "",
        context_window_tokens: null,
        auth_source: "api_key",
      })).toBe(1_000_000);
      expect(getEffectiveContextWindow(providerId, {
        model: "MiniMax-M2.7",
        custom_model_name: "",
        context_window_tokens: null,
        auth_source: "api_key",
      })).toBe(205_000);
    }
  });
});

describe("frontend xAI audio/provider settings coverage", () => {
  it("flags only supported frontend audio providers as audio-capable", () => {
    expect(providerSupportsAudioTool("openai", "stt")).toBe(true);
    expect(providerSupportsAudioTool("openrouter", "tts")).toBe(true);
    expect(providerSupportsAudioTool("xai", "stt")).toBe(true);
    expect(providerSupportsAudioTool("elevenlabs", "stt")).toBe(true);
    expect(providerSupportsAudioTool("elevenlabs", "tts")).toBe(true);
    expect(providerSupportsAudioTool("xiaomi-mimo-token-plan", "tts")).toBe(true);
    expect(providerSupportsAudioTool("xiaomi-mimo-token-plan", "stt")).toBe(true);
    expect(providerSupportsAudioTool("minimax", "tts")).toBe(true);
    expect(providerSupportsAudioTool("minimax", "stt")).toBe(false);
    expect(providerSupportsAudioTool("minimax-coding-plan", "tts")).toBe(true);
    expect(providerSupportsAudioTool("minimax-coding-plan", "stt")).toBe(false);
    expect(providerSupportsAudioTool("anthropic", "stt")).toBe(false);
    expect(providerSupportsAudioTool("together", "tts")).toBe(false);
  });

  it("normalizes stale audio models when switching providers to xAI", () => {
    expect(normalizeAudioModelForProviderChange("xai", "stt", "whisper-1")).toBe("grok-4.3");
    expect(normalizeAudioModelForProviderChange("xai", "tts", "gpt-4o-mini-tts")).toBe("grok-4.3");
    expect(normalizeAudioModelForProviderChange("xai", "stt", "grok-4.3")).toBe("grok-4.3");
  });

  it("covers xAI settings normalization and daemon serialization in a collected test file", () => {
    expect(DEFAULT_AGENT_SETTINGS.xai).toMatchObject({
      base_url: "https://api.x.ai/v1",
      model: "grok-4.6",
      api_transport: "responses",
    });

    const normalizedSettings = normalizeAgentSettingsFromSource({
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
    } as any);

    expect(normalizedSettings.active_provider).toBe("xai");
    expect(normalizedSettings.audio_stt_provider).toBe("xai");
    expect(normalizedSettings.audio_tts_provider).toBe("xai");
    expect(normalizedSettings.xai).toMatchObject({
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
});

describe("frontend Anthropic provider catalog", () => {
  it("registers Anthropic with static defaults", () => {
    const anthropic = getProviderDefinition("anthropic" as any);

    expect(anthropic).toBeDefined();
    expect(anthropic?.defaultBaseUrl).toBe("https://api.anthropic.com");
    expect(anthropic?.defaultModel).toBe("claude-opus-4-7");
    expect(anthropic?.supportsModelFetch).toBe(false);
    expect(anthropic?.models.map((model) => [model.id, model.contextWindow])).toEqual([
      ["claude-opus-4-7", 1_000_000],
      ["claude-opus-4-6", 1_000_000],
      ["claude-opus-4-5-20251101", 200_000],
      ["claude-opus-4-1-20250805", 200_000],
      ["claude-opus-4-20250514", 200_000],
      ["claude-sonnet-4-6", 1_000_000],
      ["claude-sonnet-4-5-20250929", 200_000],
      ["claude-sonnet-4-20250514", 200_000],
      ["claude-3-7-sonnet-20250219", 200_000],
      ["claude-haiku-4-5-20251001", 200_000],
      ["claude-3-5-haiku-20241022", 200_000],
      ["claude-3-opus-20240229", 200_000],
      ["claude-3-haiku-20240307", 200_000],
    ]);
  });

  it("recognizes Anthropic as a valid provider id", () => {
    expect(normalizeAgentProviderId("anthropic")).toBe("anthropic");
    expect(getDefaultModelForProvider("anthropic" as any)).toBe("claude-opus-4-7");
  });
});

describe("frontend Xiaomi MiMo token plan provider catalog", () => {
  it("registers Xiaomi MiMo token plan with static defaults", () => {
    const mimo = getProviderDefinition("xiaomi-mimo-token-plan" as any);

    expect(mimo).toBeDefined();
    expect(mimo?.defaultBaseUrl).toBe("https://token-plan-ams.xiaomimimo.com/v1");
    expect(mimo?.anthropicBaseUrl).toBe("https://token-plan-ams.xiaomimimo.com/anthropic");
    expect(mimo?.defaultModel).toBe("mimo-v2.5-pro");
    expect(mimo?.supportsModelFetch).toBe(false);
    expect(mimo?.supportedTransports).toEqual(["chat_completions", "anthropic_messages"]);
    expect(mimo?.models.map((model) => [model.id, model.contextWindow])).toEqual([
      ["mimo-v2.5-pro", 1_000_000],
      ["mimo-v2.5-pro-ultraspeed", 1_000_000],
      ["mimo-v2.5", 1_000_000],
      ["mimo-v2.5-asr", 128_000],
      ["mimo-v2.5-tts-voiceclone", 128_000],
      ["mimo-v2.5-tts-voicedesign", 128_000],
      ["mimo-v2.5-tts", 128_000],
      ["mimo-v2-pro", 1_000_000],
      ["mimo-v2-omni", 256_000],
      ["mimo-v2-tts", 128_000],
    ]);
  });

  it("recognizes Xiaomi MiMo token plan as a valid provider id", () => {
    expect(normalizeAgentProviderId("xiaomi-mimo-token-plan")).toBe("xiaomi-mimo-token-plan");
    expect(getDefaultModelForProvider("xiaomi-mimo-token-plan" as any)).toBe("mimo-v2.5-pro");
  });
});

describe("frontend Nous Portal provider catalog", () => {
  it("registers Nous Portal with fetchable defaults", () => {
    const nous = getProviderDefinition("nous-portal" as any);

    expect(nous).toBeDefined();
    expect(nous?.defaultBaseUrl).toBe("https://inference-api.nousresearch.com/v1");
    expect(nous?.defaultModel).toBe("nousresearch/hermes-4-70b");
    expect(nous?.supportsModelFetch).toBe(true);
    expect(nous?.models.map((model) => [model.id, model.contextWindow])).toEqual([
      ["nousresearch/hermes-4-70b", 131_072],
      ["nousresearch/hermes-4-405b", 131_072],
      ["nousresearch/hermes-3-llama-3.1-70b", 131_072],
      ["nousresearch/hermes-3-llama-3.1-405b", 131_072],
    ]);
  });

  it("recognizes Nous Portal as a valid provider id", () => {
    expect(normalizeAgentProviderId("nous-portal")).toBe("nous-portal");
    expect(getDefaultModelForProvider("nous-portal" as any)).toBe("nousresearch/hermes-4-70b");
  });
});

describe("frontend Hermes Agent API provider catalog", () => {
  it("registers Hermes Agent API with documented OpenAI-compatible defaults", () => {
    const hermes = getProviderDefinition("hermes-agent-api" as any);

    expect(hermes).toBeDefined();
    expect(hermes?.name).toBe("Hermes Agent API");
    expect(hermes?.defaultBaseUrl).toBe("http://localhost:8642/v1");
    expect(hermes?.defaultModel).toBe("hermes-agent");
    expect(hermes?.supportsModelFetch).toBe(true);
    expect(hermes?.defaultTransport).toBe("chat_completions");
    expect(hermes?.supportedTransports).toEqual(["chat_completions"]);
    expect(hermes?.supportsResponseContinuity).toBe(false);
    expect(hermes?.models.map((model) => [model.id, model.contextWindow])).toEqual([
      ["hermes-agent", 128_000],
    ]);
    expect(DEFAULT_AGENT_SETTINGS["hermes-agent-api"].model).toBe("hermes-agent");
  });

  it("recognizes Hermes Agent API as a valid provider id", () => {
    expect(normalizeAgentProviderId("hermes-agent-api")).toBe("hermes-agent-api");
    expect(getDefaultModelForProvider("hermes-agent-api" as any)).toBe("hermes-agent");
  });
});

describe("frontend Azure OpenAI provider catalog", () => {
  it("registers Azure OpenAI with OpenAI-compatible defaults", () => {
    const azure = getProviderDefinition("azure-openai" as any);

    expect(azure).toBeDefined();
    expect(azure?.defaultBaseUrl).toBe(
      "https://YOUR-RESOURCE-NAME.openai.azure.com/openai/v1",
    );
    expect(azure?.defaultModel).toBe("");
    expect(azure?.supportsModelFetch).toBe(true);
    expect(azure?.defaultTransport).toBe("responses");
    expect(azure?.supportedAuthSources).toEqual(["api_key"]);
    expect(azure?.supportsResponseContinuity).toBe(true);
  });

  it("recognizes Azure OpenAI as a valid provider id", () => {
    expect(normalizeAgentProviderId("azure-openai")).toBe("azure-openai");
    expect(getDefaultModelForProvider("azure-openai" as any)).toBe("");
  });

  it("preserves the configured resource-specific base URL", () => {
    const normalized = normalizeProviderConfig(
      "azure-openai" as any,
      {
        base_url: "https://YOUR-RESOURCE-NAME.openai.azure.com/openai/v1",
        model: "",
        custom_model_name: "",
        api_key: "",
        assistant_id: "",
        api_transport: "responses",
        auth_source: "api_key",
        context_window_tokens: null,
      },
      {
        base_url: "https://my-real-resource.openai.azure.com/openai/v1",
        model: "deployment-name",
      },
    );

    expect(normalized.base_url).toBe("https://my-real-resource.openai.azure.com/openai/v1");
    expect(normalized.model).toBe("deployment-name");
  });
});

describe("frontend curated media provider catalog", () => {
  it("keeps the OpenAI fallback catalog aligned with the daemon default", () => {
    expect(getDefaultModelForProvider("openai")).toBe("gpt-5.5");
    expect(getProviderModels("openai").map((model) => model.id).slice(0, 2)).toEqual([
      "gpt-5.5",
      "gpt-6-astra",
    ]);
    expect(getDefaultModelForProvider("openai", "chatgpt_subscription")).toBe("gpt-5.5");
    expect(
      getProviderModels("openai", "chatgpt_subscription").map((model) => model.id).slice(0, 2),
    ).toEqual([
      "gpt-5.5",
      "gpt-6-astra",
    ]);
    expect(getProviderModels("openai").map((model) => model.id)).toEqual(expect.arrayContaining([
      "gpt-6-astra",
      "gpt-5.6-sol",
      "gpt-5.6-terra",
      "gpt-5.6-luna",
    ]));
    expect(getProviderModels("openai", "chatgpt_subscription").map((model) => model.id)).toEqual(
      expect.arrayContaining(["gpt-6-astra", "gpt-5.6-sol-pro"]),
    );
    expect(getProviderModels("openai").find((model) => model.id === "gpt-5.3-codex-spark")).toMatchObject({
      contextWindow: 128_000,
    });
    expect(getProviderModels("openai", "chatgpt_subscription").map((model) => model.id)).toContain(
      "gpt-5.3-codex-spark",
    );
  });

  it("keeps representative modalities aligned with the curated matrix", () => {
    expect(getModelModalities(getModelDefinition("openai", "gpt-5.5"))).toEqual([
      "text",
      "image",
      "video",
      "audio",
    ]);
    expect(getModelModalities(getModelDefinition("openai", "gpt-5.4"))).toEqual([
      "text",
      "image",
      "video",
      "audio",
    ]);
    expect(
      getModelModalities(getModelDefinition("anthropic", "claude-opus-4-7")),
    ).toEqual(["text", "image"]);
    expect(
      getModelModalities(getModelDefinition("openai", "gpt-5.3-codex-spark")),
    ).toEqual(["text"]);
    expect(
      getModelModalities(getModelDefinition("xiaomi-mimo-token-plan", "mimo-v2-omni")),
    ).toEqual(["text", "image", "video", "audio"]);
    expect(
      getModelModalities(getModelDefinition("xiaomi-mimo-token-plan", "mimo-v2.5")),
    ).toEqual(["text", "image", "video", "audio"]);
    expect(
      getModelModalities(getModelDefinition("xiaomi-mimo-token-plan", "mimo-v2.5-tts")),
    ).toEqual(["text", "audio"]);
    expect(getModelModalities(getModelDefinition("arcee", "trinity-large-thinking"))).toEqual([
      "text",
    ]);
  });

  it("keeps first-party default model resolution aligned with the provider defaults", () => {
    expect(getDefaultModelForProvider("qwen")).toBe("qwen-max");
    expect(getDefaultModelForProvider("kimi")).toBe("moonshot-v1-32k");
    expect(getDefaultModelForProvider("z.ai")).toBe("glm-4-plus");
    expect(getDefaultModelForProvider("z.ai-coding-plan")).toBe("glm-5.3");
    expect(getDefaultModelForProvider("alibaba-coding-plan")).toBe("qwen3.6-plus");
    expect(getDefaultModelForProvider("alibaba-token-plan")).toBe("qwen3.7-max");
  });

  it("removes stale frontend catalog drift for z.ai, kimi coding, and alibaba coding", () => {
    expect(getProviderModels("qwen").map((model) => [model.id, getModelModalities(model)])).toEqual([
      ["qwen-max", ["text", "image"]],
      ["qwen-plus", ["text", "image"]],
      ["qwen-turbo", ["text"]],
      ["qwen-long", ["text"]],
    ]);
    expect(getProviderModels("z.ai").map((model) => model.id)).toEqual([
      "glm-4-plus",
      "glm-5.3-flash",
      "glm-5.1",
      "glm-5",
      "glm-4",
      "glm-4-air",
      "glm-4-flash",
    ]);
    expect(getProviderModels("z.ai-coding-plan").map((model) => model.id)).toEqual([
      "glm-5.3",
      "glm-5.3-flash",
      "glm-5-turbo",
      "glm-4.7",
      "glm-5",
      "glm-5.2",
      "glm-5.1",
      "glm-4-plus",
      "glm-4",
      "glm-4-air",
      "glm-4-flash",
    ]);
    expect(getProviderModels("kimi-coding-plan").map((model) => model.id)).toEqual([
      "k3",
      "kimi-for-coding",
      "kimi-k2.7-code",
      "kimi-k2.6",
      "kimi-k2.5",
      "kimi-k2-turbo-preview",
    ]);
    expect(getProviderModels("alibaba-coding-plan").map((model) => model.id)).toEqual([
      "qwen3.6-plus",
      "qwen3.8-flash",
      "qwen3-coder-plus",
      "qwen3-coder-next",
      "glm-5",
      "kimi-k2.6",
      "kimi-k2.5",
      "MiniMax-M2.5",
    ]);
    expect(getProviderModels("alibaba-token-plan").map((model) => model.id)).toEqual([
      "qwen3.7-max",
      "qwen3.7-plus",
      "qwen3.6-plus",
      "qwen3.6-flash",
      "qwen-image-2.0",
      "qwen-image-2.0-pro",
      "wan2.7-image",
      "wan2.7-image-pro",
      "deepseek-v4-pro",
      "deepseek-v4-flash",
      "deepseek-v3.2",
      "kimi-k2.6",
      "kimi-k2.5",
      "glm-5.1",
      "glm-5",
      "MiniMax-M2.5",
    ]);
    expect(getProviderModels("opencode-zen").map((model) => model.id)).toEqual([
      "claude-opus-4-6",
      "claude-sonnet-4-5",
      "claude-sonnet-4",
      "gpt-5.4",
      "gpt-5.3-codex",
      "minimax-m2.5",
      "glm-5",
      "kimi-k2.6",
      "kimi-k2.5",
    ]);
    expect(getProviderModels("opencode-go").map((model) => model.id)).toEqual([
      "glm-5.1",
      "glm-5",
      "kimi-k2.7-code",
      "kimi-k2.6",
      "deepseek-v4-pro",
      "deepseek-v4-flash",
      "mimo-v2.5",
      "mimo-v2.5-pro",
      "minimax-m3",
      "minimax-m2.7",
      "minimax-m2.5",
      "qwen3.7-max",
      "qwen3.7-plus",
      "qwen3.6-plus",
    ]);
  });

  it("keeps legacy alibaba coding-plan models on the curated context-window path", () => {
    const normalized = normalizeProviderConfig(
      "alibaba-coding-plan" as any,
      {
        base_url: "https://coding-intl.dashscope.aliyuncs.com/v1",
        model: "qwen3.6-plus",
        custom_model_name: "",
        api_key: "",
        assistant_id: "",
        api_transport: "chat_completions",
        auth_source: "api_key",
        context_window_tokens: null,
      },
      {
        model: "qwen3.5-plus",
      },
    );

    expect(normalized.custom_model_name).toBe("");
    expect(getEffectiveContextWindow("alibaba-coding-plan", normalized)).toBe(983_616);
  });
});

describe("frontend Cursor subscription provider", () => {
  it("uses the local Cursor CLI subscription instead of an HTTP base URL", () => {
    expect(getDefaultModelForProvider("cursor")).toBe("auto");
    expect(DEFAULT_AGENT_SETTINGS.cursor.model).toBe("auto");
    expect(DEFAULT_AGENT_SETTINGS.cursor.base_url).toBe("");
    expect(DEFAULT_AGENT_SETTINGS.cursor.api_transport).toBe("native_assistant");
    expect(getProviderDefinition("cursor")?.nativeTransportKind).toBe("cursor_cli");
    expect(getProviderModels("cursor").map((model) => model.id)).toContain("composer-2.5");
    expect(normalizeApiTransport("cursor", "chat_completions")).toBe("native_assistant");
  });
});

describe("frontend GitHub Copilot provider routing", () => {
  it("defaults to GPT-5.4 and keeps GPT-5.5 selectable in the fallback catalog", () => {
    expect(getDefaultModelForProvider("github-copilot")).toBe("gpt-5.4");
    expect(DEFAULT_AGENT_SETTINGS["github-copilot"].model).toBe("gpt-5.4");
    expect(getProviderModels("github-copilot").map((model) => model.id).slice(0, 2)).toEqual([
      "gpt-5.4",
      "gpt-5.5",
    ]);
    expect(getProviderModels("github-copilot").map((model) => model.id)).toContain(
      "gpt-5.5",
    );
    expect(getModelDefinition("github-copilot", "gpt-5.4")?.contextWindow).toBe(400_000);
    expect(getModelDefinition("github-copilot", "gpt-5.5")?.contextWindow).toBe(400_000);
  });

  it("keeps Claude models on the OpenAI provider api type", () => {
    expect(
      getProviderApiType(
        "github-copilot",
        "claude-sonnet-4.6",
        "https://api.githubcopilot.com",
      ),
    ).toBe("openai");
  });

  it("switches the Alibaba token plan to anthropic only for its /apps/anthropic base url", () => {
    expect(
      getProviderApiType(
        "alibaba-token-plan",
        "qwen3.7-max",
        "https://token-plan.ap-southeast-1.maas.aliyuncs.com/compatible-mode/v1",
      ),
    ).toBe("openai");
    expect(
      getProviderApiType(
        "alibaba-token-plan",
        "qwen3.7-max",
        "https://token-plan.ap-southeast-1.maas.aliyuncs.com/apps/anthropic",
      ),
    ).toBe("anthropic");
  });

  it("exposes anthropic_messages as an explicit selectable transport", () => {
    const copilot = getProviderDefinition("github-copilot");

    expect(copilot?.supportedTransports).toContain("anthropic_messages");
    expect(normalizeApiTransport("github-copilot", "anthropic_messages")).toBe(
      "anthropic_messages",
    );
  });
});
