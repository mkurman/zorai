import { useEffect, useMemo, useState, type ReactNode } from "react";
import { RefreshButton } from "@/zorai/shell/RefreshButton";
import { SubAgentsTab } from "@/components/settings-panel/SubAgentsTab";
import { ModelSelector } from "@/components/settings-panel/shared";
import {
  audioModelOptions,
  audioRemoteModelFetchOutputModalities,
  embeddingModelOptions,
  filterAudioProviderOptions,
  filterEmbeddingProviderOptions,
  filterImageGenerationProviderOptions,
  imageGenerationModelOptions,
  imageRemoteModelFetchOutputModalities,
  normalizeAudioModelForProviderChange,
  normalizeEmbeddingModelForProviderChange,
  normalizeImageGenerationModelForProviderChange,
  type ProviderOption,
} from "@/components/settings-panel/agentTabHelpers";
import { useAgentStore, type AgentSettings } from "@/lib/agentStore";
import {
  getDefaultApiTransport,
  getDefaultAuthSource,
  getDefaultModelForProvider,
  getProviderDefinition,
  getSupportedApiTransports,
  normalizeApiTransport,
} from "@/lib/agentStore/providers";
import type { AgentProviderConfig, AgentProviderId, ApiTransportMode, AuthSource } from "@/lib/agentStore/types";
import { getBridge } from "@/lib/bridge";
import { filterFetchedModelsForAudio, filterFetchedModelsForEmbeddings, filterFetchedModelsForImageGeneration } from "@/lib/providerModels";
import { usePluginStore } from "@/lib/pluginStore";
import { useSettingsStore } from "@/lib/settingsStore";
import { getWorkspaceSettings, setWorkspaceRepoMonitor } from "@/lib/workspaceBoard";
import { ZORAI_APP_NAME } from "@/zorai/branding";
import { applyRarogContextWindow } from "../threads/threadRuntimeActions";
import { canonicalThreadAgentId } from "../threads/threadFilterModel";
import { embeddingSettingsPatchForModelSelection } from "./embeddingSettings";
import { InterfacePanel } from "./InterfacePanel";
import { duckDuckGoSafeSearchOptions, searchProviderOptions } from "./searchProviders";
import { MlflowPanel } from "./MlflowPanel";
import type { ZoraiSettingsTabId } from "./settingsTabs";
import {
  formatProviderValidationError,
  getProviderValidationButtonLabel,
  markProviderValidationTesting,
  providerValidationStatusFromResult,
  type ProviderValidationStatusMap,
} from "./providerValidationStatus";

type ToggleSetting = {
  key: keyof AgentSettings;
  label: string;
  description: string;
};

const authSources: AuthSource[] = ["api_key", "chatgpt_subscription", "github_copilot"];
const reasoningEfforts: AgentSettings["reasoning_effort"][] = ["none", "minimal", "low", "medium", "high", "xhigh", "max"];
export const conciergeReasoningEffortOptions: Array<{ value: AgentSettings["reasoning_effort"]; label: string }> = [
  { value: "none", label: "No" },
  { value: "minimal", label: "minimal" },
  { value: "low", label: "low" },
  { value: "medium", label: "medium" },
  { value: "high", label: "high" },
  { value: "xhigh", label: "xhigh" },
  { value: "max", label: "max" },
];
const APP_VERSION = "0.9.65";
const APP_AUTHOR = "Mariusz Kurman";
const APP_GITHUB = "mkurman/zorai";
const APP_HOMEPAGE = "zorai.app";
const DEFAULT_WORKSPACE_ID = "main";

const toolToggles: ToggleSetting[] = [
  { key: "enable_bash_tool", label: "Terminal tool", description: "Allow agents to execute managed shell commands." },
  { key: "enable_vision_tool", label: "Vision tool", description: "Allow screenshot and image inspection workflows." },
  { key: "enable_web_browsing_tool", label: "Browser tool", description: "Allow browser-backed research actions." },
  { key: "enable_web_search_tool", label: "Web search", description: "Allow configured search provider access." },
  { key: "enable_streaming", label: "Streaming", description: "Stream assistant output as it arrives." },
  { key: "enable_conversation_memory", label: "Conversation memory", description: "Keep durable context across agent sessions." },
  { key: "auto_retry", label: "Auto retry", description: "Retry recoverable provider and tool failures." },
];

export function normalizeConciergeReasoningEffortForUi(value: string | null | undefined): AgentSettings["reasoning_effort"] {
  const normalized = value?.trim().toLowerCase();
  if (!normalized || normalized === "off" || normalized === "none") {
    return "none";
  }
  return conciergeReasoningEffortOptions.some((option) => option.value === normalized)
    ? normalized as AgentSettings["reasoning_effort"]
    : "none";
}

export function serializeConciergeReasoningEffortFromUi(value: AgentSettings["reasoning_effort"]): string | undefined {
  return value === "none" ? undefined : value;
}

export function SettingsTabPanel({ activeTab }: { activeTab: ZoraiSettingsTabId }) {
  if (activeTab === "model") return <ModelPanel />;
  if (activeTab === "auth") return <AuthPanel />;
  if (activeTab === "interface") return <InterfacePanel />;
  if (activeTab === "tools") return <ToolsPanel />;
  if (activeTab === "search") return <SearchPanel />;
  if (activeTab === "mlflow") return <MlflowPanel />;
  if (activeTab === "concierge") return <ConciergePanel />;
  if (activeTab === "subagents") return <SubAgentsPanel />;
  if (activeTab === "gateway") return <GatewayPanel />;
  if (activeTab === "features") return <FeaturesPanel />;
  if (activeTab === "advanced") return <AdvancedPanel />;
  if (activeTab === "plugins") return <PluginsPanel />;
  if (activeTab === "about") return <AboutPanel />;
  return <ChatPanel />;
}

function ChatPanel() {
  const agentSettings = useAgentStore((state) => state.agentSettings);
  const updateAgentSetting = useAgentStore((state) => state.updateAgentSetting);

  return (
    <SettingsGrid>
      <Panel section="Chat" title="Streaming and memory">
        <SettingRow label="Streaming" description="Stream assistant output as it arrives."><Switch checked={agentSettings.enable_streaming} onChange={(checked) => updateAgentSetting("enable_streaming", checked)} /></SettingRow>
        <SettingRow label="Conversation Memory" description="Keep durable context across agent sessions."><Switch checked={agentSettings.enable_conversation_memory} onChange={(checked) => updateAgentSetting("enable_conversation_memory", checked)} /></SettingRow>
        <SettingRow label="Honcho Memory" description="Enable Honcho-backed memory when configured."><Switch checked={agentSettings.enable_honcho_memory} onChange={(checked) => updateAgentSetting("enable_honcho_memory", checked)} /></SettingRow>
        <SettingRow label="Visible Msgs" description="Messages visible in the terminal chat history page.">
          <input className="zorai-input" type="number" min={25} max={1000} value={agentSettings.tui_chat_history_page_size} onChange={(event) => updateAgentSetting("tui_chat_history_page_size", Number(event.target.value))} />
        </SettingRow>
        <SettingRow label="Restore Hours" description="Recent lazy participant threads and stalled-turn supervision scanned by the daemon on startup.">
          <input className="zorai-input" type="number" min={0} max={24 * 30} value={agentSettings.participant_observer_restore_window_hours} onChange={(event) => updateAgentSetting("participant_observer_restore_window_hours", Number(event.target.value))} />
        </SettingRow>
      </Panel>
      <Panel section="Chat" title="Operational intelligence">
        <SettingRow label="Anticipatory Support" description="Enable proactive operator support."><Switch checked={agentSettings.anticipatory_enabled} onChange={(checked) => updateAgentSetting("anticipatory_enabled", checked)} /></SettingRow>
        <SettingRow label="Morning Brief" description="Include morning brief generation."><Switch checked={agentSettings.anticipatory_morning_brief} onChange={(checked) => updateAgentSetting("anticipatory_morning_brief", checked)} /></SettingRow>
        <SettingRow label="Predictive Hydration" description="Hydrate likely-needed context ahead of time."><Switch checked={agentSettings.anticipatory_predictive_hydration} onChange={(checked) => updateAgentSetting("anticipatory_predictive_hydration", checked)} /></SettingRow>
        <SettingRow label="Stuck Detection" description="Detect stalled work and stale execution."><Switch checked={agentSettings.anticipatory_stuck_detection} onChange={(checked) => updateAgentSetting("anticipatory_stuck_detection", checked)} /></SettingRow>
        <SettingRow label="Operator Model" description="Learn operator preferences from local activity."><Switch checked={agentSettings.operator_model_enabled} onChange={(checked) => updateAgentSetting("operator_model_enabled", checked)} /></SettingRow>
        <SettingRow label="Collaboration" description="Enable multi-agent collaboration features."><Switch checked={agentSettings.collaboration_enabled} onChange={(checked) => updateAgentSetting("collaboration_enabled", checked)} /></SettingRow>
      </Panel>
      <Panel section="Chat" title="Compliance and synthesis">
        <SettingRow label="Compliance" description="Audit mode for retained events.">
          <select className="zorai-input" value={agentSettings.compliance_mode} onChange={(event) => updateAgentSetting("compliance_mode", event.target.value as AgentSettings["compliance_mode"])}>
            {["standard", "soc2", "hipaa", "fedramp"].map((value) => <option key={value} value={value}>{value}</option>)}
          </select>
        </SettingRow>
        <NumberRow label="Retention Days" description="Compliance event retention window." value={agentSettings.compliance_retention_days} onChange={(value) => updateAgentSetting("compliance_retention_days", value)} min={1} max={3650} />
        <SettingRow label="Sign All Events" description="Sign compliance audit events."><Switch checked={agentSettings.compliance_sign_all_events} onChange={(checked) => updateAgentSetting("compliance_sign_all_events", checked)} /></SettingRow>
        <SettingRow label="Tool Synthesis" description="Allow generated tool workflows."><Switch checked={agentSettings.tool_synthesis_enabled} onChange={(checked) => updateAgentSetting("tool_synthesis_enabled", checked)} /></SettingRow>
        <SettingRow label="Require Activation" description="Require generated tools to be activated before use."><Switch checked={agentSettings.tool_synthesis_require_activation} onChange={(checked) => updateAgentSetting("tool_synthesis_require_activation", checked)} /></SettingRow>
        <NumberRow label="Tool Limit" description="Maximum generated tools." value={agentSettings.tool_synthesis_max_generated_tools} onChange={(value) => updateAgentSetting("tool_synthesis_max_generated_tools", value)} min={0} max={100} />
      </Panel>
    </SettingsGrid>
  );
}

