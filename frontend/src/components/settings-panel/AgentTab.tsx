import { useCallback, useEffect, useState } from "react";
import { getDaemonOwnedAuthCapability, getProviderAuthSupportOptions } from "@/lib/agentDaemonConfig";
import { getBridge } from "@/lib/bridge";
import { PRIMARY_AGENT_NAME } from "@/lib/agentNames";
import { filterFetchedModelsForAudio, filterFetchedModelsForEmbeddings, filterFetchedModelsForImageGeneration } from "@/lib/providerModels";
import type { AgentProviderConfig, AgentProviderId, AgentSettings } from "../../lib/agentStore";
import { DEFAULT_CUSTOM_MODEL_CONTEXT_WINDOW, getDefaultApiTransport, getDefaultAuthSource, getDefaultModelForProvider, getEffectiveContextWindow, getProviderApiType, getProviderDefinition, getProviderModels, getSupportedApiTransports, getSupportedAuthSources, modelUsesContextWindowOverride, normalizeAuthSource, providerUsesConfigurableBaseUrl, resolveProviderModelDefinition } from "../../lib/agentStore";
import { useAgentStore } from "../../lib/agentStore";
import { deriveOpenAICodexAuthUi } from "./openaiSubscriptionAuth";
import { applySttReuseDecision, getModelSelectionEffects } from "./modelSelectionEffects";
import {
    audioModelOptions,
    audioRemoteModelFetchOutputModalities,
    buildProviderOptions,
    filterAudioProviderOptions,
    filterEmbeddingProviderOptions,
    filterImageGenerationProviderOptions,
    embeddingModelOptions,
    imageGenerationModelOptions,
    imageRemoteModelFetchOutputModalities,
    normalizeEmbeddingModelForProviderChange,
    normalizeAudioModelForProviderChange,
    normalizeImageGenerationModelForProviderChange,
    normalizeLlmStreamTimeoutInput,
    type ProviderOption,
} from "./agentTabHelpers";
import { GeneratedToolsPanel } from "../generated-tools/GeneratedToolsPanel";
import { OperatorModelControls } from "./OperatorModelControls";
import { PromptPreviewSection } from "./PromptPreviewSection";
import {
    DEFAULT_CHAT_HISTORY_PAGE_SIZE,
    MAX_TUI_CHAT_HISTORY_PAGE_SIZE,
    MIN_CHAT_HISTORY_PAGE_SIZE,
    REACT_CHAT_HISTORY_PAGE_SIZE_ALL,
    normalizeReactChatHistoryPageSize,
    normalizeTuiChatHistoryPageSize,
} from "../../lib/chatHistoryPageSize";
import { RefreshButton } from "@/zorai/shell/RefreshButton";
import { addBtnStyle, ModelSelector, NumberInput, PasswordInput, Section, SelectInput, SettingRow, TextInput, Toggle, inputStyle, smallBtnStyle } from "./shared";
import { OpenRouterProviderRoutingControls } from "./OpenRouterProviderRoutingControls";

type SemanticIndexStatus = {
    queued_jobs?: number;
    pending_for_model?: number;
    completed_for_model?: number;
    queued_deletions?: number;
    failed_jobs?: number;
    failed_deletions?: number;
    error?: string;
};