function ModelPanel() {
  const agentSettings = useAgentStore((state) => state.agentSettings);
  const updateAgentSetting = useAgentStore((state) => state.updateAgentSetting);
  const providerAuthStates = useAgentStore((state) => state.providerAuthStates);
  const providerIds = useProviderIds(agentSettings);
  const activeProvider = agentSettings.active_provider;
  const activeProviderConfig = agentSettings[activeProvider] ?? {};
  const updateProviderConfig = (patch: Record<string, unknown>) => updateAgentSetting(activeProvider, { ...activeProviderConfig, ...patch });
  const supportedTransports = getSupportedApiTransports(activeProvider);
  const transportValue = normalizeApiTransport(activeProvider, activeProviderConfig.api_transport);
  const providerAuthenticated = Boolean(providerAuthStates.find((entry) => entry.provider_id === activeProvider)?.authenticated);

  return (
    <SettingsGrid>
      <Panel section="Svarog" title="Svarog Provider">
        <SettingRow label="Provider" description="Provider used by the primary agent.">
          <select className="zorai-input" value={activeProvider} onChange={(event) => updateAgentSetting("active_provider", event.target.value as AgentSettings["active_provider"])}>
            {providerIds.map((providerId) => <option key={providerId} value={providerId}>{providerId}</option>)}
          </select>
        </SettingRow>
        <SettingRow label="Model" description="Default model for this provider.">
          <ModelSelector
            providerId={activeProvider}
            value={String(activeProviderConfig.model ?? "")}
            customName={String(activeProviderConfig.custom_model_name ?? "")}
            onChange={(value, customModelName, details) => updateProviderConfig({
              model: value,
              custom_model_name: customModelName && customModelName !== value ? customModelName : "",
              context_window_tokens: details?.predefinedModel?.contextWindow ?? details?.fetchedModel?.contextWindow ?? activeProviderConfig.context_window_tokens,
            })}
            base_url={String(activeProviderConfig.base_url ?? "")}
            api_key={String(activeProviderConfig.api_key ?? "")}
            auth_source={activeProviderConfig.auth_source}
            allowProviderAuthFetch={providerAuthenticated}
          />
        </SettingRow>
        <SettingRow label="Transport" description="Provider API transport mode.">
          <select className="zorai-input" value={transportValue} onChange={(event) => updateProviderConfig({ api_transport: normalizeApiTransport(activeProvider, event.target.value) })}>
            {supportedTransports.map((transport) => <option key={transport} value={transport}>{formatTransportLabel(transport)}</option>)}
          </select>
        </SettingRow>
        <SettingRow label="Base URL" description="Optional OpenAI-compatible endpoint override.">
          <input className="zorai-input" value={String(activeProviderConfig.base_url ?? "")} onChange={(event) => updateProviderConfig({ base_url: event.target.value })} />
        </SettingRow>
        <SettingRow label="Assistant ID" description="Optional native assistant identifier.">
          <input className="zorai-input" value={String(activeProviderConfig.assistant_id ?? "")} onChange={(event) => updateProviderConfig({ assistant_id: event.target.value })} />
        </SettingRow>
        <SettingRow label="Effort" description="Svarog reasoning effort.">
          <select className="zorai-input" value={agentSettings.reasoning_effort} onChange={(event) => updateAgentSetting("reasoning_effort", event.target.value as AgentSettings["reasoning_effort"])}>
            {reasoningEfforts.map((value) => <option key={value} value={value}>{value}</option>)}
          </select>
        </SettingRow>
        <NumberRow label="Ctx Length" description="Context length override in tokens." value={Number(activeProviderConfig.context_window_tokens ?? 0)} onChange={(value) => updateProviderConfig({ context_window_tokens: value || null })} min={0} max={2000000} />
      </Panel>
      <Panel section="Svarog" title="Main agent identity and behavior">
        <Metric label="Fixed Name" value="Svarog" />
        <SettingRow label="System Prompt" description="Primary agent identity prompt.">
          <textarea className="zorai-input" value={agentSettings.system_prompt} onChange={(event) => updateAgentSetting("system_prompt", event.target.value)} />
        </SettingRow>
        <Metric label="Backend" value="daemon" />
      </Panel>
      <MigrationPanel />
    </SettingsGrid>
  );
}

function MigrationPanel() {
  const bridge = getBridge();
  const [status, setStatus] = useState<unknown | null>(null);
  const [result, setResult] = useState<unknown | null>(null);
  const [configPath, setConfigPath] = useState("");
  const [busy, setBusy] = useState<string | null>(null);

  const runMigration = async (
    label: string,
    action: () => Promise<unknown> | undefined,
  ) => {
    setBusy(label);
    try {
      const payload = await action();
      setResult(payload ?? null);
    } catch (error) {
      setResult({ error: error instanceof Error ? error.message : String(error) });
    } finally {
      setBusy(null);
    }
  };

  const preview = (runtime: "hermes" | "openclaw") => runMigration(
    `preview-${runtime}`,
    () => bridge?.agentExternalRuntimeMigrationPreview?.(runtime, configPath.trim() || null),
  );
  const apply = (runtime: "hermes" | "openclaw") => runMigration(
    `apply-${runtime}`,
    () => bridge?.agentExternalRuntimeMigrationApply?.(runtime, configPath.trim() || null, "stage_for_review"),
  );

  return (
    <Panel section="Migration" title="Migrate from existing agents">
      <button
        type="button"
        className="zorai-ghost-button"
        onClick={() => void runMigration("status", async () => {
          const payload = await bridge?.agentExternalRuntimeMigrationStatus?.();
          setStatus(payload ?? null);
          return payload;
        })}
        disabled={busy !== null || !bridge?.agentExternalRuntimeMigrationStatus}
      >
        {busy === "status" ? "Checking..." : "Check installed agents"}
      </button>
      <SettingRow label="Config Path" description="Optional Hermes/OpenClaw config path override.">
        <input className="zorai-input" value={configPath} onChange={(event) => setConfigPath(event.target.value)} placeholder="Use detected default" />
      </SettingRow>
      <SettingRow label="Migrate from Hermes" description="Preview or import Hermes config into Zorai.">
        <div className="zorai-card-actions">
          <button type="button" className="zorai-ghost-button" onClick={() => void preview("hermes")} disabled={busy !== null || !bridge?.agentExternalRuntimeMigrationPreview}>Preview</button>
          <button type="button" className="zorai-ghost-button" onClick={() => void apply("hermes")} disabled={busy !== null || !bridge?.agentExternalRuntimeMigrationApply}>Import</button>
        </div>
      </SettingRow>
      <SettingRow label="Migrate from OpenClaw" description="Preview or import OpenClaw config into Zorai.">
        <div className="zorai-card-actions">
          <button type="button" className="zorai-ghost-button" onClick={() => void preview("openclaw")} disabled={busy !== null || !bridge?.agentExternalRuntimeMigrationPreview}>Preview</button>
          <button type="button" className="zorai-ghost-button" onClick={() => void apply("openclaw")} disabled={busy !== null || !bridge?.agentExternalRuntimeMigrationApply}>Import</button>
        </div>
      </SettingRow>
      <button type="button" className="zorai-ghost-button" onClick={() => void runMigration("report", () => bridge?.agentExternalRuntimeMigrationReport?.(null, 20))} disabled={busy !== null || !bridge?.agentExternalRuntimeMigrationReport}>Show migration report</button>
      {status ? <pre className="zorai-empty-state">{JSON.stringify(status, null, 2)}</pre> : null}
      {result ? <pre className="zorai-empty-state">{JSON.stringify(result, null, 2)}</pre> : null}
    </Panel>
  );
}

function AuthPanel() {
  const authStates = useAgentStore((state) => state.providerAuthStates);
  const refreshAuth = useAgentStore((state) => state.refreshProviderAuthStates);
  const validateProvider = useAgentStore((state) => state.validateProvider);
  const loginProvider = useAgentStore((state) => state.loginProvider);
  const logoutProvider = useAgentStore((state) => state.logoutProvider);
  const [loginTarget, setLoginTarget] = useState<string | null>(null);
  const [loginKey, setLoginKey] = useState("");
  const [validationResult, setValidationResult] = useState<ProviderValidationStatusMap>({});

  useEffect(() => {
    void refreshAuth();
  }, [refreshAuth]);

  const runTest = async (providerId: string, baseUrl: string, authSource: string) => {
    setValidationResult((items) => markProviderValidationTesting(items, providerId));
    try {
      const result = await validateProvider(providerId, baseUrl, "", authSource);
      setValidationResult((items) => ({
        ...items,
        [providerId]: providerValidationStatusFromResult(result),
      }));
    } catch (error) {
      setValidationResult((items) => ({
        ...items,
        [providerId]: { state: "error", message: formatProviderValidationError(error) },
      }));
    }
  };

  return (
    <SettingsGrid extraClassName="zorai-settings-grid--full">
      <Panel section="Auth" title="Authentication" extraClassName="zorai-settings-auth">
        <RefreshButton label="Refresh auth status" onClick={() => void refreshAuth()} />
        {authStates.length === 0 ? <p className="zorai-empty-state">No provider auth status has been reported by the daemon yet.</p> : authStates.map((state) => {
          const providerValidation = validationResult[state.provider_id];
          const isTestingProvider = providerValidation?.state === "testing";

          const usesCursorSubscription = state.provider_id === "cursor";
          const usesLocalCliAuth = state.provider_id === "claude-code-cli" || usesCursorSubscription;

          return (
            <div key={`${state.provider_id}-${state.auth_source}`} className="zorai-setting-row">
              <div><strong>{state.authenticated ? "●" : "○"} {state.provider_name}</strong><span>{state.model ? `${state.model} / ` : ""}{state.auth_source}</span>{providerValidation ? <span>{providerValidation.message}</span> : null}</div>
              <div className="zorai-card-actions">
                {usesCursorSubscription && !state.authenticated ? (
                  <button type="button" className="zorai-ghost-button" onClick={() => void loginProvider(state.provider_id, "", state.base_url)}>Subscription</button>
                ) : null}
                {usesLocalCliAuth ? null : (
                  <button type="button" className="zorai-ghost-button" onClick={() => {
                    setLoginTarget(loginTarget === state.provider_id ? null : state.provider_id);
                    setLoginKey("");
                  }}>API Key</button>
                )}
                {state.authenticated ? <button type="button" className="zorai-ghost-button" onClick={() => void logoutProvider(state.provider_id)}>Logout</button> : null}
                <button type="button" className="zorai-ghost-button" disabled={isTestingProvider} onClick={() => void runTest(state.provider_id, state.base_url, state.auth_source)}>{getProviderValidationButtonLabel(validationResult, state.provider_id)}</button>
              </div>
              {!usesLocalCliAuth && loginTarget === state.provider_id ? (
                <div className="zorai-setting-row">
                  <div><strong>API Key</strong><span>Stored by the daemon provider auth store.</span></div>
                  <input className="zorai-input" type="password" value={loginKey} onChange={(event) => setLoginKey(event.target.value)} />
                  <button type="button" className="zorai-primary-button" disabled={!loginKey.trim()} onClick={() => void loginProvider(state.provider_id, loginKey, state.base_url).then(() => { setLoginKey(""); setLoginTarget(null); })}>Save</button>
                </div>
              ) : null}
            </div>
          );
        })}
      </Panel>
    </SettingsGrid>
  );
}

function SearchPanel() {
  const agentSettings = useAgentStore((state) => state.agentSettings);
  const updateAgentSetting = useAgentStore((state) => state.updateAgentSetting);

  return (
    <SettingsGrid>
      <Panel section="Search" title="Web Search">
        <SettingRow label="Enable Web Search" description="Mirrors the web search tool toggle."><Switch checked={agentSettings.enable_web_search_tool} onChange={(checked) => updateAgentSetting("enable_web_search_tool", checked)} /></SettingRow>
        <SettingRow label="Provider" description="Search provider used by agent web search.">
          <select className="zorai-input" value={agentSettings.search_provider} onChange={(event) => updateAgentSetting("search_provider", event.target.value as AgentSettings["search_provider"])}>
            {searchProviderOptions.map((value) => <option key={value} value={value}>{value}</option>)}
          </select>
        </SettingRow>
        <SettingRow label="DDG Region" description="DuckDuckGo region code such as us-en, wt-wt, or pl-pl.">
          <input className="zorai-input" value={agentSettings.duckduckgo_region} onChange={(event) => updateAgentSetting("duckduckgo_region", event.target.value)} />
        </SettingRow>
        <SettingRow label="DDG Safe Search" description="DuckDuckGo Safe Search setting.">
          <select className="zorai-input" value={agentSettings.duckduckgo_safe_search} onChange={(event) => updateAgentSetting("duckduckgo_safe_search", event.target.value as AgentSettings["duckduckgo_safe_search"])}>
            {duckDuckGoSafeSearchOptions.map((value) => <option key={value} value={value}>{value}</option>)}
          </select>
        </SettingRow>
        <SecretRow label="Firecrawl Key" value={agentSettings.firecrawl_api_key} onChange={(value) => updateAgentSetting("firecrawl_api_key", value)} />
        <SecretRow label="Exa Key" value={agentSettings.exa_api_key} onChange={(value) => updateAgentSetting("exa_api_key", value)} />
        <SecretRow label="Tavily Key" value={agentSettings.tavily_api_key} onChange={(value) => updateAgentSetting("tavily_api_key", value)} />
        <NumberRow label="Max Results" description="Maximum search results." value={agentSettings.search_max_results} onChange={(value) => updateAgentSetting("search_max_results", value)} min={1} max={50} />
        <NumberRow label="Timeout" description="Search timeout in seconds." value={agentSettings.search_timeout_secs} onChange={(value) => updateAgentSetting("search_timeout_secs", value)} min={1} max={120} />
        <SettingRow label="Browser" description="Browsing provider.">
          <select className="zorai-input" value={agentSettings.browse_provider} onChange={(event) => updateAgentSetting("browse_provider", event.target.value as AgentSettings["browse_provider"])}>
            {["auto", "lightpanda", "chrome", "none"].map((value) => <option key={value} value={value}>{value}</option>)}
          </select>
        </SettingRow>
      </Panel>
    </SettingsGrid>
  );
}

function ToolsPanel() {
  const agentSettings = useAgentStore((state) => state.agentSettings);
  const updateAgentSetting = useAgentStore((state) => state.updateAgentSetting);

  return (
    <SettingsGrid>
      <Panel section="Tools" title="Agent capabilities" extraClassName="zorai-settings-tools">
        {toolToggles.map((toggle) => (
          <SettingRow key={toggle.key} label={toggle.label} description={toggle.description}>
            <Switch checked={Boolean(agentSettings[toggle.key])} onChange={(checked) => updateAgentSetting(toggle.key, checked as never)} />
          </SettingRow>
        ))}
      </Panel>
    </SettingsGrid>
  );
}