export function AgentTab({
    settings, updateSetting, resetSettings,
}: {
    settings: AgentSettings;
    updateSetting: <K extends keyof AgentSettings>(key: K, value: AgentSettings[K]) => void;
    resetSettings: () => void;
}) {
    const [useCustomUrl, setUseCustomUrl] = useState(false);
    const [subscriptionAuthStatus, setSubscriptionAuthStatus] = useState<any>(null);
    const [subscriptionAuthBusy, setSubscriptionAuthBusy] = useState(false);
    const [pendingSttReuseModelId, setPendingSttReuseModelId] = useState<string | null>(null);
    const [semanticStatus, setSemanticStatus] = useState<SemanticIndexStatus | null>(null);
    const [semanticBackfillBusy, setSemanticBackfillBusy] = useState(false);
    const providerAuthStates = useAgentStore((s) => s.providerAuthStates);
    const refreshProviderAuthStates = useAgentStore((s) => s.refreshProviderAuthStates);

    const openSubscriptionAuthUrl = (url: string) => {
        if (!url) return;
        const opened = typeof window !== "undefined"
            ? window.open(url, "_blank", "noopener,noreferrer")
            : null;
        if (opened) {
            opened.opener = null;
        }
    };

    const builtInProviderOptions: ProviderOption[] = [
        { id: "featherless", label: "Featherless" },
        { id: "anthropic", label: "Anthropic" },
        { id: "openai", label: "OpenAI / ChatGPT" },
        { id: "xai", label: "xAI" },
        { id: "azure-openai", label: "Azure OpenAI" },
        { id: "github-copilot", label: "GitHub Copilot" },
        { id: "qwen", label: "Qwen" },
        { id: "qwen-deepinfra", label: "Qwen (DeepInfra)" },
        { id: "kimi", label: "Kimi (Moonshot)" },
        { id: "kimi-coding-plan", label: "Kimi Coding Plan" },
        { id: "z.ai", label: "Z.AI" },
        { id: "z.ai-coding-plan", label: "Z.AI Coding Plan" },
        { id: "arcee", label: "Arcee" },
        { id: "nvidia", label: "NVIDIA" },
        { id: "nous-portal", label: "Nous Portal" },
        { id: "openrouter", label: "OpenRouter" },
        { id: "cerebras", label: "Cerebras" },
        { id: "together", label: "Together" },
        { id: "groq", label: "Groq" },
        { id: "ollama", label: "Ollama" },
        { id: "chutes", label: "Chutes" },
        { id: "huggingface", label: "Hugging Face" },
        { id: "minimax", label: "MiniMax" },
        { id: "minimax-coding-plan", label: "MiniMax Coding Plan" },
        { id: "alibaba-coding-plan", label: "Alibaba Coding Plan" },
        { id: "xiaomi-mimo-token-plan", label: "Xiaomi MiMo Token Plan" },
        { id: "opencode-zen", label: "OpenCode Zen" },
        { id: "custom", label: "Custom" },
    ];
    const { allProviderOptions, providerOptions } = buildProviderOptions(builtInProviderOptions, providerAuthStates);
    const audioSttProviderOptions = filterAudioProviderOptions(providerOptions, "stt");
    const audioTtsProviderOptions = filterAudioProviderOptions(providerOptions, "tts");
    const imageGenerationProviderOptions = filterImageGenerationProviderOptions(providerOptions);
    const embeddingProviderOptions = filterEmbeddingProviderOptions(providerOptions);

    const providerConfig = settings[settings.active_provider] as AgentProviderConfig;
    const audioSttProviderConfig = settings[settings.audio_stt_provider] as AgentProviderConfig;
    const audioTtsProviderConfig = settings[settings.audio_tts_provider] as AgentProviderConfig;
    const imageGenerationProviderConfig = settings[settings.image_generation_provider] as AgentProviderConfig;
    const embeddingProviderConfig = settings[settings.semantic_embedding_provider] as AgentProviderConfig;
    const authCapability = getDaemonOwnedAuthCapability(settings.agent_backend);
    const authSupportOptions = getProviderAuthSupportOptions(settings.agent_backend);
    const providerDef = getProviderDefinition(settings.active_provider);
    const providerApiType = getProviderApiType(
        settings.active_provider,
        providerConfig.model,
        providerConfig.base_url,
    );
    const supportedTransports = getSupportedApiTransports(settings.active_provider);
    const supportedAuthSources = getSupportedAuthSources(settings.active_provider, authSupportOptions);
    const effectiveAuthSource = normalizeAuthSource(settings.active_provider, providerConfig.auth_source, authSupportOptions);
    const isCustomProvider = settings.active_provider === "custom";
    const usesConfigurableUrl = providerUsesConfigurableBaseUrl(settings.active_provider);
    const showUrlEditor = usesConfigurableUrl || useCustomUrl || Boolean(providerConfig.base_url && providerConfig.base_url !== providerDef?.defaultBaseUrl);
    const effectiveContextWindow = getEffectiveContextWindow(settings.active_provider, providerConfig);
    const canEditContextWindow = modelUsesContextWindowOverride(
        settings.active_provider,
        providerConfig.model,
        providerConfig.custom_model_name,
        effectiveAuthSource,
    );
    const activeProviderAuthState = providerAuthStates.find((state) => state.provider_id === settings.active_provider);
    const providerHasDaemonAuth = (providerId: AgentProviderId) =>
        Boolean(providerAuthStates.find((state) => state.provider_id === providerId)?.authenticated);
    const subscriptionAuthUi = deriveOpenAICodexAuthUi(subscriptionAuthStatus);
    const providerAuthenticated = effectiveAuthSource === "chatgpt_subscription"
        ? Boolean(subscriptionAuthStatus?.available)
        : effectiveAuthSource === "github_copilot"
            ? Boolean(activeProviderAuthState?.authenticated)
            : Boolean(providerConfig.api_key || activeProviderAuthState?.authenticated);
    const daemonDelayInputStyle = { ...inputStyle, width: 80 };
    const reactHistoryIsUnlimited = settings.react_chat_history_page_size === REACT_CHAT_HISTORY_PAGE_SIZE_ALL;
    const refreshSemanticIndexStatus = useCallback(async () => {
        const zorai = getBridge();
        if (!zorai?.dbGetSemanticIndexStatus) return;
        const status = await zorai.dbGetSemanticIndexStatus({
            embeddingModel: settings.semantic_embedding_model,
            dimensions: settings.semantic_embedding_dimensions,
        });
        if (status && typeof status === "object") {
            setSemanticStatus(status as SemanticIndexStatus);
        }
    }, [settings.semantic_embedding_dimensions, settings.semantic_embedding_model]);
    const queueSemanticBackfill = async () => {
        const zorai = getBridge();
        if (!zorai?.dbQueueSemanticBackfill) return;
        setSemanticBackfillBusy(true);
        try {
            await zorai.dbQueueSemanticBackfill(null);
            await refreshSemanticIndexStatus();
        } finally {
            setSemanticBackfillBusy(false);
        }
    };

    useEffect(() => {
        void refreshProviderAuthStates();
    }, [refreshProviderAuthStates]);

    useEffect(() => {
        void refreshSemanticIndexStatus();
    }, [refreshSemanticIndexStatus]);

    useEffect(() => {
        if (
            settings.active_provider !== "openai"
            || effectiveAuthSource !== "chatgpt_subscription"
            || !authCapability.chatgptSubscriptionAvailable
        ) {
            setSubscriptionAuthStatus(null);
            return;
        }

        const zorai = getBridge();
        if (!zorai?.openAICodexAuthStatus) {
            setSubscriptionAuthStatus({ ok: false, available: false, error: "ChatGPT auth bridge unavailable" });
            return;
        }

        let cancelled = false;
        void zorai.openAICodexAuthStatus({ refresh: true }).then((status: any) => {
            if (!cancelled) {
                setSubscriptionAuthStatus(status);
            }
        }).catch((error: any) => {
            if (!cancelled) {
                setSubscriptionAuthStatus({ ok: false, available: false, error: error?.message || "Failed to read ChatGPT auth" });
            }
        });

        return () => {
            cancelled = true;
        };
    }, [authCapability.chatgptSubscriptionAvailable, effectiveAuthSource, settings.active_provider]);

    useEffect(() => {
        if (!subscriptionAuthUi.shouldPoll) {
            return;
        }

        const timer = window.setInterval(() => {
            const zorai = getBridge();
            if (!zorai?.openAICodexAuthStatus) {
                return;
            }
            void zorai.openAICodexAuthStatus({ refresh: true }).then((status: any) => {
                setSubscriptionAuthStatus(status);
            }).catch(() => { });
        }, 2000);

        return () => window.clearInterval(timer);
    }, [subscriptionAuthUi.shouldPoll]);

    useEffect(() => {
        if (pendingSttReuseModelId && settings.audio_stt_model === pendingSttReuseModelId) {
            setPendingSttReuseModelId(null);
        }
    }, [pendingSttReuseModelId, settings.audio_stt_model]);

    const triggerSubscriptionAuth = async () => {
        if (!authCapability.chatgptSubscriptionAvailable) {
            setSubscriptionAuthStatus({ ok: false, available: false, error: "ChatGPT subscription auth requires daemon-backed execution" });
            return;
        }
        const zorai = getBridge();
        if (!zorai?.openAICodexAuthLogin) {
            setSubscriptionAuthStatus({ ok: false, available: false, error: "ChatGPT auth bridge unavailable" });
            return;
        }

        setSubscriptionAuthBusy(true);
        try {
            const result = await zorai.openAICodexAuthLogin();
            setSubscriptionAuthStatus(result);
            const authUrl = deriveOpenAICodexAuthUi(result).authUrl;
            if (authUrl) {
                openSubscriptionAuthUrl(authUrl);
            }
        } catch (error: any) {
            setSubscriptionAuthStatus({ ok: false, available: false, error: error?.message || "ChatGPT authentication failed" });
        } finally {
            setSubscriptionAuthBusy(false);
        }
    };

    const clearSubscriptionAuth = async () => {
        if (!authCapability.chatgptSubscriptionAvailable) {
            setSubscriptionAuthStatus({ ok: false, available: false, error: "ChatGPT subscription auth requires daemon-backed execution" });
            return;
        }
        const zorai = getBridge();
        if (!zorai?.openAICodexAuthLogout) {
            setSubscriptionAuthStatus({ ok: false, available: false, error: "ChatGPT auth bridge unavailable" });
            return;
        }

        setSubscriptionAuthBusy(true);
        try {
            await zorai.openAICodexAuthLogout();
            setSubscriptionAuthStatus({ available: false, status: null, authMode: "chatgpt_subscription", error: "No ChatGPT subscription auth found" });
        } catch (error: any) {
            setSubscriptionAuthStatus({ ok: false, available: false, error: error?.message || "Failed to clear ChatGPT auth" });
        } finally {
            setSubscriptionAuthBusy(false);
        }
    };

    return (
        <>
            <div style={{
                display: "flex",
                alignItems: "center",
                gap: 8,
                marginBottom: 14,
                padding: "8px 12px",
                border: "1px solid var(--border)",
                background: "var(--zorai-bg-panel)",
            }}>
                <span style={{
                    width: 8, height: 8, borderRadius: "50%",
                    background: providerAuthenticated ? "#4ade80" : "#6b7280",
                }} />
                <span style={{ fontSize: "var(--text-sm)", fontWeight: 600 }}>
                    {PRIMARY_AGENT_NAME}: {allProviderOptions.find((p) => p.id === settings.active_provider)?.label || settings.active_provider}
                </span>
                <span style={{
                    fontSize: "var(--text-sm)",
                    color: "var(--text-secondary)",
                    background: "var(--zorai-bg-panel)",
                    padding: "1px 6px",
                    borderRadius: 3,
                }}>
                    {providerConfig.model || "No model"}
                </span>
                <button
                    onClick={() => {
                        window.dispatchEvent(new CustomEvent("zorai-open-settings-tab", { detail: { tab: "auth" } }));
                        window.dispatchEvent(new CustomEvent("zorai-open-settings-tab", { detail: { tab: "auth" } }));
                    }}
                    style={{ ...smallBtnStyle, fontSize: "var(--text-sm)", marginLeft: "auto" }}
                >
                    Manage Auth
                </button>
            </div>
            <Section title="General">
                <SettingRow label={`${PRIMARY_AGENT_NAME} Name`}>
                    <div style={{ ...inputStyle, width: "100%", color: "var(--text-primary)", opacity: 0.85, cursor: "default" }}>
                        {PRIMARY_AGENT_NAME}
                    </div>
                </SettingRow>
                <SettingRow label="Handler Prefix">
                    <TextInput value={settings.handler} onChange={(value) => updateSetting("handler", value)} />
                </SettingRow>
                <SettingRow label="System Prompt">
                    <textarea value={settings.system_prompt}
                        onChange={(event) => updateSetting("system_prompt", event.target.value)}
                        rows={4}
                        style={{ ...inputStyle, width: "100%", resize: "vertical", fontFamily: "inherit" }} />
                </SettingRow>
            </Section>

            <Section title="Audio">
                    <SettingRow label="Enable Speech-to-Text">
                        <Toggle value={settings.audio_stt_enabled} onChange={(value) => updateSetting("audio_stt_enabled", value)} />
                    </SettingRow>
                    <SettingRow label="STT Provider">
                        <SelectInput
                            value={settings.audio_stt_provider}
                            options={audioSttProviderOptions.map((provider) => provider.id)}
                            onChange={(value) => {
                                const providerId = value as AgentProviderId;
                                updateSetting("audio_stt_provider", providerId);
                                updateSetting(
                                    "audio_stt_model",
                                    normalizeAudioModelForProviderChange(
                                        providerId,
                                        "stt",
                                        settings.audio_stt_model,
                                    ),
                                );
                            }}
                        />
                    </SettingRow>
                        <SettingRow label="STT Model">
                        <ModelSelector
                            providerId={settings.audio_stt_provider}
                            value={settings.audio_stt_model}
                            customName={audioSttProviderConfig.custom_model_name}
                            onChange={(value) => updateSetting("audio_stt_model", value)}
                            base_url={audioSttProviderConfig.base_url}
                            api_key={audioSttProviderConfig.api_key}
                            auth_source={audioSttProviderConfig.auth_source}
                            allowProviderAuthFetch={providerHasDaemonAuth(settings.audio_stt_provider)}
                            modelOptions={audioModelOptions(settings.audio_stt_provider, "stt")}
                            remoteModelFilter={(model) => filterFetchedModelsForAudio([model], "stt").length > 0}
                            fetchOutputModalities={audioRemoteModelFetchOutputModalities("stt", settings.audio_stt_provider)}
                            disabled={!settings.audio_stt_enabled}
                        />
                    </SettingRow>
                    <SettingRow label="STT Language">
                        <TextInput
                            value={settings.audio_stt_language}
                            onChange={(value) => updateSetting("audio_stt_language", value)}
                            placeholder="auto"
                            disabled={!settings.audio_stt_enabled}
                        />
                    </SettingRow>

                    <SettingRow label="Enable Text-to-Speech">
                        <Toggle value={settings.audio_tts_enabled} onChange={(value) => updateSetting("audio_tts_enabled", value)} />
                    </SettingRow>
                    <SettingRow label="TTS Provider">
                        <SelectInput
                            value={settings.audio_tts_provider}
                            options={audioTtsProviderOptions.map((provider) => provider.id)}
                            onChange={(value) => {
                                const providerId = value as AgentProviderId;
                                updateSetting("audio_tts_provider", providerId);
                                updateSetting(
                                    "audio_tts_model",
                                    normalizeAudioModelForProviderChange(
                                        providerId,
                                        "tts",
                                        settings.audio_tts_model,
                                    ),
                                );
                            }}
                        />
                    </SettingRow>
                    <SettingRow label="TTS Model">
                        <ModelSelector
                            providerId={settings.audio_tts_provider}
                            value={settings.audio_tts_model}
                            customName={audioTtsProviderConfig.custom_model_name}
                            onChange={(value) => updateSetting("audio_tts_model", value)}
                            base_url={audioTtsProviderConfig.base_url}
                            api_key={audioTtsProviderConfig.api_key}
                            auth_source={audioTtsProviderConfig.auth_source}
                            allowProviderAuthFetch={providerHasDaemonAuth(settings.audio_tts_provider)}
                            modelOptions={audioModelOptions(settings.audio_tts_provider, "tts")}
                            remoteModelFilter={(model) => filterFetchedModelsForAudio([model], "tts").length > 0}
                            fetchOutputModalities={audioRemoteModelFetchOutputModalities("tts", settings.audio_tts_provider)}
                            disabled={!settings.audio_tts_enabled}
                        />
                    </SettingRow>
                    <SettingRow label="TTS Voice">
                        <TextInput
                            value={settings.audio_tts_voice}
                            onChange={(value) => updateSetting("audio_tts_voice", value)}
                            placeholder="alloy"
                            disabled={!settings.audio_tts_enabled}
                        />
                    </SettingRow>
                    <SettingRow label="Auto-speak Replies">
                        <Toggle value={settings.audio_tts_auto_speak} onChange={(value) => updateSetting("audio_tts_auto_speak", value)} />
                    </SettingRow>
            </Section>

            <Section title="Image Generation">
                <SettingRow label="Image Provider">
                    <SelectInput
                        value={settings.image_generation_provider}
                        options={imageGenerationProviderOptions.map((provider) => provider.id)}
                        onChange={(value) => {
                            const providerId = value as AgentProviderId;
                            updateSetting("image_generation_provider", providerId);
                            updateSetting(
                                "image_generation_model",
                                normalizeImageGenerationModelForProviderChange(
                                    providerId,
                                    settings.image_generation_model,
                                ),
                            );
                        }}
                    />
                </SettingRow>
                <SettingRow label="Image Model">
                    <ModelSelector
                        providerId={settings.image_generation_provider}
                        value={settings.image_generation_model}
                        customName={imageGenerationProviderConfig.custom_model_name}
                        onChange={(value) => updateSetting("image_generation_model", value)}
                        base_url={imageGenerationProviderConfig.base_url}
                        api_key={imageGenerationProviderConfig.api_key}
                        auth_source={imageGenerationProviderConfig.auth_source}
                        allowProviderAuthFetch={providerHasDaemonAuth(settings.image_generation_provider)}
                        modelOptions={imageGenerationModelOptions(settings.image_generation_provider)}
                        remoteModelFilter={(model) => filterFetchedModelsForImageGeneration([model]).length > 0}
                        fetchOutputModalities={imageRemoteModelFetchOutputModalities(settings.image_generation_provider)}
                    />
                </SettingRow>
            </Section>

            <Section title="Semantic Search">
                <SettingRow label="Enable Embeddings">
                    <Toggle
                        value={settings.semantic_embedding_enabled}
                        onChange={(value) => updateSetting("semantic_embedding_enabled", value)}
                    />
                </SettingRow>
                <SettingRow label="Embedding Provider">
                    <SelectInput
                        value={settings.semantic_embedding_provider}
                        options={embeddingProviderOptions.map((provider) => provider.id)}
                        onChange={(value) => {
                            const providerId = value as AgentProviderId;
                            updateSetting("semantic_embedding_provider", providerId);
                            updateSetting(
                                "semantic_embedding_model",
                                normalizeEmbeddingModelForProviderChange(
                                    providerId,
                                    settings.semantic_embedding_model,
                                ),
                            );
                        }}
                    />
                </SettingRow>
                <SettingRow label="Embedding Model">
                    <ModelSelector
                        providerId={settings.semantic_embedding_provider}
                        value={settings.semantic_embedding_model}
                        customName={embeddingProviderConfig.custom_model_name}
                        onChange={(value) => updateSetting("semantic_embedding_model", value)}
                        base_url={embeddingProviderConfig.base_url}
                        api_key={embeddingProviderConfig.api_key}
                        auth_source={embeddingProviderConfig.auth_source}
                        allowProviderAuthFetch={providerHasDaemonAuth(settings.semantic_embedding_provider)}
                        modelOptions={embeddingModelOptions(settings.semantic_embedding_provider)}
                        remoteModelFilter={(model) => filterFetchedModelsForEmbeddings([model]).length > 0}
                        fetchOutputModalities="embedding"
                        disabled={!settings.semantic_embedding_enabled}
                    />
                </SettingRow>
                <SettingRow label="Dimensions">
                    <NumberInput
                        value={settings.semantic_embedding_dimensions}
                        min={1}
                        max={32768}
                        onChange={(value) => updateSetting("semantic_embedding_dimensions", Math.floor(value))}
                    />
                </SettingRow>
                <SettingRow label="Batch Size">
                    <NumberInput
                        value={settings.semantic_embedding_batch_size}
                        min={1}
                        max={512}
                        onChange={(value) => updateSetting("semantic_embedding_batch_size", Math.floor(value))}
                    />
                </SettingRow>
                <SettingRow label="Concurrency">
                    <NumberInput
                        value={settings.semantic_embedding_max_concurrency}
                        min={1}
                        max={16}
                        onChange={(value) => updateSetting("semantic_embedding_max_concurrency", Math.floor(value))}
                    />
                </SettingRow>
                <SettingRow label="Index Status">
                    <div style={{ display: "flex", alignItems: "center", gap: 8, flexWrap: "wrap" }}>
                        <span style={{ fontSize: "var(--text-sm)", color: "var(--muted)" }}>
                            {semanticStatus?.error
                                ? semanticStatus.error
                                : `pending ${semanticStatus?.pending_for_model ?? 0} / indexed ${semanticStatus?.completed_for_model ?? 0} / deletes ${semanticStatus?.queued_deletions ?? 0}`}
                        </span>
                        <RefreshButton onClick={() => void refreshSemanticIndexStatus()} />
                        <button
                            type="button"
                            style={addBtnStyle}
                            onClick={() => void queueSemanticBackfill()}
                            disabled={semanticBackfillBusy}
                        >
                            {semanticBackfillBusy ? "Queueing..." : "Rebuild"}
                        </button>
                    </div>
                </SettingRow>
            </Section>

            <PromptPreviewSection
                refreshKey={[
                    settings.system_prompt,
                    settings.active_provider,
                    providerConfig.model,
                    providerConfig.custom_model_name,
                ].join("|")}
            />

            <Section title={`${PRIMARY_AGENT_NAME} Provider`}>
                    <SettingRow label="Active Provider">
                        <SelectInput value={settings.active_provider}
                            options={providerOptions.map((provider) => provider.id)}
                            onChange={(value) => updateSetting("active_provider", value as AgentProviderId)} />
                    </SettingRow>

                    <div style={{ marginTop: 6, marginBottom: 6, fontSize: "var(--text-sm)", color: "var(--text-secondary)" }}>
                        {allProviderOptions.find((provider) => provider.id === settings.active_provider)?.label}
                    </div>

                    {showUrlEditor ? (
                        <SettingRow label="Base URL">
                            <div style={{ display: "flex", alignItems: "center", gap: 4 }}>
                                <TextInput value={providerConfig.base_url}
                                    onChange={(value) => updateSetting(settings.active_provider, { ...providerConfig, base_url: value })}
                                    placeholder={providerDef?.defaultBaseUrl} />
                                {!isCustomProvider && (
                                    <button type="button" onClick={() => {
                                        updateSetting(settings.active_provider, { ...providerConfig, base_url: "" });
                                        setUseCustomUrl(false);
                                    }} style={smallBtnStyle} title="Reset to predefined default">
                                        Reset
                                    </button>
                                )}
                            </div>
                        </SettingRow>
                    ) : (
                        <SettingRow label="Base URL">
                            <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                                <span style={{
                                    fontSize: "var(--text-sm)",
                                    fontFamily: "var(--font-mono)",
                                    color: "var(--text-muted)",
                                    background: "var(--zorai-bg-surface)",
                                    padding: "3px 8px",
                                    border: "1px solid var(--border)",
                                    flex: 1,
                                }}>
                                    {providerDef?.defaultBaseUrl || "(none)"}
                                </span>
                                {!isCustomProvider && (
                                    <button type="button" onClick={() => setUseCustomUrl(true)} style={smallBtnStyle}>
                                        Override
                                    </button>
                                )}
                            </div>
                        </SettingRow>
                    )}
                    <SettingRow label="Model">
                        <ModelSelector
                            providerId={settings.active_provider}
                            value={providerConfig.model}
                            customName={providerConfig.custom_model_name}
                            onChange={(value, custom_model_name, details) => {
                                const nextCustomModelName = custom_model_name && custom_model_name !== value
                                    ? custom_model_name
                                    : "";
                                const resolvedModel = resolveProviderModelDefinition(
                                    settings.active_provider,
                                    effectiveAuthSource,
                                    value,
                                    nextCustomModelName,
                                );
                                updateSetting(settings.active_provider, {
                                    ...providerConfig,
                                    model: value,
                                    custom_model_name: nextCustomModelName,
                                    context_window_tokens: resolvedModel
                                        ? null
                                        : DEFAULT_CUSTOM_MODEL_CONTEXT_WINDOW,
                                });
                                const effects = getModelSelectionEffects({
                                    enableVisionTool: settings.enable_vision_tool,
                                    currentSttModel: settings.audio_stt_model,
                                    selectedModelId: value,
                                    predefinedModel: details?.predefinedModel ?? resolvedModel,
                                    fetchedModel: details?.fetchedModel ?? null,
                                });
                                if (effects.enableVision) {
                                    updateSetting("enable_vision_tool", true);
                                }
                                if (effects.promptForSttReuse && effects.sttModelOnConfirm) {
                                    setPendingSttReuseModelId(effects.sttModelOnConfirm);
                                } else {
                                    setPendingSttReuseModelId(null);
                                }
                            }}
                            base_url={providerConfig.base_url || providerDef?.defaultBaseUrl}
                            api_key={providerConfig.api_key}
                            auth_source={effectiveAuthSource}
                            allowProviderAuthFetch={Boolean(activeProviderAuthState?.authenticated)}
                        />
                    </SettingRow>
                    {pendingSttReuseModelId ? (
                        <div
                            style={{
                                display: "flex",
                                alignItems: "center",
                                justifyContent: "space-between",
                                gap: 8,
                                marginTop: -2,
                                marginBottom: 10,
                                padding: "8px 10px",
                                border: "1px solid var(--border)",
                                background: "var(--zorai-bg-surface)",
                            }}
                        >
                            <span style={{ fontSize: "var(--text-sm)", color: "var(--text-secondary)" }}>
                                Selected model supports audio. Use it as the STT model too?
                            </span>
                            <div style={{ display: "flex", gap: 6 }}>
                                <button
                                    type="button"
                                    onClick={() => {
                                        updateSetting(
                                            "audio_stt_model",
                                            applySttReuseDecision(
                                                settings.audio_stt_model,
                                                pendingSttReuseModelId,
                                                true,
                                            ),
                                        );
                                        setPendingSttReuseModelId(null);
                                    }}
                                    style={smallBtnStyle}
                                >
                                    Use for STT
                                </button>
                                <button
                                    type="button"
                                    onClick={() => setPendingSttReuseModelId(null)}
                                    style={smallBtnStyle}
                                >
                                    Keep current
                                </button>
                            </div>
                        </div>
                    ) : null}
                    {settings.active_provider === "openrouter" ? (
                        <OpenRouterProviderRoutingControls
                            config={providerConfig}
                            baseUrl={providerConfig.base_url || providerDef?.defaultBaseUrl || "https://openrouter.ai/api/v1"}
                            onChange={(nextConfig) => updateSetting("openrouter", nextConfig)}
                        />
                    ) : null}
                    {providerApiType === "openai" ? (
                        <SettingRow label="Auth">
                            <select
                                value={effectiveAuthSource}
                                onChange={(e) => updateSetting(settings.active_provider, {
                                    ...providerConfig,
                                    auth_source: supportedAuthSources.includes(e.target.value as any)
                                        ? e.target.value as AgentProviderConfig["auth_source"]
                                        : getDefaultAuthSource(settings.active_provider, authSupportOptions),
                                    model: (() => {
                                        const nextAuthSource = supportedAuthSources.includes(e.target.value as any)
                                            ? e.target.value as AgentProviderConfig["auth_source"]
                                            : getDefaultAuthSource(settings.active_provider, authSupportOptions);
                                        const supportedModels = getProviderModels(settings.active_provider, nextAuthSource);
                                        return supportedModels.some((entry) => entry.id === providerConfig.model)
                                            ? providerConfig.model
                                            : getDefaultModelForProvider(settings.active_provider, nextAuthSource);
                                    })(),
                                })}
                                style={inputStyle}
                            >
                                {supportedAuthSources.map((source) => (
                                    <option key={source} value={source}>
                                        {source === "chatgpt_subscription"
                                            ? "ChatGPT Subscription"
                                            : source === "github_copilot"
                                                ? "GitHub Copilot"
                                                : "API Key"}
                                    </option>
                                ))}
                            </select>
                        </SettingRow>
                    ) : null}
                    {settings.active_provider === "openai"
                        && providerConfig.auth_source === "chatgpt_subscription"
                        && effectiveAuthSource !== "chatgpt_subscription" ? (
                        <div style={{ marginTop: 2, marginBottom: 8, fontSize: "var(--text-sm)", color: "var(--text-secondary)", lineHeight: 1.4 }}>
                            ChatGPT Subscription is unavailable for the current backend. Switch to daemon-backed execution to re-enable it.
                        </div>
                    ) : null}
                    <div style={{ marginTop: 2, marginBottom: 8, fontSize: "var(--text-sm)", color: "var(--text-secondary)", lineHeight: 1.4 }}>
                        Credentials are managed in the <strong>Auth</strong> tab. Keep provider selection, model, base URL, and transport here.
                    </div>
                    {settings.active_provider === "openai" && effectiveAuthSource === "chatgpt_subscription" ? (
                        <SettingRow label="ChatGPT Auth">
                            <div style={{ display: "grid", gap: 6, width: "100%" }}>
                                <div style={{ display: "flex", alignItems: "center", gap: 8, justifyContent: "flex-end" }}>
                                    <span style={{ fontSize: "var(--text-sm)", color: subscriptionAuthStatus?.available ? "var(--success, #6ee7b7)" : "var(--text-secondary)" }}>
                                        {subscriptionAuthStatus?.available
                                            ? `Connected (${subscriptionAuthStatus.source || subscriptionAuthStatus.authMode || "zorai"})`
                                            : subscriptionAuthStatus?.error || "No ChatGPT subscription auth found"}
                                    </span>
                                    {subscriptionAuthStatus?.available ? (
                                        <button type="button" onClick={clearSubscriptionAuth} style={smallBtnStyle} disabled={subscriptionAuthBusy}>
                                            {subscriptionAuthBusy ? "Working..." : "Logout"}
                                        </button>
                                    ) : (
                                        <button type="button" onClick={triggerSubscriptionAuth} style={smallBtnStyle} disabled={subscriptionAuthBusy}>
                                            {subscriptionAuthBusy ? "Preparing..." : "Login"}
                                        </button>
                                    )}
                                </div>
                                {subscriptionAuthUi.authUrl ? (
                                    <div style={{ display: "grid", gap: 6, justifyItems: "end" }}>
                                        <a
                                            href={subscriptionAuthUi.authUrl}
                                            target="_blank"
                                            rel="noreferrer"
                                            onClick={(event) => {
                                                event.preventDefault();
                                                openSubscriptionAuthUrl(subscriptionAuthUi.authUrl!);
                                            }}
                                            style={{ fontSize: "var(--text-sm)", color: "var(--accent, #60a5fa)", wordBreak: "break-all", textAlign: "right" }}
                                        >
                                            {subscriptionAuthUi.authUrl}
                                        </a>
                                        <div style={{ display: "flex", gap: 6 }}>
                                            <button
                                                type="button"
                                                onClick={() => openSubscriptionAuthUrl(subscriptionAuthUi.authUrl!)}
                                                style={smallBtnStyle}
                                            >
                                                Open Browser
                                            </button>
                                            <button
                                                type="button"
                                                onClick={() => {
                                                    const zorai = getBridge();
                                                    if (zorai?.writeClipboardText) {
                                                        void zorai.writeClipboardText(subscriptionAuthUi.authUrl!);
                                                        return;
                                                    }
                                                    void navigator.clipboard?.writeText(subscriptionAuthUi.authUrl!).catch(() => { });
                                                }}
                                                style={smallBtnStyle}
                                            >
                                                Copy Link
                                            </button>
                                        </div>
                                    </div>
                                ) : null}
                                {subscriptionAuthUi.authUrl ? (
                                    <div style={{ fontSize: "var(--text-sm)", color: "var(--text-secondary)", textAlign: "right" }}>
                                        Open the link above and complete ChatGPT authentication. This row updates automatically after the callback returns.
                                    </div>
                                ) : null}
                            </div>
                        </SettingRow>
                    ) : null}
                    {providerApiType === "openai" ? (
                        <SettingRow label="API Transport">
                            <select
                                value={providerConfig.api_transport}
                                onChange={(e) => updateSetting(settings.active_provider, {
                                    ...providerConfig,
                                    api_transport: supportedTransports.includes(e.target.value as any)
                                        ? (e.target.value as AgentProviderConfig["api_transport"])
                                        : getDefaultApiTransport(settings.active_provider),
                                })}
                                style={inputStyle}
                            >
                                {supportedTransports.map((transport) => (
                                    <option key={transport} value={transport}>
                                        {transport === "native_assistant"
                                            ? "Native Assistant"
                                            : transport === "anthropic_messages"
                                                ? "Anthropic Messages"
                                            : transport === "responses"
                                                ? "Responses"
                                                : "Legacy Chat Completions"}
                                    </option>
                                ))}
                            </select>
                        </SettingRow>
                    ) : null}
                    {providerConfig.api_transport === "native_assistant" ? (
                        <SettingRow label="Assistant ID">
                            <TextInput
                                value={providerConfig.assistant_id}
                                onChange={(value) => updateSetting(settings.active_provider, {
                                    ...providerConfig,
                                    assistant_id: value,
                                })}
                                placeholder="asst_..."
                            />
                        </SettingRow>
                    ) : null}
                    <SettingRow label="Context Length">
                        {canEditContextWindow ? (
                            <NumberInput
                                value={providerConfig.context_window_tokens ?? effectiveContextWindow}
                                min={16000}
                                max={2000000}
                                step={1000}
                                onChange={(value) => updateSetting(settings.active_provider, {
                                    ...providerConfig,
                                    context_window_tokens: Math.max(1000, Math.trunc(value)),
                                })}
                            />
                        ) : (
                            <span style={{
                                fontSize: "var(--text-sm)",
                                fontFamily: "var(--font-mono)",
                                color: "var(--text-muted)",
                                background: "var(--zorai-bg-surface)",
                                padding: "3px 8px",
                                border: "1px solid var(--border)",
                                minWidth: 120,
                                textAlign: "right",
                            }}>
                                {effectiveContextWindow.toLocaleString()} tok
                            </span>
                        )}
                    </SettingRow>
                    <SettingRow label="Reasoning Effort">
                        <select value={settings.reasoning_effort}
                            onChange={(e) => updateSetting("reasoning_effort", e.target.value as AgentSettings["reasoning_effort"])}
                            style={inputStyle}>
                            <option value="none">None</option>
                            <option value="minimal">Minimal</option>
                            <option value="low">Low</option>
                            <option value="medium">Medium</option>
                            <option value="high">High</option>
                            <option value="xhigh">Extra High</option>
                            <option value="max">Max</option>
                        </select>
                    </SettingRow>
            </Section>

            <Section title="Tools">
                <SettingRow label="Bash Tool">
                    <Toggle value={settings.enable_bash_tool} onChange={(value) => updateSetting("enable_bash_tool", value)} />
                </SettingRow>
                <SettingRow label="Vision Tool">
                    <Toggle value={settings.enable_vision_tool} onChange={(value) => updateSetting("enable_vision_tool", value)} />
                </SettingRow>
                <SettingRow label="Web Browsing Tool">
                    <Toggle value={settings.enable_web_browsing_tool} onChange={(value) => updateSetting("enable_web_browsing_tool", value)} />
                </SettingRow>
                <SettingRow label="Bash Timeout (s)">
                    <NumberInput value={settings.bash_timeout_seconds} min={5} max={300}
                        onChange={(value) => updateSetting("bash_timeout_seconds", value)} />
                </SettingRow>
                <SettingRow label="Web Search Tool">
                    <Toggle value={settings.enable_web_search_tool} onChange={(value) => updateSetting("enable_web_search_tool", value)} />
                </SettingRow>
                {settings.enable_web_search_tool ? (
                    <>
                        <SettingRow label="Search Provider">
                            <SelectInput
                                value={settings.search_provider}
                                options={["none", "firecrawl", "exa", "tavily"]}
                                onChange={(value) => updateSetting("search_provider", value as "none" | "firecrawl" | "exa" | "tavily")}
                            />
                        </SettingRow>
                        <SettingRow label="Firecrawl API Key">
                            <PasswordInput value={settings.firecrawl_api_key} onChange={(value) => updateSetting("firecrawl_api_key", value)} placeholder="fc-..." />
                        </SettingRow>
                        <SettingRow label="Exa API Key">
                            <PasswordInput value={settings.exa_api_key} onChange={(value) => updateSetting("exa_api_key", value)} placeholder="exa_..." />
                        </SettingRow>
                        <SettingRow label="Tavily API Key">
                            <PasswordInput value={settings.tavily_api_key} onChange={(value) => updateSetting("tavily_api_key", value)} placeholder="tvly-..." />
                        </SettingRow>
                        <SettingRow label="Search Max Results">
                            <NumberInput value={settings.search_max_results} min={1} max={20}
                                onChange={(value) => updateSetting("search_max_results", value)} />
                        </SettingRow>
                        <SettingRow label="Search Timeout (s)">
                            <NumberInput value={settings.search_timeout_secs} min={3} max={120}
                                onChange={(value) => updateSetting("search_timeout_secs", value)} />
                        </SettingRow>
                    </>
                ) : null}
            </Section>

            <Section title="Chat">
                <SettingRow label="Streaming">
                    <Toggle value={settings.enable_streaming} onChange={(value) => updateSetting("enable_streaming", value)} />
                </SettingRow>
                <SettingRow label="Conversation Memory">
                    <Toggle value={settings.enable_conversation_memory} onChange={(value) => updateSetting("enable_conversation_memory", value)} />
                </SettingRow>
                <SettingRow label="Honcho Memory">
                    <Toggle value={settings.enable_honcho_memory} onChange={(value) => updateSetting("enable_honcho_memory", value)} />
                </SettingRow>
                {settings.enable_honcho_memory ? (
                    <>
                        <SettingRow label="Honcho API Key">
                            <PasswordInput value={settings.honcho_api_key} onChange={(value) => updateSetting("honcho_api_key", value)} placeholder="hc_..." />
                        </SettingRow>
                        <SettingRow label="Honcho Base URL">
                            <TextInput value={settings.honcho_base_url} onChange={(value) => updateSetting("honcho_base_url", value)} placeholder="Leave blank for managed cloud" />
                        </SettingRow>
                        <SettingRow label="Honcho Workspace">
                            <TextInput value={settings.honcho_workspace_id} onChange={(value) => updateSetting("honcho_workspace_id", value)} placeholder="zorai" />
                        </SettingRow>
                    </>
                ) : null}
                <SettingRow label="Chat Font Family">
                    <TextInput value={settings.chatFontFamily} onChange={(value) => updateSetting("chatFontFamily", value)} />
                </SettingRow>
                <SettingRow label="Chat Font Size">
                    <NumberInput value={settings.chatFontSize} min={10} max={24}
                        onChange={(value) => updateSetting("chatFontSize", value)} />
                </SettingRow>
            </Section>

            <Section title="Self-Orchestrating">
                <SettingRow label="Anticipatory Support">
                    <Toggle value={settings.anticipatory_enabled} onChange={(value) => updateSetting("anticipatory_enabled", value)} />
                </SettingRow>
                {settings.anticipatory_enabled ? (
                    <>
                        <SettingRow label="Morning Brief">
                            <Toggle value={settings.anticipatory_morning_brief} onChange={(value) => updateSetting("anticipatory_morning_brief", value)} />
                        </SettingRow>
                        <SettingRow label="Predictive Hydration">
                            <Toggle value={settings.anticipatory_predictive_hydration} onChange={(value) => updateSetting("anticipatory_predictive_hydration", value)} />
                        </SettingRow>
                        <SettingRow label="Stuck Detection">
                            <Toggle value={settings.anticipatory_stuck_detection} onChange={(value) => updateSetting("anticipatory_stuck_detection", value)} />
                        </SettingRow>
                    </>
                ) : null}
                <SettingRow label="Operator Model">
                    <Toggle value={settings.operator_model_enabled} onChange={(value) => updateSetting("operator_model_enabled", value)} />
                </SettingRow>
                {settings.operator_model_enabled ? (
                    <>
                        <SettingRow label="Message Statistics">
                            <Toggle value={settings.operator_model_allow_message_statistics} onChange={(value) => updateSetting("operator_model_allow_message_statistics", value)} />
                        </SettingRow>
                        <SettingRow label="Approval Learning">
                            <Toggle value={settings.operator_model_allow_approval_learning} onChange={(value) => updateSetting("operator_model_allow_approval_learning", value)} />
                        </SettingRow>
                        <SettingRow label="Attention Tracking">
                            <Toggle value={settings.operator_model_allow_attention_tracking} onChange={(value) => updateSetting("operator_model_allow_attention_tracking", value)} />
                        </SettingRow>
                        <SettingRow label="Implicit Feedback">
                            <Toggle value={settings.operator_model_allow_implicit_feedback} onChange={(value) => updateSetting("operator_model_allow_implicit_feedback", value)} />
                        </SettingRow>
                        <OperatorModelControls enabled={settings.operator_model_enabled} />
                    </>
                ) : null}
                <SettingRow label="Collaboration">
                    <Toggle value={settings.collaboration_enabled} onChange={(value) => updateSetting("collaboration_enabled", value)} />
                </SettingRow>
                <SettingRow label="Compliance Mode">
                    <select value={settings.compliance_mode}
                        onChange={(e) => updateSetting("compliance_mode", e.target.value as AgentSettings["compliance_mode"])}
                        style={inputStyle}>
                        <option value="standard">Standard</option>
                        <option value="soc2">SOC 2</option>
                        <option value="hipaa">HIPAA</option>
                        <option value="fedramp">FedRAMP</option>
                    </select>
                </SettingRow>
                <SettingRow label="Retention Days">
                    <NumberInput value={settings.compliance_retention_days} min={1} max={3650}
                        onChange={(value) => updateSetting("compliance_retention_days", value)} />
                </SettingRow>
                <SettingRow label="Sign All Events">
                    <Toggle value={settings.compliance_sign_all_events} onChange={(value) => updateSetting("compliance_sign_all_events", value)} />
                </SettingRow>
                <SettingRow label="Tool Synthesis">
                    <Toggle value={settings.tool_synthesis_enabled} onChange={(value) => updateSetting("tool_synthesis_enabled", value)} />
                </SettingRow>
                {settings.tool_synthesis_enabled ? (
                    <>
                        <SettingRow label="Require Activation">
                            <Toggle value={settings.tool_synthesis_require_activation} onChange={(value) => updateSetting("tool_synthesis_require_activation", value)} />
                        </SettingRow>
                        <SettingRow label="Generated Tool Limit">
                            <NumberInput value={settings.tool_synthesis_max_generated_tools} min={1} max={200}
                                onChange={(value) => updateSetting("tool_synthesis_max_generated_tools", value)} />
                        </SettingRow>
                        <GeneratedToolsPanel enabled={settings.tool_synthesis_enabled} />
                    </>
                ) : null}
            </Section>

            <Section title="Skill Discovery">
                <SettingRow label="Local Skill Gate">
                    <Toggle
                        value={settings.skill_recommendation.enabled}
                        onChange={(value) =>
                            updateSetting("skill_recommendation", {
                                ...settings.skill_recommendation,
                                enabled: value,
                            })}
                    />
                </SettingRow>
                <SettingRow label="Background Community Scout">
                    <Toggle
                        value={settings.skill_recommendation.background_community_search}
                        onChange={(value) =>
                            updateSetting("skill_recommendation", {
                                ...settings.skill_recommendation,
                                background_community_search: value,
                            })}
                    />
                </SettingRow>
                <SettingRow label="Scout Prompt Timeout (s)">
                    <NumberInput
                        value={settings.skill_recommendation.community_preapprove_timeout_secs}
                        min={5}
                        max={300}
                        step={5}
                        onChange={(value) =>
                            updateSetting("skill_recommendation", {
                                ...settings.skill_recommendation,
                                community_preapprove_timeout_secs: value,
                            })}
                    />
                </SettingRow>
                <SettingRow label="Suggest Global Enable After">
                    <NumberInput
                        value={settings.skill_recommendation.suggest_global_enable_after_approvals}
                        min={1}
                        max={12}
                        onChange={(value) =>
                            updateSetting("skill_recommendation", {
                                ...settings.skill_recommendation,
                                suggest_global_enable_after_approvals: value,
                            })}
                    />
                </SettingRow>
                <div style={{ marginTop: 4, marginBottom: 8, fontSize: "var(--text-sm)", color: "var(--text-secondary)", lineHeight: 1.4 }}>
                    Local installed skills remain authoritative. Community discovery is advisory, non-blocking, and only used to surface install candidates with a short operator approval window.
                </div>
            </Section>

            <Section title="Context Compaction">
                <SettingRow label="Auto Compact">
                    <Toggle value={settings.auto_compact_context} onChange={(value) => updateSetting("auto_compact_context", value)} />
                </SettingRow>
                <SettingRow label="Strategy">
                    <select
                        value={settings.compaction.strategy}
                        onChange={(e) => updateSetting("compaction", {
                            ...settings.compaction,
                            strategy: e.target.value as AgentSettings["compaction"]["strategy"],
                        })}
                        style={inputStyle}
                    >
                        <option value="heuristic">Heuristic</option>
                        <option value="weles">WELES</option>
                        <option value="custom_model">Custom model</option>
                    </select>
                </SettingRow>
                <SettingRow label="Heuristic Max Context Messages">
                    <NumberInput value={settings.max_context_messages} min={10} max={500}
                        onChange={(value) => updateSetting("max_context_messages", value)} />
                </SettingRow>
                <div style={{ marginTop: 4, marginBottom: 8, fontSize: "var(--text-sm)", color: "var(--text-secondary)", lineHeight: 1.4 }}>
                    Applies only to heuristic compaction. WELES and custom-model compaction trigger on token budget only.
                </div>
                <SettingRow label="TUI Thread History">
                    <NumberInput
                        value={settings.tui_chat_history_page_size}
                        min={MIN_CHAT_HISTORY_PAGE_SIZE}
                        max={MAX_TUI_CHAT_HISTORY_PAGE_SIZE}
                        onChange={(value) =>
                            updateSetting("tui_chat_history_page_size", normalizeTuiChatHistoryPageSize(value))}
                    />
                </SettingRow>
                <SettingRow label="React Thread History">
                    <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                        <input
                            type="number"
                            value={reactHistoryIsUnlimited ? "" : settings.react_chat_history_page_size}
                            min={MIN_CHAT_HISTORY_PAGE_SIZE}
                            step={1}
                            placeholder={reactHistoryIsUnlimited ? "All" : undefined}
                            disabled={reactHistoryIsUnlimited}
                            onChange={(event) => {
                                const nextValue = normalizeReactChatHistoryPageSize(event.target.value);
                                if (nextValue !== REACT_CHAT_HISTORY_PAGE_SIZE_ALL) {
                                    updateSetting("react_chat_history_page_size", nextValue);
                                }
                            }}
                            style={{
                                ...inputStyle,
                                width: 80,
                                opacity: reactHistoryIsUnlimited ? 0.6 : 1,
                                cursor: reactHistoryIsUnlimited ? "not-allowed" : "text",
                            }}
                        />
                        <button
                            type="button"
                            onClick={() => updateSetting(
                                "react_chat_history_page_size",
                                reactHistoryIsUnlimited
                                    ? DEFAULT_CHAT_HISTORY_PAGE_SIZE
                                    : REACT_CHAT_HISTORY_PAGE_SIZE_ALL,
                            )}
                            style={{
                                ...smallBtnStyle,
                                borderColor: reactHistoryIsUnlimited ? "var(--accent)" : "var(--border)",
                                color: reactHistoryIsUnlimited ? "var(--accent)" : "var(--text-primary)",
                            }}
                        >
                            All
                        </button>
                    </div>
                </SettingRow>
                <SettingRow label="Startup Restore Hours">
                    <NumberInput
                        value={settings.participant_observer_restore_window_hours}
                        min={0}
                        max={24 * 30}
                        onChange={(value) =>
                            updateSetting("participant_observer_restore_window_hours", value)}
                    />
                </SettingRow>
                <SettingRow label="Max Tool Loops">
                    <NumberInput value={settings.max_tool_loops} min={0} max={1000}
                        onChange={(value) => updateSetting("max_tool_loops", value)} />
                </SettingRow>
                <SettingRow label="429 Max Retries">
                    <NumberInput value={settings.max_retries} min={0} max={10}
                        onChange={(value) => updateSetting("max_retries", value)} />
                </SettingRow>
                <SettingRow label="429 Retry Delay (ms)">
                    <NumberInput value={settings.retry_delay_ms} min={100} max={60000} step={100}
                        onChange={(value) => updateSetting("retry_delay_ms", value)} />
                </SettingRow>
                <SettingRow label="Message Loop Delay (ms)">
                    <input
                        type="number"
                        value={settings.message_loop_delay_ms}
                        min={0}
                        max={60000}
                        step={100}
                        onChange={(event) => {
                            const nextValue = Number.parseFloat(event.target.value);
                            if (!Number.isNaN(nextValue)) {
                                updateSetting("message_loop_delay_ms", nextValue);
                            }
                        }}
                        style={daemonDelayInputStyle}
                    />
                </SettingRow>
                <SettingRow label="Tool Call Gap (ms)">
                    <input
                        type="number"
                        value={settings.tool_call_delay_ms}
                        min={0}
                        max={60000}
                        step={100}
                        onChange={(event) => {
                            const nextValue = Number.parseFloat(event.target.value);
                            if (!Number.isNaN(nextValue)) {
                                updateSetting("tool_call_delay_ms", nextValue);
                            }
                        }}
                        style={daemonDelayInputStyle}
                    />
                </SettingRow>
                <SettingRow label="LLM Stream Timeout (s)">
                    <input
                        type="number"
                        value={settings.llm_stream_chunk_timeout_secs}
                        min={30}
                        max={1800}
                        step={10}
                        onChange={(event) => {
                            const nextValue = normalizeLlmStreamTimeoutInput(event.target.value);
                            if (nextValue !== null) {
                                updateSetting("llm_stream_chunk_timeout_secs", nextValue);
                            }
                        }}
                        style={inputStyle}
                    />
                </SettingRow>
                <SettingRow label="Auto Retry">
                    <Toggle value={settings.auto_retry}
                        onChange={(value) => updateSetting("auto_retry", value)} />
                </SettingRow>
                <SettingRow label="Compact Threshold %">
                    <NumberInput value={settings.compact_threshold_pct} min={50} max={95}
                        onChange={(value) => updateSetting("compact_threshold_pct", value)} />
                </SettingRow>
                <SettingRow label="Keep Recent on Compact">
                    <NumberInput value={settings.keep_recent_on_compact} min={1} max={50}
                        onChange={(value) => updateSetting("keep_recent_on_compact", value)} />
                </SettingRow>
                <SettingRow label="WELES Max Reviews">
                    <NumberInput
                        value={settings.weles_max_concurrent_reviews}
                        min={1}
                        max={16}
                        onChange={(value) => updateSetting("weles_max_concurrent_reviews", value)}
                    />
                </SettingRow>

                {settings.compaction.strategy === "weles" ? (
                    <>
                        <SettingRow label="WELES Provider">
                            <SelectInput
                                value={settings.compaction.weles.provider}
                                options={allProviderOptions.map((provider) => provider.id)}
                                onChange={(value) => updateSetting("compaction", {
                                    ...settings.compaction,
                                    weles: { ...settings.compaction.weles, provider: value as AgentProviderId },
                                })}
                            />
                        </SettingRow>
                        <SettingRow label="WELES Model">
                            <TextInput
                                value={settings.compaction.weles.model}
                                onChange={(value) => updateSetting("compaction", {
                                    ...settings.compaction,
                                    weles: { ...settings.compaction.weles, model: value },
                                })}
                            />
                        </SettingRow>
                        <SettingRow label="WELES Reasoning">
                            <select
                                value={settings.compaction.weles.reasoning_effort}
                                onChange={(e) => updateSetting("compaction", {
                                    ...settings.compaction,
                                    weles: {
                                        ...settings.compaction.weles,
                                        reasoning_effort: e.target.value as AgentSettings["reasoning_effort"],
                                    },
                                })}
                                style={inputStyle}
                            >
                                <option value="none">none</option>
                                <option value="minimal">minimal</option>
                                <option value="low">low</option>
                                <option value="medium">medium</option>
                                <option value="high">high</option>
                                <option value="xhigh">xhigh</option>
                                <option value="max">max</option>
                            </select>
                        </SettingRow>
                        <SettingRow label="WELES Transport">
                            <select
                                value={settings.compaction.weles.api_transport
                                    ?? getDefaultApiTransport(settings.compaction.weles.provider)}
                                onChange={(e) => updateSetting("compaction", {
                                    ...settings.compaction,
                                    weles: {
                                        ...settings.compaction.weles,
                                        api_transport: e.target.value as AgentProviderConfig["api_transport"],
                                    },
                                })}
                                style={inputStyle}
                            >
                                {getSupportedApiTransports(settings.compaction.weles.provider).map((transport) => (
                                    <option key={transport} value={transport}>{transport}</option>
                                ))}
                            </select>
                        </SettingRow>
                    </>
                ) : null}

                {settings.compaction.strategy === "custom_model" ? (
                    <>
                        <SettingRow label="Custom Provider">
                            <SelectInput
                                value={settings.compaction.custom_model.provider}
                                options={allProviderOptions.map((provider) => provider.id)}
                                onChange={(value) => updateSetting("compaction", {
                                    ...settings.compaction,
                                    custom_model: { ...settings.compaction.custom_model, provider: value as AgentProviderId },
                                })}
                            />
                        </SettingRow>
                        <SettingRow label="Custom Base URL">
                            <TextInput
                                value={settings.compaction.custom_model.base_url}
                                onChange={(value) => updateSetting("compaction", {
                                    ...settings.compaction,
                                    custom_model: { ...settings.compaction.custom_model, base_url: value },
                                })}
                            />
                        </SettingRow>
                        <SettingRow label="Custom Model">
                            <TextInput
                                value={settings.compaction.custom_model.model}
                                onChange={(value) => updateSetting("compaction", {
                                    ...settings.compaction,
                                    custom_model: { ...settings.compaction.custom_model, model: value },
                                })}
                            />
                        </SettingRow>
                        <SettingRow label="Custom API Key">
                            <PasswordInput
                                value={settings.compaction.custom_model.api_key}
                                onChange={(value) => updateSetting("compaction", {
                                    ...settings.compaction,
                                    custom_model: { ...settings.compaction.custom_model, api_key: value },
                                })}
                            />
                        </SettingRow>
                        <SettingRow label="Custom Assistant ID">
                            <TextInput
                                value={settings.compaction.custom_model.assistant_id}
                                onChange={(value) => updateSetting("compaction", {
                                    ...settings.compaction,
                                    custom_model: { ...settings.compaction.custom_model, assistant_id: value },
                                })}
                            />
                        </SettingRow>
                        <SettingRow label="Custom Auth">
                            <select
                                value={settings.compaction.custom_model.auth_source}
                                onChange={(e) => updateSetting("compaction", {
                                    ...settings.compaction,
                                    custom_model: {
                                        ...settings.compaction.custom_model,
                                        auth_source: e.target.value as AgentSettings["compaction"]["custom_model"]["auth_source"],
                                    },
                                })}
                                style={inputStyle}
                            >
                                <option value="api_key">api_key</option>
                                <option value="chatgpt_subscription">chatgpt_subscription</option>
                                <option value="github_copilot">github_copilot</option>
                            </select>
                        </SettingRow>
                        <SettingRow label="Custom Transport">
                            <select
                                value={settings.compaction.custom_model.api_transport}
                                onChange={(e) => updateSetting("compaction", {
                                    ...settings.compaction,
                                    custom_model: {
                                        ...settings.compaction.custom_model,
                                        api_transport: e.target.value as AgentSettings["compaction"]["custom_model"]["api_transport"],
                                    },
                                })}
                                style={inputStyle}
                            >
                                <option value="responses">responses</option>
                                <option value="anthropic_messages">anthropic_messages</option>
                                <option value="chat_completions">chat_completions</option>
                                <option value="native_assistant">native_assistant</option>
                            </select>
                        </SettingRow>
                        <SettingRow label="Custom Reasoning">
                            <select
                                value={settings.compaction.custom_model.reasoning_effort}
                                onChange={(e) => updateSetting("compaction", {
                                    ...settings.compaction,
                                    custom_model: {
                                        ...settings.compaction.custom_model,
                                        reasoning_effort: e.target.value as AgentSettings["reasoning_effort"],
                                    },
                                })}
                                style={inputStyle}
                            >
                                <option value="none">none</option>
                                <option value="minimal">minimal</option>
                                <option value="low">low</option>
                                <option value="medium">medium</option>
                                <option value="high">high</option>
                                <option value="xhigh">xhigh</option>
                                <option value="max">max</option>
                            </select>
                        </SettingRow>
                        <SettingRow label="Custom Context Window">
                            <NumberInput
                                value={settings.compaction.custom_model.context_window_tokens}
                                min={1000}
                                max={2000000}
                                step={1000}
                                onChange={(value) => updateSetting("compaction", {
                                    ...settings.compaction,
                                    custom_model: {
                                        ...settings.compaction.custom_model,
                                        context_window_tokens: value,
                                    },
                                })}
                            />
                        </SettingRow>
                    </>
                ) : null}
            </Section>

            <div style={{ marginTop: 12 }}>
                <button onClick={resetSettings} style={addBtnStyle}>Reset Agent Settings</button>
            </div>
        </>
    );
}