function ConciergePanel() {
  const agentSettings = useAgentStore((state) => state.agentSettings);
  const config = useAgentStore((state) => state.conciergeConfig);
  const updateConfig = useAgentStore((state) => state.updateConciergeConfig);
  const providerAuthStates = useAgentStore((state) => state.providerAuthStates);
  const refreshProviderAuthStates = useAgentStore((state) => state.refreshProviderAuthStates);
  const providerIds = useProviderIds(agentSettings);
  const selectedConciergeProvider = config.provider ?? "";
  const providerNames = new Map(providerAuthStates.map((state) => [state.provider_id, state.provider_name]));
  const selectableProviders = Array.from(new Set([
    ...providerIds,
    ...providerAuthStates.map((state) => state.provider_id),
    ...(selectedConciergeProvider ? [selectedConciergeProvider] : []),
  ])).filter(Boolean).sort().map((providerId) => ({
    provider_id: providerId,
    provider_name: providerNames.get(providerId) ?? providerId,
  }));
  const detailLevels = Array.from(new Set([config.detail_level, "proactive_triage", "brief", "standard", "detailed"].filter(Boolean)));
  const patchConfig = (patch: Record<string, unknown>) => void updateConfig({ ...config, ...patch });

  useEffect(() => {
    void refreshProviderAuthStates();
  }, [refreshProviderAuthStates]);

  return (
    <SettingsGrid>
      <Panel section="Rarog" title="Welcome agent and operational assistant">
        <SettingRow label="Enabled" description="Allow the concierge to brief and guide operator sessions.">
          <Switch checked={config.enabled} onChange={(checked) => patchConfig({ enabled: checked })} />
        </SettingRow>
        <SettingRow label="Detail level" description="Default depth for concierge guidance.">
          <select className="zorai-input" value={config.detail_level} onChange={(event) => patchConfig({ detail_level: event.target.value })}>
            {detailLevels.map((value) => <option key={value} value={value}>{value}</option>)}
          </select>
        </SettingRow>
        <SettingRow label="Provider" description="Rarog provider, or inherit from Svarog.">
          <select className="zorai-input" value={selectedConciergeProvider} onChange={(event) => patchConfig({ provider: event.target.value || undefined, model: event.target.value ? config.model : undefined })}>
            <option value="">(use Svarog)</option>
            {selectableProviders.map((provider) => <option key={provider.provider_id} value={provider.provider_id}>{provider.provider_name}</option>)}
          </select>
        </SettingRow>
        <SettingRow label="Model" description="Rarog model, or inherit from Svarog.">
          {selectedConciergeProvider ? (
            <ModelSelector providerId={selectedConciergeProvider as AgentProviderId} value={config.model ?? ""} customName={config.model ?? ""} onChange={(model) => patchConfig({ model: model || undefined })} allowProviderAuthFetch={Boolean(providerAuthStates.find((entry) => entry.provider_id === selectedConciergeProvider)?.authenticated)} />
          ) : <span className="zorai-empty-state">(use Svarog)</span>}
        </SettingRow>
        <SettingRow label="Reasoning" description="Rarog reasoning effort.">
          <select className="zorai-input" value={normalizeConciergeReasoningEffortForUi(config.reasoning_effort)} onChange={(event) => patchConfig({ reasoning_effort: serializeConciergeReasoningEffortFromUi(event.target.value as AgentSettings["reasoning_effort"]) })}>
            {conciergeReasoningEffortOptions.map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}
          </select>
        </SettingRow>
        <SettingRow label="Context" description="Rarog context window in tokens.">
          <RarogContextField />
        </SettingRow>
      </Panel>
    </SettingsGrid>
  );
}

function SubAgentsPanel() {
  return <SubAgentsTab />;
}

function RarogContextField() {
  const threads = useAgentStore((state) => state.threads);
  const agentSettings = useAgentStore((state) => state.agentSettings);
  const initial = threads.find((thread) => {
    const agentId = canonicalThreadAgentId(thread.agent_name);
    return agentId === "rarog" || agentId === "concierge" || thread.daemonThreadId === "concierge";
  })?.profileContextWindowTokens ?? agentSettings.context_window_tokens ?? 128000;
  const [value, setValue] = useState(initial);
  return (
    <input
      className="zorai-input"
      type="number"
      min={1000}
      max={2000000}
      value={value}
      onChange={(event) => setValue(Number(event.target.value))}
      onBlur={() => void applyRarogContextWindow(value)}
    />
  );
}

function GatewayPanel() {
  const agentSettings = useAgentStore((state) => state.agentSettings);
  const updateAgentSetting = useAgentStore((state) => state.updateAgentSetting);
  const [whatsappStatus, setWhatsappStatus] = useState("");
  const connectWhatsApp = async () => {
    const result = await getBridge()?.whatsappConnect?.();
    setWhatsappStatus(result?.ok === false ? result.error ?? "WhatsApp link failed." : "WhatsApp link requested.");
  };

  return (
    <SettingsGrid>
      <Panel section="Gateway" title="Messaging platform connections">
        <SettingRow label="Gateway enabled" description="Bridge external chat platforms into Zorai.">
          <Switch checked={agentSettings.gateway_enabled} onChange={(checked) => updateAgentSetting("gateway_enabled", checked)} />
        </SettingRow>
        <SettingRow label="Command Prefix" description="Prefix used for external platform commands.">
          <input className="zorai-input" value={agentSettings.gateway_command_prefix} onChange={(event) => updateAgentSetting("gateway_command_prefix", event.target.value)} />
        </SettingRow>
      </Panel>
      <Panel section="Gateway" title="Slack">
        <SecretRow label="Bot Token" value={agentSettings.slack_token} onChange={(value) => updateAgentSetting("slack_token", value)} />
        <SettingRow label="Channel Filter" description="Allowed Slack channels."><input className="zorai-input" value={agentSettings.slack_channel_filter} onChange={(event) => updateAgentSetting("slack_channel_filter", event.target.value)} /></SettingRow>
      </Panel>
      <Panel section="Gateway" title="Telegram">
        <SecretRow label="Bot Token" value={agentSettings.telegram_token} onChange={(value) => updateAgentSetting("telegram_token", value)} />
        <SettingRow label="Allowed Chats" description="Comma-separated Telegram chat ids."><input className="zorai-input" value={agentSettings.telegram_allowed_chats} onChange={(event) => updateAgentSetting("telegram_allowed_chats", event.target.value)} /></SettingRow>
      </Panel>
      <Panel section="Gateway" title="Discord">
        <SecretRow label="Bot Token" value={agentSettings.discord_token} onChange={(value) => updateAgentSetting("discord_token", value)} />
        <SettingRow label="Channel Filter" description="Allowed Discord channels."><input className="zorai-input" value={agentSettings.discord_channel_filter} onChange={(event) => updateAgentSetting("discord_channel_filter", event.target.value)} /></SettingRow>
        <SettingRow label="Allowed Users" description="Allowed Discord users."><input className="zorai-input" value={agentSettings.discord_allowed_users} onChange={(event) => updateAgentSetting("discord_allowed_users", event.target.value)} /></SettingRow>
      </Panel>
      <Panel section="Gateway" title="WhatsApp">
        <SettingRow label="Allowed Contacts" description="Comma or newline separated phone numbers."><textarea className="zorai-input" value={agentSettings.whatsapp_allowed_contacts} onChange={(event) => updateAgentSetting("whatsapp_allowed_contacts", event.target.value)} /></SettingRow>
        <SecretRow label="API Token" value={agentSettings.whatsapp_token} onChange={(value) => updateAgentSetting("whatsapp_token", value)} />
        <SettingRow label="Phone Number ID" description="WhatsApp Cloud API phone number id."><input className="zorai-input" value={agentSettings.whatsapp_phone_id} onChange={(event) => updateAgentSetting("whatsapp_phone_id", event.target.value)} /></SettingRow>
        <button type="button" className="zorai-ghost-button" onClick={() => void connectWhatsApp()}>Link Device</button>
        <button type="button" className="zorai-ghost-button" onClick={() => void connectWhatsApp()}>Re-link Device</button>
        {whatsappStatus ? <p className="zorai-empty-state">{whatsappStatus}</p> : null}
      </Panel>
    </SettingsGrid>
  );
}

function FeaturesPanel() {
  const agentSettings = useAgentStore((state) => state.agentSettings);
  const updateAgentSetting = useAgentStore((state) => state.updateAgentSetting);
  const providerAuthStates = useAgentStore((state) => state.providerAuthStates);
  const subAgents = useAgentStore((state) => state.subAgents);
  const providerIds = useProviderIds(agentSettings);
  const providerOptions = useProviderOptions(agentSettings);
  const audioSttProviderOptions = filterAudioProviderOptions(providerOptions, "stt");
  const audioTtsProviderOptions = filterAudioProviderOptions(providerOptions, "tts");
  const imageGenerationProviderOptions = filterImageGenerationProviderOptions(providerOptions);
  const embeddingProviderOptions = filterEmbeddingProviderOptions(providerOptions);
  const audioSttProviderConfig = getProviderConfig(agentSettings, agentSettings.audio_stt_provider);
  const audioTtsProviderConfig = getProviderConfig(agentSettings, agentSettings.audio_tts_provider);
  const imageGenerationProviderConfig = getProviderConfig(agentSettings, agentSettings.image_generation_provider);
  const embeddingProviderConfig = getProviderConfig(agentSettings, agentSettings.semantic_embedding_provider);
  const providerHasDaemonAuth = (providerId: AgentProviderId) => providerAuthStates.some((state) => state.provider_id === providerId && state.authenticated);

  return (
    <SettingsGrid>
      <Panel section="Features" title="Audio and media">
        <SettingRow label="STT Enabled" description="Speech-to-text feature toggle."><Switch checked={agentSettings.audio_stt_enabled} onChange={(checked) => updateAgentSetting("audio_stt_enabled", checked)} /></SettingRow>
        <SettingRow label="STT Provider" description="Speech-to-text provider.">
          <ProviderSelect
            value={agentSettings.audio_stt_provider}
            options={audioSttProviderOptions}
            fallbackOptions={providerIds}
            onChange={(providerId) => {
              updateAgentSetting("audio_stt_provider", providerId);
              updateAgentSetting("audio_stt_model", normalizeAudioModelForProviderChange(providerId, "stt", agentSettings.audio_stt_model));
            }}
          />
        </SettingRow>
        <SettingRow label="STT Model" description="Speech-to-text model.">
          <ModelSelector
            providerId={agentSettings.audio_stt_provider}
            value={agentSettings.audio_stt_model}
            customName={audioSttProviderConfig.custom_model_name}
            onChange={(value) => updateAgentSetting("audio_stt_model", value)}
            base_url={audioSttProviderConfig.base_url}
            api_key={audioSttProviderConfig.api_key}
            auth_source={audioSttProviderConfig.auth_source}
            allowProviderAuthFetch={providerHasDaemonAuth(agentSettings.audio_stt_provider)}
            modelOptions={audioModelOptions(agentSettings.audio_stt_provider, "stt")}
            remoteModelFilter={(model) => filterFetchedModelsForAudio([model], "stt").length > 0}
            fetchOutputModalities={audioRemoteModelFetchOutputModalities("stt", agentSettings.audio_stt_provider)}
            disabled={!agentSettings.audio_stt_enabled}
          />
        </SettingRow>
        <SettingRow label="STT Language" description="Speech-to-text language override."><input className="zorai-input" value={agentSettings.audio_stt_language} placeholder="auto" disabled={!agentSettings.audio_stt_enabled} onChange={(event) => updateAgentSetting("audio_stt_language", event.target.value)} /></SettingRow>
        <SettingRow label="TTS Enabled" description="Text-to-speech feature toggle."><Switch checked={agentSettings.audio_tts_enabled} onChange={(checked) => updateAgentSetting("audio_tts_enabled", checked)} /></SettingRow>
        <SettingRow label="TTS Provider" description="Text-to-speech provider.">
          <ProviderSelect
            value={agentSettings.audio_tts_provider}
            options={audioTtsProviderOptions}
            fallbackOptions={providerIds}
            onChange={(providerId) => {
              updateAgentSetting("audio_tts_provider", providerId);
              updateAgentSetting("audio_tts_model", normalizeAudioModelForProviderChange(providerId, "tts", agentSettings.audio_tts_model));
            }}
          />
        </SettingRow>
        <SettingRow label="TTS Model" description="Text-to-speech model.">
          <ModelSelector
            providerId={agentSettings.audio_tts_provider}
            value={agentSettings.audio_tts_model}
            customName={audioTtsProviderConfig.custom_model_name}
            onChange={(value) => updateAgentSetting("audio_tts_model", value)}
            base_url={audioTtsProviderConfig.base_url}
            api_key={audioTtsProviderConfig.api_key}
            auth_source={audioTtsProviderConfig.auth_source}
            allowProviderAuthFetch={providerHasDaemonAuth(agentSettings.audio_tts_provider)}
            modelOptions={audioModelOptions(agentSettings.audio_tts_provider, "tts")}
            remoteModelFilter={(model) => filterFetchedModelsForAudio([model], "tts").length > 0}
            fetchOutputModalities={audioRemoteModelFetchOutputModalities("tts", agentSettings.audio_tts_provider)}
            disabled={!agentSettings.audio_tts_enabled}
          />
        </SettingRow>
        <SettingRow label="TTS Voice" description="Text-to-speech voice."><input className="zorai-input" value={agentSettings.audio_tts_voice} disabled={!agentSettings.audio_tts_enabled} onChange={(event) => updateAgentSetting("audio_tts_voice", event.target.value)} /></SettingRow>
        <SettingRow label="TTS Auto Speak" description="Speak assistant replies automatically."><Switch checked={agentSettings.audio_tts_auto_speak} onChange={(checked) => updateAgentSetting("audio_tts_auto_speak", checked)} /></SettingRow>
        <SettingRow label="Image Provider" description="Image generation provider.">
          <ProviderSelect
            value={agentSettings.image_generation_provider}
            options={imageGenerationProviderOptions}
            fallbackOptions={providerIds}
            onChange={(providerId) => {
              updateAgentSetting("image_generation_provider", providerId);
              updateAgentSetting("image_generation_model", normalizeImageGenerationModelForProviderChange(providerId, agentSettings.image_generation_model));
            }}
          />
        </SettingRow>
        <SettingRow label="Image Model" description="Image generation model.">
          <ModelSelector
            providerId={agentSettings.image_generation_provider}
            value={agentSettings.image_generation_model}
            customName={imageGenerationProviderConfig.custom_model_name}
            onChange={(value) => updateAgentSetting("image_generation_model", value)}
            base_url={imageGenerationProviderConfig.base_url}
            api_key={imageGenerationProviderConfig.api_key}
            auth_source={imageGenerationProviderConfig.auth_source}
            allowProviderAuthFetch={providerHasDaemonAuth(agentSettings.image_generation_provider)}
            modelOptions={imageGenerationModelOptions(agentSettings.image_generation_provider)}
            remoteModelFilter={(model) => filterFetchedModelsForImageGeneration([model]).length > 0}
            fetchOutputModalities={imageRemoteModelFetchOutputModalities(agentSettings.image_generation_provider)}
          />
        </SettingRow>
      </Panel>
      <Panel section="Features" title="Semantic embeddings">
        <SettingRow label="Embeddings Enabled" description="Enable semantic embedding generation."><Switch checked={agentSettings.semantic_embedding_enabled} onChange={(checked) => updateAgentSetting("semantic_embedding_enabled", checked)} /></SettingRow>
        <SettingRow label="Embedding Provider" description="Provider used for semantic embeddings.">
          <ProviderSelect
            value={agentSettings.semantic_embedding_provider}
            options={embeddingProviderOptions}
            fallbackOptions={providerIds}
            onChange={(providerId) => {
              updateAgentSetting("semantic_embedding_provider", providerId);
              updateAgentSetting("semantic_embedding_model", normalizeEmbeddingModelForProviderChange(providerId, agentSettings.semantic_embedding_model));
            }}
          />
        </SettingRow>
        <SettingRow label="Embedding Model" description="Model used for semantic embeddings.">
          <ModelSelector
            providerId={agentSettings.semantic_embedding_provider}
            value={agentSettings.semantic_embedding_model}
            customName={embeddingProviderConfig.custom_model_name}
            onChange={(value, _name, details) => {
              const patch = embeddingSettingsPatchForModelSelection(value, details);
              updateAgentSetting("semantic_embedding_model", patch.semantic_embedding_model);
              if (patch.semantic_embedding_dimensions != null) {
                updateAgentSetting("semantic_embedding_dimensions", patch.semantic_embedding_dimensions);
              }
            }}
            base_url={embeddingProviderConfig.base_url}
            api_key={embeddingProviderConfig.api_key}
            auth_source={embeddingProviderConfig.auth_source}
            allowProviderAuthFetch={providerHasDaemonAuth(agentSettings.semantic_embedding_provider)}
            modelOptions={embeddingModelOptions(agentSettings.semantic_embedding_provider)}
            remoteModelFilter={(model) => filterFetchedModelsForEmbeddings([model]).length > 0}
            fetchOutputModalities="embedding"
            disabled={!agentSettings.semantic_embedding_enabled}
          />
        </SettingRow>
        <NumberRow label="Dimensions" description="Embedding vector dimensions." value={agentSettings.semantic_embedding_dimensions} onChange={(value) => updateAgentSetting("semantic_embedding_dimensions", value)} min={1} max={32768} />
        <NumberRow label="Batch Size" description="Embedding request batch size." value={agentSettings.semantic_embedding_batch_size} onChange={(value) => updateAgentSetting("semantic_embedding_batch_size", value)} min={1} max={512} />
        <NumberRow label="Concurrency" description="Concurrent embedding requests." value={agentSettings.semantic_embedding_max_concurrency} onChange={(value) => updateAgentSetting("semantic_embedding_max_concurrency", value)} min={1} max={16} />
      </Panel>
      <Panel section="Features" title="Threads">
        <SettingRow label="Auto Title" description="Generate a thread title from the first operator message using Rarog, WELES, or leave titles unchanged.">
          <select className="zorai-input" value={agentSettings.auto_thread_title ?? "off"} onChange={(event) => updateAgentSetting("auto_thread_title", event.target.value as AgentSettings["auto_thread_title"])}>
            <option value="off">Off</option>
            <option value="rarog">Rarog</option>
            <option value="weles">Weles</option>
          </select>
        </SettingRow>
      </Panel>
      <Panel section="Features" title="Code review">
        <SettingRow label="Review Agent" description="Agent or subagent that runs code review from the GUI Code tab.">
          <select className="zorai-input" value={agentSettings.code_review_agent} onChange={(event) => updateAgentSetting("code_review_agent", event.target.value)}>
            {codeReviewAgentOptions.map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}
            {subAgents.map((subAgent) => <option key={subAgent.id} value={subAgent.id}>{subAgent.name}</option>)}
          </select>
        </SettingRow>
        <NumberRow label="Commits per review" description="How many latest commits the reviewer examines." value={agentSettings.code_review_commit_window} onChange={(value) => updateAgentSetting("code_review_commit_window", value)} min={1} max={50} />
      </Panel>
    </SettingsGrid>
  );
}

const codeReviewAgentOptions = [
  { value: "weles", label: "Weles (built-in reviewer)" },
  { value: "swarog", label: "Svarog" },
  { value: "rarog", label: "Rarog" },
];

function AdvancedPanel() {
  const agentSettings = useAgentStore((state) => state.agentSettings);
  const updateAgentSetting = useAgentStore((state) => state.updateAgentSetting);
  const providerAuthStates = useAgentStore((state) => state.providerAuthStates);
  const compaction = agentSettings.compaction;
  const providerIds = useProviderIds(agentSettings);
  const providerOptions = useProviderOptions(agentSettings);
  const welesProviderConfig = getProviderConfig(agentSettings, compaction.weles.provider);
  const customCompactionProvider = compaction.custom_model.provider;
  const customCompactionProviderConfig = getProviderConfig(agentSettings, customCompactionProvider);
  const customCompactionTransports = getSupportedApiTransports(customCompactionProvider);
  const customCompactionTransport = normalizeApiTransport(customCompactionProvider, compaction.custom_model.api_transport);
  const updateLooseAgentSetting = (key: keyof AgentSettings, value: unknown) => updateAgentSetting(key, value as never);
  const updateCompaction = (patch: Partial<AgentSettings["compaction"]>) => updateAgentSetting("compaction", { ...compaction, ...patch });
  const updateCompactionWeles = (patch: Partial<AgentSettings["compaction"]["weles"]>) => updateCompaction({ weles: { ...compaction.weles, ...patch } });
  const updateCompactionCustom = (patch: Partial<AgentSettings["compaction"]["custom_model"]>) => updateCompaction({ custom_model: { ...compaction.custom_model, ...patch } });
  const providerHasDaemonAuth = (providerId: AgentProviderId) => providerAuthStates.some((state) => state.provider_id === providerId && state.authenticated);
  const applyCompactionWelesProvider = (providerId: AgentProviderId) => updateCompactionWeles({
    provider: providerId,
    model: getDefaultModelForProvider(providerId, getProviderConfig(agentSettings, providerId).auth_source) || compaction.weles.model,
  });
  const applyCompactionCustomProvider = (providerId: AgentProviderId) => {
    const providerConfig = getProviderConfig(agentSettings, providerId);
    updateCompactionCustom({
      provider: providerId,
      base_url: providerConfig.base_url,
      model: getDefaultModelForProvider(providerId, providerConfig.auth_source) || providerConfig.model || compaction.custom_model.model,
      api_key: providerConfig.api_key,
      auth_source: providerConfig.auth_source,
      api_transport: normalizeApiTransport(providerId, providerConfig.api_transport),
      context_window_tokens: providerConfig.context_window_tokens ?? 128000,
    });
  };

  return (
    <SettingsGrid>
      <Panel section="Advanced" title="Context compaction, safety, and retry settings">
        <SettingRow label="Sandbox Managed Cmds" description="Use daemon-managed command execution defaults."><Switch checked={Boolean(agentSettings.managed_sandbox_enabled)} onChange={(checked) => updateLooseAgentSetting("managed_sandbox_enabled", checked)} /></SettingRow>
        <SettingRow label="Managed Security" description="Default approval strictness for managed shell commands.">
          <select className="zorai-input" value={String(agentSettings.managed_security_level ?? "lowest")} onChange={(event) => updateLooseAgentSetting("managed_security_level", event.target.value)}>
            {["highest", "moderate", "lowest", "yolo"].map((value) => <option key={value} value={value}>{value}</option>)}
          </select>
        </SettingRow>
        <SettingRow label="Auto Compact Context" description="Compress older conversation context automatically."><Switch checked={agentSettings.auto_compact_context} onChange={(checked) => updateAgentSetting("auto_compact_context", checked)} /></SettingRow>
        <SettingRow label="Compaction Mode" description="Strategy used when active context needs compaction.">
          <select className="zorai-input" value={compaction.strategy} onChange={(event) => updateCompaction({ strategy: event.target.value as AgentSettings["compaction"]["strategy"] })}>
            <option value="heuristic">Heuristic</option>
            <option value="weles">WELES</option>
            <option value="custom_model">LLM provider</option>
          </select>
        </SettingRow>
        <NumberRow label="Heuristic Max Msgs" description="Conversation messages kept before compaction." value={agentSettings.max_context_messages} onChange={(value) => updateAgentSetting("max_context_messages", value)} min={10} max={500} />
        <NumberRow label="Max Tool Loops" description="Upper bound for tool-call cycles in one response." value={agentSettings.max_tool_loops} onChange={(value) => updateAgentSetting("max_tool_loops", value)} min={0} max={50} />
        <NumberRow label="Max Retries" description="Provider and tool retry attempts." value={agentSettings.max_retries} onChange={(value) => updateAgentSetting("max_retries", value)} min={0} max={10} />
        <NumberRow label="Auto Refresh (s)" description="Goal and workspace refresh interval; 0 disables polling." value={agentSettings.auto_refresh_interval_secs} onChange={(value) => updateAgentSetting("auto_refresh_interval_secs", value)} min={0} max={86400} />
        <NumberRow label="Retry Delay (ms)" description="Delay between retries." value={agentSettings.retry_delay_ms} onChange={(value) => updateAgentSetting("retry_delay_ms", value)} min={0} max={60000} />
        <NumberRow label="Message Loop (ms)" description="Delay between message loop iterations." value={agentSettings.message_loop_delay_ms} onChange={(value) => updateAgentSetting("message_loop_delay_ms", value)} min={0} max={60000} />
        <NumberRow label="Tool Call Gap (ms)" description="Delay between tool calls." value={agentSettings.tool_call_delay_ms} onChange={(value) => updateAgentSetting("tool_call_delay_ms", value)} min={0} max={60000} />
        <NumberRow label="LLM Stream Timeout (s)" description="Timeout while waiting for streamed model chunks." value={agentSettings.llm_stream_chunk_timeout_secs} onChange={(value) => updateAgentSetting("llm_stream_chunk_timeout_secs", value)} min={1} max={3600} />
        <SettingRow label="Auto Retry" description="Retry recoverable provider and tool failures."><Switch checked={agentSettings.auto_retry} onChange={(checked) => updateAgentSetting("auto_retry", checked)} /></SettingRow>
        <NumberRow label="Context Len Tok" description="Fallback context length when the provider model has no known value." value={agentSettings.context_window_tokens} onChange={(value) => updateAgentSetting("context_window_tokens", value)} min={1000} max={2000000} />
        <NumberRow label="Compact Thres %" description="Token-budget threshold that triggers compaction." value={agentSettings.compact_threshold_pct} onChange={(value) => updateAgentSetting("compact_threshold_pct", value)} min={1} max={100} />
        <NumberRow label="Keep Recent" description="Recent messages preserved across compaction." value={agentSettings.keep_recent_on_compact} onChange={(value) => updateAgentSetting("keep_recent_on_compact", value)} min={0} max={100} />
        <NumberRow label="Bash Timeout (s)" description="Managed shell command timeout." value={agentSettings.bash_timeout_seconds} onChange={(value) => updateAgentSetting("bash_timeout_seconds", value)} min={5} max={300} />
        <NumberRow label="WELES Reviews" description="Concurrent WELES review limit." value={agentSettings.weles_max_concurrent_reviews} onChange={(value) => updateAgentSetting("weles_max_concurrent_reviews", value)} min={0} max={16} />
      </Panel>
      <WorkspaceRepoMonitorPanel />
      <Panel section="Advanced" title="Compaction Strategy Settings">
        {compaction.strategy === "weles" ? (
          <>
            <SettingRow label="WELES Provider" description="Provider used by WELES compaction.">
              <ProviderSelect value={compaction.weles.provider} options={providerOptions} fallbackOptions={providerIds} onChange={applyCompactionWelesProvider} />
            </SettingRow>
            <SettingRow label="WELES Model" description="Model used by WELES compaction.">
              <ModelSelector
                providerId={compaction.weles.provider}
                value={compaction.weles.model}
                customName={welesProviderConfig.custom_model_name}
                onChange={(value) => updateCompactionWeles({ model: value })}
                base_url={welesProviderConfig.base_url}
                api_key={welesProviderConfig.api_key}
                auth_source={welesProviderConfig.auth_source}
                allowProviderAuthFetch={providerHasDaemonAuth(compaction.weles.provider)}
              />
            </SettingRow>
            <SettingRow label="WELES Reasoning" description="Reasoning effort for WELES compaction.">
              <select className="zorai-input" value={compaction.weles.reasoning_effort} onChange={(event) => updateCompactionWeles({ reasoning_effort: event.target.value as AgentSettings["reasoning_effort"] })}>
                {reasoningEfforts.map((value) => <option key={value} value={value}>{value}</option>)}
              </select>
            </SettingRow>
          </>
        ) : null}
        {compaction.strategy === "custom_model" ? (
          <>
            <SettingRow label="Custom Provider" description="Provider used by LLM provider compaction.">
              <ProviderSelect value={customCompactionProvider} options={providerOptions} fallbackOptions={providerIds} onChange={applyCompactionCustomProvider} />
            </SettingRow>
            <SettingRow label="Custom Base URL" description="Endpoint used by custom-model compaction."><input className="zorai-input" value={compaction.custom_model.base_url} onChange={(event) => updateCompactionCustom({ base_url: event.target.value })} /></SettingRow>
            <SettingRow label="Custom Auth" description="Credential source for the custom compaction model.">
              <select className="zorai-input" value={compaction.custom_model.auth_source} onChange={(event) => updateCompactionCustom({ auth_source: event.target.value as AuthSource })}>
                {authSources.map((source) => <option key={source} value={source}>{source}</option>)}
              </select>
            </SettingRow>
            <SettingRow label="Custom Model" description="Model used by LLM provider compaction.">
              <ModelSelector
                providerId={customCompactionProvider}
                value={compaction.custom_model.model}
                customName={customCompactionProviderConfig.custom_model_name}
                onChange={(value, _customName, details) => updateCompactionCustom({
                  model: value,
                  context_window_tokens: details?.predefinedModel?.contextWindow ?? details?.fetchedModel?.contextWindow ?? compaction.custom_model.context_window_tokens,
                })}
                base_url={compaction.custom_model.base_url}
                api_key={compaction.custom_model.api_key}
                auth_source={compaction.custom_model.auth_source}
                allowProviderAuthFetch={providerHasDaemonAuth(customCompactionProvider)}
              />
            </SettingRow>
            <SecretRow label="Custom API Key" value={compaction.custom_model.api_key} onChange={(value) => updateCompactionCustom({ api_key: value })} />
            <SettingRow label="Assistant ID" description="Optional native assistant identifier."><input className="zorai-input" value={compaction.custom_model.assistant_id} onChange={(event) => updateCompactionCustom({ assistant_id: event.target.value })} /></SettingRow>
            <SettingRow label="Custom Transport" description="Transport for custom-model compaction.">
              <select className="zorai-input" value={customCompactionTransport} onChange={(event) => updateCompactionCustom({ api_transport: normalizeApiTransport(customCompactionProvider, event.target.value) })}>
                {customCompactionTransports.map((transport) => <option key={transport} value={transport}>{formatTransportLabel(transport)}</option>)}
              </select>
            </SettingRow>
            <SettingRow label="Custom Reasoning" description="Reasoning effort for custom-model compaction.">
              <select className="zorai-input" value={compaction.custom_model.reasoning_effort} onChange={(event) => updateCompactionCustom({ reasoning_effort: event.target.value as AgentSettings["reasoning_effort"] })}>
                {reasoningEfforts.map((value) => <option key={value} value={value}>{value}</option>)}
              </select>
            </SettingRow>
            <NumberRow label="Custom Ctx Tok" description="Context window for custom-model compaction." value={compaction.custom_model.context_window_tokens} onChange={(value) => updateCompactionCustom({ context_window_tokens: value })} min={1000} max={2000000} />
          </>
        ) : null}
        {compaction.strategy === "heuristic" ? <p className="zorai-empty-state">Heuristic compaction uses the message limit and token threshold above.</p> : null}
      </Panel>
    </SettingsGrid>
  );
}

function WorkspaceRepoMonitorPanel() {
  const [enabled, setEnabled] = useState(false);
  const [includeDirs, setIncludeDirs] = useState("");
  const [excludeDirs, setExcludeDirs] = useState("");
  const [status, setStatus] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    let cancelled = false;
    void getWorkspaceSettings(DEFAULT_WORKSPACE_ID).then((settings) => {
      if (cancelled || !settings) return;
      setEnabled(settings.repo_monitor_enabled);
      setIncludeDirs(settings.repo_monitor_include_dirs.join("\n"));
      setExcludeDirs(settings.repo_monitor_exclude_dirs.join("\n"));
    }).catch(() => {
      if (!cancelled) {
        setStatus("Workspace git monitor settings are unavailable until the daemon bridge responds.");
      }
    });
    return () => {
      cancelled = true;
    };
  }, []);

  const parseList = (value: string) => value
    .split(/\r?\n|,/)
    .map((entry) => entry.trim())
    .filter(Boolean);

  const save = async () => {
    setSaving(true);
    setStatus(null);
    try {
      const settings = await setWorkspaceRepoMonitor(DEFAULT_WORKSPACE_ID, {
        repo_monitor_enabled: enabled,
        repo_monitor_include_dirs: parseList(includeDirs),
        repo_monitor_exclude_dirs: parseList(excludeDirs),
      });
      if (!settings) {
        setStatus("Workspace git monitor settings were not saved.");
        return;
      }
      setEnabled(settings.repo_monitor_enabled);
      setIncludeDirs(settings.repo_monitor_include_dirs.join("\n"));
      setExcludeDirs(settings.repo_monitor_exclude_dirs.join("\n"));
      setStatus(settings.repo_monitor_enabled
        ? "Workspace git monitoring saved. Only included directories will be scanned."
        : "Workspace git monitoring saved. Monitoring stays off until includes are provided and enabled.");
    } catch (error) {
      setStatus(error instanceof Error ? error.message : "Failed to save workspace git monitor settings.");
    } finally {
      setSaving(false);
    }
  };

  return (
    <Panel section="Advanced" title="Workspace git monitoring">
      <SettingRow label="Enable Monitor" description="Default is off. Monitoring only runs when this is enabled and the include list is not empty.">
        <Switch checked={enabled} onChange={setEnabled} />
      </SettingRow>
      <SettingRow label="Include Dirs" description="Relative or absolute directories to monitor. Only these directories are scanned for repo changes.">
        <textarea className="zorai-input" value={includeDirs} onChange={(event) => setIncludeDirs(event.target.value)} placeholder="frontend/src\ncrates/zorai-daemon" />
      </SettingRow>
      <SettingRow label="Exclude Dirs" description="Directories to subtract from the include list.">
        <textarea className="zorai-input" value={excludeDirs} onChange={(event) => setExcludeDirs(event.target.value)} placeholder="target\nfrontend/dist" />
      </SettingRow>
      <button type="button" className="zorai-ghost-button" onClick={() => void save()} disabled={saving}>
        {saving ? "Saving..." : "Save workspace monitor"}
      </button>
      {status ? <p className="zorai-empty-state">{status}</p> : null}
    </Panel>
  );
}

function PluginsPanel() {
  const plugins = usePluginStore((state) => state.plugins);
  const loading = usePluginStore((state) => state.loading);
  const error = usePluginStore((state) => state.error);
  const fetchPlugins = usePluginStore((state) => state.fetchPlugins);
  const toggleEnabled = usePluginStore((state) => state.toggleEnabled);
  const selectedPlugin = usePluginStore((state) => state.selectedPlugin);
  const settingsSchema = usePluginStore((state) => state.settingsSchema);
  const settingsValues = usePluginStore((state) => state.settingsValues);
  const selectPlugin = usePluginStore((state) => state.selectPlugin);
  const updateSetting = usePluginStore((state) => state.updateSetting);
  const installPlugin = usePluginStore((state) => state.installPlugin);
  const uninstallPlugin = usePluginStore((state) => state.uninstallPlugin);
  const testConnection = usePluginStore((state) => state.testConnection);
  const startOAuth = usePluginStore((state) => state.startOAuth);
  const testResult = usePluginStore((state) => state.testResult);
  const actionMessage = usePluginStore((state) => state.actionMessage);
  const [newPluginDir, setNewPluginDir] = useState("");
  const [newPluginSource, setNewPluginSource] = useState("");
  const pluginUpdateSettings = updateSetting;

  useEffect(() => {
    if (plugins.length === 0 && !loading) void fetchPlugins();
  }, [fetchPlugins, loading, plugins.length]);

  if (selectedPlugin) {
    const plugin = plugins.find((entry) => entry.name === selectedPlugin);
    const valueFor = (key: string) => settingsValues.find((entry) => entry.key === key)?.value ?? "";
    return (
      <SettingsGrid>
        <Panel section="Plugins" title={selectedPlugin}>
          <button type="button" className="zorai-ghost-button" onClick={() => void selectPlugin(null)}>Back</button>
          {settingsSchema.length === 0 ? <p className="zorai-empty-state">No editable plugin settings.</p> : settingsSchema.map((field) => (
            <SettingRow key={field.key} label={`${field.label}${field.required ? " *" : ""}`} description={field.description ?? field.key}>
              <input className="zorai-input" type={field.secret ? "password" : "text"} defaultValue={valueFor(field.key)} onBlur={(event) => void pluginUpdateSettings(selectedPlugin, field.key, event.target.value, field.secret)} />
            </SettingRow>
          ))}
          {plugin?.has_api ? <button type="button" className="zorai-ghost-button" onClick={() => void testConnection(selectedPlugin)}>Test Connection</button> : null}
          {plugin?.has_auth ? <button type="button" className="zorai-ghost-button" onClick={() => void startOAuth(selectedPlugin)}>{plugin.auth_status === "not_configured" ? "Connect" : "Reconnect"}</button> : null}
          <button type="button" className="zorai-ghost-button" onClick={() => void uninstallPlugin(selectedPlugin)}>Remove plugin</button>
          {testResult ? <p className="zorai-empty-state">{testResult.success ? "OK" : "Failed"}: {testResult.message}</p> : null}
          {actionMessage ? <p className="zorai-empty-state">{actionMessage}</p> : null}
        </Panel>
      </SettingsGrid>
    );
  }

  return (
    <SettingsGrid>
      <Panel section="Plugins" title="Installed extensions">
        <RefreshButton busy={loading} label={loading ? "Refreshing..." : "Refresh plugins"} onClick={() => void fetchPlugins()} />
        <SettingRow label="Add plugin" description="Register an installed plugin directory with the daemon.">
          <div className="zorai-inline-fields">
            <input className="zorai-input" value={newPluginDir} onChange={(event) => setNewPluginDir(event.target.value)} placeholder="dir-name" />
            <input className="zorai-input" value={newPluginSource} onChange={(event) => setNewPluginSource(event.target.value)} placeholder="install source" />
            <button type="button" className="zorai-ghost-button" onClick={() => {
              void installPlugin(newPluginDir, newPluginSource).then(() => {
                setNewPluginDir("");
                setNewPluginSource("");
              });
            }}>Add</button>
          </div>
        </SettingRow>
        {error ? <p className="zorai-empty-state">{error}</p> : null}
        {actionMessage ? <p className="zorai-empty-state">{actionMessage}</p> : null}
        {plugins.length === 0 && !loading ? <p className="zorai-empty-state">No plugins are currently reported by the plugin daemon.</p> : plugins.map((plugin) => (
          <SettingRow key={plugin.name} label={plugin.name} description={`${plugin.version} / ${plugin.endpoint_count} endpoints / auth ${plugin.auth_status}`}>
            <Switch checked={plugin.enabled} onChange={(checked) => void toggleEnabled(plugin.name, checked)} />
            <button type="button" className="zorai-ghost-button" onClick={() => void selectPlugin(plugin.name)}>Edit</button>
            <button type="button" className="zorai-ghost-button" onClick={() => void uninstallPlugin(plugin.name)}>Remove</button>
          </SettingRow>
        ))}
      </Panel>
    </SettingsGrid>
  );
}

function AboutPanel() {
  const agentSettings = useAgentStore((state) => state.agentSettings);
  const settings = useSettingsStore((state) => state.settings);

  return (
    <SettingsGrid>
      <Panel section="About" title={`${ZORAI_APP_NAME} shell`}>
        <Metric label="Version" value={APP_VERSION} />
        <Metric label="Author" value={APP_AUTHOR} />
        <Metric label="GitHub" value={APP_GITHUB} />
        <Metric label="Homepage" value={APP_HOMEPAGE} />
        <Metric label="Active provider" value={agentSettings.active_provider} />
        <Metric label="Backend" value="daemon" />
        <Metric label="Theme" value={settings.themeName} />
        <Metric label="Chat page size" value={String(agentSettings.react_chat_history_page_size)} />
      </Panel>
    </SettingsGrid>
  );
}

function useProviderIds(agentSettings: AgentSettings): AgentProviderId[] {
  return useMemo(() => Object.keys(agentSettings).filter((key) => {
    const value = agentSettings[key];
    return value && typeof value === "object" && "model" in value && "base_url" in value;
  }).sort() as AgentProviderId[], [agentSettings]);
}

function useProviderOptions(agentSettings: AgentSettings): ProviderOption[] {
  const providerIds = useProviderIds(agentSettings);
  return useMemo(() => providerIds.map((providerId) => ({
    id: providerId,
    label: getProviderDefinition(providerId)?.name ?? providerId,
  })), [providerIds]);
}

function getProviderConfig(agentSettings: AgentSettings, providerId: AgentProviderId): AgentProviderConfig {
  const value = agentSettings[providerId] as Partial<AgentProviderConfig> | undefined;
  return {
    base_url: value?.base_url ?? getProviderDefinition(providerId)?.defaultBaseUrl ?? "",
    model: value?.model ?? getDefaultModelForProvider(providerId),
    custom_model_name: value?.custom_model_name ?? "",
    api_key: value?.api_key ?? "",
    assistant_id: value?.assistant_id ?? "",
    api_transport: normalizeApiTransport(providerId, value?.api_transport ?? getDefaultApiTransport(providerId)),
    auth_source: value?.auth_source ?? getDefaultAuthSource(providerId),
    context_window_tokens: value?.context_window_tokens ?? null,
  };
}

function ProviderSelect({
  value,
  options,
  fallbackOptions,
  onChange,
}: {
  value: AgentProviderId;
  options: ProviderOption[];
  fallbackOptions: AgentProviderId[];
  onChange: (value: AgentProviderId) => void;
}) {
  const optionIds = new Set(options.map((option) => option.id));
  const visibleOptions = optionIds.has(value)
    ? options
    : [{ id: value, label: getProviderDefinition(value)?.name ?? value }, ...options];

  if (visibleOptions.length === 0) {
    return (
      <select className="zorai-input" value={value} onChange={(event) => onChange(event.target.value as AgentProviderId)}>
        {fallbackOptions.map((providerId) => <option key={providerId} value={providerId}>{getProviderDefinition(providerId)?.name ?? providerId}</option>)}
      </select>
    );
  }

  return (
    <select className="zorai-input" value={value} onChange={(event) => onChange(event.target.value as AgentProviderId)}>
      {visibleOptions.map((provider) => <option key={provider.id} value={provider.id}>{provider.label}</option>)}
    </select>
  );
}

function SettingsGrid({ children, extraClassName }: { children: ReactNode; extraClassName?: string }) {
  return <div className={["zorai-settings-grid", extraClassName ?? ""].filter(Boolean).join(" ")}>{children}</div>;
}

function Panel({ section, title, children, extraClassName }: { section: string; title: string; children: ReactNode; extraClassName?: string }) {
  return (
    <div className={["zorai-panel", extraClassName ?? ""].filter(Boolean).join(" ")}>
      <div><div className="zorai-section-label">{section}</div><h2>{title}</h2></div>
      {children}
    </div>
  );
}

function SettingRow({ label, description, children }: { label: string; description: string; children: ReactNode }) {
  return (
    <div className="zorai-setting-row">
      <div><strong>{label}</strong><span>{description}</span></div>
      {children}
    </div>
  );
}

function SecretRow({ label, value, onChange }: { label: string; value: string; onChange: (value: string) => void }) {
  return (
    <SettingRow label={label} description="Stored as a local gateway credential.">
      <input className="zorai-input" type="password" value={value} onChange={(event) => onChange(event.target.value)} />
    </SettingRow>
  );
}

function NumberRow({ label, description, value, onChange, min, max }: { label: string; description: string; value: number; onChange: (value: number) => void; min: number; max: number }) {
  return (
    <SettingRow label={label} description={description}>
      <input className="zorai-input" type="number" min={min} max={max} value={value} onChange={(event) => onChange(Number(event.target.value))} />
    </SettingRow>
  );
}

function Metric({ label, value }: { label: string; value: string }) {
  return <div className="zorai-setting-row"><div><strong>{label}</strong><span>{value}</span></div></div>;
}

function Switch({ checked, onChange }: { checked: boolean; onChange: (checked: boolean) => void }) {
  return (
    <button type="button" className={["zorai-switch", checked ? "zorai-switch--on" : ""].filter(Boolean).join(" ")} aria-pressed={checked} onClick={() => onChange(!checked)}>
      <span />
    </button>
  );
}

function formatTransportLabel(transport: ApiTransportMode) {
  return transport.replace(/_/g, " ");
}
