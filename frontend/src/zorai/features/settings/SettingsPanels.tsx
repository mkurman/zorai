import { useEffect, useMemo, useState, type ReactNode } from "react";
import { RefreshButton } from "@/zorai/shell/RefreshButton";
import { OpenRouterProviderRoutingControls } from "@/components/settings-panel/OpenRouterProviderRoutingControls";
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
import {
  CAPABILITY_TIER_OVERRIDES,
  CLAUDE_PERMISSION_MODES,
  HEARTBEAT_INTERVAL_OPTIONS_MINS,
  normalizeCapabilityTierOverride,
  normalizeClaudePermissionMode,
  useAgentStore,
  type AgentHeartbeatChecksSettings,
  type AgentSettings,
  type CapabilityTierOverride,
  type ClaudePermissionMode,
} from "@/lib/agentStore";
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
import { useAgentDisplayName } from "@/lib/useAgentDisplayName";
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
const APP_VERSION = "1.0.22";
const APP_AUTHOR = "Mariusz Kurman";
const APP_GITHUB = "mkurman/zorai";
const APP_HOMEPAGE = "zorai.app";
const DEFAULT_WORKSPACE_ID = "main";

const toolToggles: ToggleSetting[] = [
  { key: "enable_bash_tool", label: "Terminal", description: "Run managed shell commands." },
  { key: "enable_file_operations_tool", label: "File operations", description: "Read, write, and edit files in the workspace." },
  { key: "enable_vision_tool", label: "Vision", description: "Inspect screenshots and images." },
  { key: "enable_web_browsing_tool", label: "Browser", description: "Open and read web pages." },
  { key: "enable_web_search_tool", label: "Web search", description: "Query the configured search provider." },
  { key: "enable_system_info_tool", label: "System info", description: "Read host resources such as CPU, memory, and disks." },
  { key: "enable_gateway_messaging_tool", label: "Gateway messaging", description: "Send messages to connected Slack, Discord, Telegram, and WhatsApp chats." },
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

export function filterProviderAuthStates<T extends { provider_id: string; provider_name: string }>(states: T[], query: string): T[] {
  const needle = query.trim().toLowerCase();
  if (!needle) return states;
  return states.filter((state) => state.provider_name.toLowerCase().includes(needle) || state.provider_id.toLowerCase().includes(needle));
}

const SETTINGS_TAB_PANELS: Record<ZoraiSettingsTabId, () => ReactNode> = {
  auth: AuthPanel,
  model: ModelPanel,
  concierge: ConciergePanel,
  subagents: SubAgentsPanel,
  tools: ToolsPanel,
  skills: SkillsPanel,
  search: SearchPanel,
  media: MediaPanel,
  memory: MemoryPanel,
  heartbeat: HeartbeatTab,
  proactive: ProactivityPanel,
  workflows: WorkflowsPanel,
  gateway: GatewayPanel,
  runtime: ChatPanel,
  compaction: CompactionPanel,
  execution: ExecutionPanel,
  safety: SafetyPanel,
  mlflow: MlflowPanel,
  storage: StoragePanel,
  plugins: PluginsPanel,
  interface: InterfacePanel,
  migration: MigrationTab,
  about: AboutPanel,
};

export function SettingsTabPanel({ activeTab }: { activeTab: ZoraiSettingsTabId }) {
  const TabPanel = SETTINGS_TAB_PANELS[activeTab];
  return <TabPanel />;
}

function ChatPanel() {
  const agentSettings = useAgentStore((state) => state.agentSettings);
  const updateAgentSetting = useAgentStore((state) => state.updateAgentSetting);

  return (
    <SettingsGrid>
      <Panel section="Chat" title="Responses">
        <SettingRow label="Streaming" description="Stream assistant output as it arrives."><Switch checked={agentSettings.enable_streaming} onChange={(checked) => updateAgentSetting("enable_streaming", checked)} /></SettingRow>
      </Panel>
      <Panel section="Chat" title="History and restore">
        <NumberRow label="Visible Msgs" description="Messages visible in the terminal chat history page." value={agentSettings.tui_chat_history_page_size} onChange={(value) => updateAgentSetting("tui_chat_history_page_size", value)} min={25} max={1000} />
        <NumberRow label="Restore Hours" description="Recent lazy participant threads and stalled-turn supervision scanned by the daemon on startup." value={agentSettings.participant_observer_restore_window_hours} onChange={(value) => updateAgentSetting("participant_observer_restore_window_hours", value)} min={0} max={24 * 30} />
        <NumberRow label="Auto Refresh (s)" description="Goal and workspace refresh interval; 0 disables polling." value={agentSettings.auto_refresh_interval_secs} onChange={(value) => updateAgentSetting("auto_refresh_interval_secs", value)} min={0} max={86400} />
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
              context_window_tokens: details?.predefinedModel?.contextWindow || details?.fetchedModel?.contextWindow || activeProviderConfig.context_window_tokens,
            })}
            base_url={String(activeProviderConfig.base_url ?? "")}
            api_key={String(activeProviderConfig.api_key ?? "")}
            auth_source={activeProviderConfig.auth_source}
            allowProviderAuthFetch={providerAuthenticated}
          />
        </SettingRow>
        <SettingRow label="Effort" description="Svarog reasoning effort.">
          <select className="zorai-input" value={agentSettings.reasoning_effort} onChange={(event) => updateAgentSetting("reasoning_effort", event.target.value as AgentSettings["reasoning_effort"])}>
            {reasoningEfforts.map((value) => <option key={value} value={value}>{value}</option>)}
          </select>
        </SettingRow>
        <NumberRow label="Ctx Length" description="Context length override in tokens. Only used for models without a known context window." value={Number(activeProviderConfig.context_window_tokens ?? 0)} onChange={(value) => updateProviderConfig({ context_window_tokens: value || null })} min={0} max={2000000} />
      </Panel>
      <Panel section="Svarog" title="Connection">
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
        {activeProvider === "claude-code-cli" ? (
          <ClaudePermissionSelect value={agentSettings.claude_permission_mode} onChange={(value) => updateAgentSetting("claude_permission_mode", value)} />
        ) : null}
        {activeProvider === "huggingface" ? (
          <SettingRow label="HF Route" description="Hugging Face inference provider to route through, e.g. together or fireworks-ai. Empty lets Hugging Face choose.">
            <input className="zorai-input" value={String(activeProviderConfig.huggingface_provider ?? "")} placeholder="auto" onChange={(event) => updateProviderConfig({ huggingface_provider: event.target.value })} />
          </SettingRow>
        ) : null}
      </Panel>
      {activeProvider === "openrouter" ? (
        <Panel section="Svarog" title="OpenRouter routing">
          <OpenRouterProviderRoutingControls
            config={{ ...activeProviderConfig, model: String(activeProviderConfig.model ?? "") }}
            baseUrl={String(activeProviderConfig.base_url ?? "")}
            onChange={(next) => updateProviderConfig(next)}
          />
        </Panel>
      ) : null}
      <Panel section="Svarog" title="Identity">
        <AgentAliasRow aliasKey="swarog" name="Svarog" />
        <SettingRow label="System Prompt" description="Primary agent identity prompt.">
          <textarea className="zorai-input" value={agentSettings.system_prompt} onChange={(event) => updateAgentSetting("system_prompt", event.target.value)} />
        </SettingRow>
        <Metric label="Backend" value="daemon" />
      </Panel>
    </SettingsGrid>
  );
}

function ClaudePermissionSelect({ value, inheritLabel, onChange }: { value: string; inheritLabel?: string; onChange: (value: ClaudePermissionMode) => void }) {
  return (
    <SettingRow label="Claude perms" description="Permission mode passed to the Claude Code CLI.">
      <select className="zorai-input" value={normalizeClaudePermissionMode(value)} onChange={(event) => onChange(normalizeClaudePermissionMode(event.target.value))}>
        {CLAUDE_PERMISSION_MODES.map((mode) => <option key={mode} value={mode}>{mode || (inheritLabel ?? "CLI default")}</option>)}
      </select>
    </SettingRow>
  );
}

function MigrationTab() {
  return (
    <SettingsGrid>
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
  const [query, setQuery] = useState("");
  const visibleAuthStates = useMemo(() => filterProviderAuthStates(authStates, query), [authStates, query]);

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
        {authStates.length > 0 ? <input className="zorai-input" type="search" value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Search providers" aria-label="Search providers" /> : null}
        {authStates.length > 0 && visibleAuthStates.length === 0 ? <p className="zorai-empty-state">No providers match "{query.trim()}".</p> : null}
        {authStates.length === 0 ? <p className="zorai-empty-state">No provider auth status has been reported by the daemon yet.</p> : (
          <p className="zorai-auth-summary">{authStates.filter((state) => state.authenticated).length} of {authStates.length} providers connected</p>
        )}
        {([["Connected", true], ["Not connected", false]] as const).map(([groupLabel, connected]) => {
          const groupStates = visibleAuthStates.filter((state) => state.authenticated === connected);
          if (groupStates.length === 0) return null;
          return (
            <div key={groupLabel} className="zorai-auth-group">
              <div className="zorai-auth-group__label">{groupLabel} · {groupStates.length}</div>
              {groupStates.map((state) => {
                const providerValidation = validationResult[state.provider_id];
                const isTestingProvider = providerValidation?.state === "testing";

                const usesCursorSubscription = state.provider_id === "cursor";
                const usesLocalCliAuth = state.provider_id === "claude-code-cli" || usesCursorSubscription;
                const editingKey = !usesLocalCliAuth && loginTarget === state.provider_id;
                const saveKey = () => {
                  if (!loginKey.trim()) return;
                  void loginProvider(state.provider_id, loginKey, state.base_url).then(() => { setLoginKey(""); setLoginTarget(null); });
                };

                return (
                  <div key={`${state.provider_id}-${state.auth_source}`} className={["zorai-auth-row", state.authenticated ? "zorai-auth-row--connected" : ""].filter(Boolean).join(" ")}>
                    <span className="zorai-auth-row__dot" aria-hidden="true" />
                    <div className="zorai-auth-row__main">
                      <strong>{state.provider_name}</strong>
                      <span>{state.model ? `${state.model} · ` : ""}{state.auth_source}</span>
                    </div>
                    {providerValidation && providerValidation.state !== "testing" ? (
                      <span className={`zorai-auth-row__result zorai-auth-row__result--${providerValidation.state}`} title={providerValidation.message}>{providerValidation.state === "success" ? "Test passed" : providerValidation.message}</span>
                    ) : null}
                    <div className="zorai-auth-row__actions">
                      {usesCursorSubscription && !state.authenticated ? (
                        <button type="button" className="zorai-ghost-button" onClick={() => void loginProvider(state.provider_id, "", state.base_url)}>Subscription</button>
                      ) : null}
                      {usesLocalCliAuth ? null : (
                        <button type="button" className={["zorai-ghost-button", editingKey ? "zorai-button--active" : ""].filter(Boolean).join(" ")} onClick={() => {
                          setLoginTarget(editingKey ? null : state.provider_id);
                          setLoginKey("");
                        }}>{state.authenticated ? "Replace key" : "Add key"}</button>
                      )}
                      <button type="button" className="zorai-ghost-button" disabled={isTestingProvider} onClick={() => void runTest(state.provider_id, state.base_url, state.auth_source)}>{getProviderValidationButtonLabel(validationResult, state.provider_id)}</button>
                      {state.authenticated ? <button type="button" className="zorai-ghost-button zorai-auth-row__logout" onClick={() => void logoutProvider(state.provider_id)}>Logout</button> : null}
                    </div>
                    {editingKey ? (
                      <form className="zorai-auth-row__editor" onSubmit={(event) => { event.preventDefault(); saveKey(); }}>
                        <input className="zorai-input" type="password" autoFocus value={loginKey} onChange={(event) => setLoginKey(event.target.value)} placeholder={`${state.provider_name} API key`} aria-label={`${state.provider_name} API key`} />
                        <button type="submit" className="zorai-primary-button" disabled={!loginKey.trim()}>Save</button>
                        <button type="button" className="zorai-ghost-button" onClick={() => { setLoginKey(""); setLoginTarget(null); }}>Cancel</button>
                        <span>Stored by the daemon provider auth store.</span>
                      </form>
                    ) : null}
                  </div>
                );
              })}
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
  const provider = agentSettings.search_provider;

  return (
    <SettingsGrid>
      <Panel section="Web" title="Web Search">
        <SettingRow label="Enable Web Search" description="Mirrors the web search tool toggle."><Switch checked={agentSettings.enable_web_search_tool} onChange={(checked) => updateAgentSetting("enable_web_search_tool", checked)} /></SettingRow>
        <SettingRow label="Provider" description="Search provider used by agent web search.">
          <select className="zorai-input" value={provider} onChange={(event) => updateAgentSetting("search_provider", event.target.value as AgentSettings["search_provider"])}>
            {searchProviderOptions.map((value) => <option key={value} value={value}>{value}</option>)}
          </select>
        </SettingRow>
        {provider === "duckduckgo" ? (
          <>
            <SettingRow label="DDG Region" description="DuckDuckGo region code such as us-en, wt-wt, or pl-pl.">
              <input className="zorai-input" value={agentSettings.duckduckgo_region} onChange={(event) => updateAgentSetting("duckduckgo_region", event.target.value)} />
            </SettingRow>
            <SettingRow label="DDG Safe Search" description="DuckDuckGo Safe Search setting.">
              <select className="zorai-input" value={agentSettings.duckduckgo_safe_search} onChange={(event) => updateAgentSetting("duckduckgo_safe_search", event.target.value as AgentSettings["duckduckgo_safe_search"])}>
                {duckDuckGoSafeSearchOptions.map((value) => <option key={value} value={value}>{value}</option>)}
              </select>
            </SettingRow>
          </>
        ) : null}
        {provider === "firecrawl" ? <SecretRow label="Firecrawl Key" description="API key for Firecrawl search." value={agentSettings.firecrawl_api_key} onChange={(value) => updateAgentSetting("firecrawl_api_key", value)} /> : null}
        {provider === "exa" ? <SecretRow label="Exa Key" description="API key for Exa search." value={agentSettings.exa_api_key} onChange={(value) => updateAgentSetting("exa_api_key", value)} /> : null}
        {provider === "tavily" ? <SecretRow label="Tavily Key" description="API key for Tavily search." value={agentSettings.tavily_api_key} onChange={(value) => updateAgentSetting("tavily_api_key", value)} /> : null}
        <NumberRow label="Max Results" description="Maximum search results." value={agentSettings.search_max_results} onChange={(value) => updateAgentSetting("search_max_results", value)} min={1} max={50} />
        <NumberRow label="Timeout" description="Search timeout in seconds." value={agentSettings.search_timeout_secs} onChange={(value) => updateAgentSetting("search_timeout_secs", value)} min={1} max={120} />
      </Panel>
      <Panel section="Web" title="Browsing">
        <SettingRow label="Browser tool" description="Allow agents to open and read web pages."><Switch checked={agentSettings.enable_web_browsing_tool} onChange={(checked) => updateAgentSetting("enable_web_browsing_tool", checked)} /></SettingRow>
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
      <Panel section="Tools" title="Generated tools">
        <SettingRow label="Tool Synthesis" description="Allow generated tool workflows."><Switch checked={agentSettings.tool_synthesis_enabled} onChange={(checked) => updateAgentSetting("tool_synthesis_enabled", checked)} /></SettingRow>
        <SettingRow label="Require Activation" description="Require generated tools to be activated before use."><Switch checked={agentSettings.tool_synthesis_require_activation} onChange={(checked) => updateAgentSetting("tool_synthesis_require_activation", checked)} /></SettingRow>
        <NumberRow label="Tool Limit" description="Maximum generated tools." value={agentSettings.tool_synthesis_max_generated_tools} onChange={(value) => updateAgentSetting("tool_synthesis_max_generated_tools", value)} min={0} max={100} />
      </Panel>
    </SettingsGrid>
  );
}

function SkillsPanel() {
  const skills = useAgentStore((state) => state.agentSettings.skill_recommendation);
  const updateAgentSetting = useAgentStore((state) => state.updateAgentSetting);
  const updateSkills = (patch: Partial<AgentSettings["skill_recommendation"]>) => updateAgentSetting("skill_recommendation", { ...skills, ...patch });

  return (
    <SettingsGrid>
      <Panel section="Skills" title="Skill recommendation">
        <SettingRow label="Local Skill Gate" description="Recommend installed skills that match the task."><Switch checked={skills.enabled} onChange={(checked) => updateSkills({ enabled: checked })} /></SettingRow>
        <SettingRow label="Community Scout" description="Search the community skill registry in the background for install candidates."><Switch checked={skills.background_community_search} onChange={(checked) => updateSkills({ background_community_search: checked })} /></SettingRow>
        <NumberRow label="Scout Timeout (s)" description="How long a community skill install candidate waits for pre-approval." value={skills.community_preapprove_timeout_secs} onChange={(value) => updateSkills({ community_preapprove_timeout_secs: value })} min={5} max={300} />
        <NumberRow label="Suggest After" description="Approvals before suggesting the skill be enabled globally." value={skills.suggest_global_enable_after_approvals} onChange={(value) => updateSkills({ suggest_global_enable_after_approvals: value })} min={1} max={12} />
      </Panel>
    </SettingsGrid>
  );
}

function AgentAliasRow({ aliasKey, name }: { aliasKey: keyof AgentSettings["agent_aliases"]; name: string }) {
  const aliases = useAgentStore((state) => state.agentSettings.agent_aliases);
  const updateAgentSetting = useAgentStore((state) => state.updateAgentSetting);
  return (
    <SettingRow label="Alias" description={`Display name for ${name}. Agents still know it as ${name}. Leave empty to use ${name}.`}>
      <input
        className="zorai-input"
        value={aliases[aliasKey]}
        placeholder={name}
        maxLength={40}
        onChange={(event) => updateAgentSetting("agent_aliases", { ...aliases, [aliasKey]: event.target.value })}
      />
    </SettingRow>
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
  const effectiveProvider = (selectedConciergeProvider || agentSettings.active_provider) as AgentProviderId;
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
        <AgentAliasRow aliasKey="rarog" name="Rarog" />
        <SettingRow label="Enabled" description="Allow the concierge to brief and guide operator sessions.">
          <Switch checked={config.enabled} onChange={(checked) => patchConfig({ enabled: checked })} />
        </SettingRow>
        <SettingRow label="Detail level" description="Default depth for concierge guidance.">
          <select className="zorai-input" value={config.detail_level} onChange={(event) => patchConfig({ detail_level: event.target.value })}>
            {detailLevels.map((value) => <option key={value} value={value}>{value}</option>)}
          </select>
        </SettingRow>
      </Panel>
      <Panel section="Rarog" title="Model">
        <SettingRow label="Provider" description="Rarog provider, or inherit from Svarog.">
          <select className="zorai-input" value={selectedConciergeProvider} onChange={(event) => patchConfig({ provider: event.target.value || undefined, model: event.target.value ? config.model : undefined, api_transport: null })}>
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
      <Panel section="Rarog" title="Connection">
        <SettingRow label="Transport" description="API transport for Rarog. Inherit uses the provider default.">
          <select className="zorai-input" value={config.api_transport ?? ""} onChange={(event) => patchConfig({ api_transport: event.target.value ? normalizeApiTransport(effectiveProvider, event.target.value) : null })}>
            <option value="">Inherit</option>
            {getSupportedApiTransports(effectiveProvider).map((transport) => <option key={transport} value={transport}>{formatTransportLabel(transport)}</option>)}
          </select>
        </SettingRow>
        <SettingRow label="Base URL" description="Optional endpoint override for Rarog. Empty uses the provider default.">
          <input className="zorai-input" value={config.base_url ?? ""} placeholder="Provider default" onChange={(event) => patchConfig({ base_url: event.target.value.trim() ? event.target.value : null })} />
        </SettingRow>
        {effectiveProvider === "claude-code-cli" ? (
          <ClaudePermissionSelect value={config.claude_permission_mode ?? ""} inheritLabel="Inherit" onChange={(value) => patchConfig({ claude_permission_mode: value || null })} />
        ) : null}
        {effectiveProvider === "huggingface" ? (
          <SettingRow label="HF Route" description="Hugging Face inference provider for Rarog. Empty lets Hugging Face choose.">
            <input className="zorai-input" value={config.huggingface_provider ?? ""} placeholder="auto" onChange={(event) => patchConfig({ huggingface_provider: event.target.value.trim() ? event.target.value : null })} />
          </SettingRow>
        ) : null}
      </Panel>
      {selectedConciergeProvider === "openrouter" ? (
        <Panel section="Rarog" title="OpenRouter routing">
          <OpenRouterProviderRoutingControls
            config={{ ...config, model: config.model ?? "" }}
            baseUrl={config.base_url ?? ""}
            onChange={({ model: _model, ...routing }) => patchConfig(routing)}
          />
        </Panel>
      ) : null}
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
  })?.profileContextWindowTokens ?? agentSettings.context_window_tokens ?? 256000;
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
        <SettingRow label="Default agent" description="Agent that answers new gateway chats.">
          <GatewayAgentSelect value={agentSettings.gateway_default_agent} onChange={(value) => updateAgentSetting("gateway_default_agent", value)} />
        </SettingRow>
      </Panel>
      <Panel section="Gateway" title="Slack">
        <SettingRow label="Default agent" description="Agent that answers new chats on this platform.">
          <GatewayAgentSelect value={agentSettings.slack_default_agent} allowInherit onChange={(value) => updateAgentSetting("slack_default_agent", value)} />
        </SettingRow>
        <SecretRow label="Bot Token" value={agentSettings.slack_token} onChange={(value) => updateAgentSetting("slack_token", value)} />
        <SettingRow label="Channel Filter" description="Allowed Slack channels."><input className="zorai-input" value={agentSettings.slack_channel_filter} onChange={(event) => updateAgentSetting("slack_channel_filter", event.target.value)} /></SettingRow>
      </Panel>
      <Panel section="Gateway" title="Telegram">
        <SettingRow label="Default agent" description="Agent that answers new chats on this platform.">
          <GatewayAgentSelect value={agentSettings.telegram_default_agent} allowInherit onChange={(value) => updateAgentSetting("telegram_default_agent", value)} />
        </SettingRow>
        <SecretRow label="Bot Token" value={agentSettings.telegram_token} onChange={(value) => updateAgentSetting("telegram_token", value)} />
        <SettingRow label="Allowed Chats" description="Comma-separated Telegram chat ids."><input className="zorai-input" value={agentSettings.telegram_allowed_chats} onChange={(event) => updateAgentSetting("telegram_allowed_chats", event.target.value)} /></SettingRow>
      </Panel>
      <Panel section="Gateway" title="Discord">
        <SettingRow label="Default agent" description="Agent that answers new chats on this platform.">
          <GatewayAgentSelect value={agentSettings.discord_default_agent} allowInherit onChange={(value) => updateAgentSetting("discord_default_agent", value)} />
        </SettingRow>
        <SecretRow label="Bot Token" value={agentSettings.discord_token} onChange={(value) => updateAgentSetting("discord_token", value)} />
        <SettingRow label="Channel Filter" description="Allowed Discord channels."><input className="zorai-input" value={agentSettings.discord_channel_filter} onChange={(event) => updateAgentSetting("discord_channel_filter", event.target.value)} /></SettingRow>
        <SettingRow label="Allowed Users" description="Allowed Discord users."><input className="zorai-input" value={agentSettings.discord_allowed_users} onChange={(event) => updateAgentSetting("discord_allowed_users", event.target.value)} /></SettingRow>
      </Panel>
      <Panel section="Gateway" title="WhatsApp">
        <SettingRow label="Default agent" description="Agent that answers new chats on this platform.">
          <GatewayAgentSelect value={agentSettings.whatsapp_default_agent} allowInherit onChange={(value) => updateAgentSetting("whatsapp_default_agent", value)} />
        </SettingRow>
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

function GatewayAgentSelect({ value, allowInherit = false, onChange }: { value: string; allowInherit?: boolean; onChange: (value: string) => void }) {
  const subAgents = useAgentStore((state) => state.subAgents);
  const agentDisplayName = useAgentDisplayName();
  return (
    <select className="zorai-input" value={value} onChange={(event) => onChange(event.target.value)}>
      {allowInherit ? <option value="">Use gateway default</option> : null}
      <option value="rarog">{agentDisplayName("Rarog")}</option>
      <option value="swarog">{agentDisplayName("Svarog")}</option>
      <option value="weles">{agentDisplayName("Weles")}</option>
      {subAgents.filter((subAgent) => subAgent.enabled && !subAgent.builtin).map((subAgent) => <option key={subAgent.id} value={subAgent.id}>{subAgent.name}</option>)}
    </select>
  );
}

const HEARTBEAT_CHECK_ROWS: Array<{ key: keyof AgentHeartbeatChecksSettings; label: string; description: string }> = [
  { key: "stale_todos_enabled", label: "Stale Todos", description: "Flag todos that have not moved for a while." },
  { key: "stuck_goals_enabled", label: "Stuck Goals", description: "Flag goal runs that stopped making progress." },
  { key: "unreplied_messages_enabled", label: "Unreplied Messages", description: "Flag gateway messages still waiting for a reply." },
  { key: "repo_changes_enabled", label: "Repo Changes", description: "Report uncommitted changes in watched repositories." },
  { key: "plugin_auth_enabled", label: "Plugin Auth", description: "Warn about plugins whose authentication expired." },
];

function formatHeartbeatInterval(mins: number): string {
  if (mins % 1440 === 0) return mins === 1440 ? "Every day" : `Every ${mins / 1440} days`;
  if (mins % 60 === 0) return mins === 60 ? "Every hour" : `Every ${mins / 60} hours`;
  return `Every ${mins} minutes`;
}

function HeartbeatPanel() {
  const agentSettings = useAgentStore((state) => state.agentSettings);
  const updateAgentSetting = useAgentStore((state) => state.updateAgentSetting);
  const heartbeatOn = agentSettings.heartbeat_enabled;
  const usesCron = agentSettings.heartbeat_cron.trim() !== "";
  const intervalOptions: number[] = HEARTBEAT_INTERVAL_OPTIONS_MINS.includes(agentSettings.heartbeat_interval_mins as (typeof HEARTBEAT_INTERVAL_OPTIONS_MINS)[number])
    ? [...HEARTBEAT_INTERVAL_OPTIONS_MINS]
    : [...HEARTBEAT_INTERVAL_OPTIONS_MINS, agentSettings.heartbeat_interval_mins].sort((left, right) => left - right);
  const quietHourSelect = (value: number | null, key: "quiet_hours_start" | "quiet_hours_end") => (
    <select className="zorai-input" value={value ?? ""} disabled={!heartbeatOn} onChange={(event) => updateAgentSetting(key, event.target.value === "" ? null : Number(event.target.value))}>
      <option value="">Off</option>
      {Array.from({ length: 24 }, (_, hour) => <option key={hour} value={hour}>{`${String(hour).padStart(2, "0")}:00`}</option>)}
    </select>
  );

  return (
    <Panel section="Heartbeat" title="Schedule and checks">
      <SettingRow label="Heartbeat" description="Off by default. Periodically checks todos, goals, messages and repos, then makes one model call without tools to summarize anything that needs your attention.">
        <Switch checked={heartbeatOn} onChange={(checked) => updateAgentSetting("heartbeat_enabled", checked)} />
      </SettingRow>
      <SettingRow label="Interval" description="How often the heartbeat runs. Ignored when a cron schedule is set.">
        <select className="zorai-input" value={agentSettings.heartbeat_interval_mins} disabled={!heartbeatOn || usesCron} onChange={(event) => updateAgentSetting("heartbeat_interval_mins", Number(event.target.value))}>
          {intervalOptions.map((mins) => <option key={mins} value={mins}>{formatHeartbeatInterval(mins)}</option>)}
        </select>
      </SettingRow>
      <SettingRow label="Cron Schedule" description="Optional cron expression that replaces the interval, e.g. 0 9-18 * * 1-5. Leave empty to use the interval.">
        <input className="zorai-input" value={agentSettings.heartbeat_cron} placeholder="Use interval" disabled={!heartbeatOn} onChange={(event) => updateAgentSetting("heartbeat_cron", event.target.value)} />
      </SettingRow>
      <SettingRow label="Quiet Hours Start" description="Local hour when heartbeats pause. Needs an end hour too.">{quietHourSelect(agentSettings.quiet_hours_start, "quiet_hours_start")}</SettingRow>
      <SettingRow label="Quiet Hours End" description="Local hour when heartbeats resume.">{quietHourSelect(agentSettings.quiet_hours_end, "quiet_hours_end")}</SettingRow>
      {HEARTBEAT_CHECK_ROWS.map((row) => (
        <SettingRow key={row.key} label={row.label} description={row.description}>
          <Switch checked={agentSettings.heartbeat_checks[row.key]} onChange={(checked) => updateAgentSetting("heartbeat_checks", { ...agentSettings.heartbeat_checks, [row.key]: checked })} />
        </SettingRow>
      ))}
    </Panel>
  );
}

function HeartbeatTab() {
  const desktopNotifications = useAgentStore((state) => state.agentSettings.desktop_notifications);
  const updateAgentSetting = useAgentStore((state) => state.updateAgentSetting);

  return (
    <SettingsGrid>
      <HeartbeatPanel />
      <Panel section="Heartbeat" title="Notifications">
        <SettingRow label="Desktop Notifications" description="Show a system notification when another thread asks a question or needs an approval.">
          <Switch checked={desktopNotifications} onChange={(checked) => updateAgentSetting("desktop_notifications", checked)} />
        </SettingRow>
      </Panel>
    </SettingsGrid>
  );
}

function MediaPanel() {
  const agentSettings = useAgentStore((state) => state.agentSettings);
  const updateAgentSetting = useAgentStore((state) => state.updateAgentSetting);
  const providerAuthStates = useAgentStore((state) => state.providerAuthStates);
  const providerIds = useProviderIds(agentSettings);
  const providerOptions = useProviderOptions(agentSettings);
  const audioSttProviderOptions = filterAudioProviderOptions(providerOptions, "stt");
  const audioTtsProviderOptions = filterAudioProviderOptions(providerOptions, "tts");
  const imageGenerationProviderOptions = filterImageGenerationProviderOptions(providerOptions);
  const audioSttProviderConfig = getProviderConfig(agentSettings, agentSettings.audio_stt_provider);
  const audioTtsProviderConfig = getProviderConfig(agentSettings, agentSettings.audio_tts_provider);
  const imageGenerationProviderConfig = getProviderConfig(agentSettings, agentSettings.image_generation_provider);
  const providerHasDaemonAuth = (providerId: AgentProviderId) => providerAuthStates.some((state) => state.provider_id === providerId && state.authenticated);

  return (
    <SettingsGrid>
      <Panel section="Audio" title="Speech-to-text">
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
      </Panel>
      <Panel section="Audio" title="Text-to-speech">
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
      </Panel>
      <Panel section="Images" title="Image generation">
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
    </SettingsGrid>
  );
}

function MemoryPanel() {
  const agentSettings = useAgentStore((state) => state.agentSettings);
  const updateAgentSetting = useAgentStore((state) => state.updateAgentSetting);
  const providerAuthStates = useAgentStore((state) => state.providerAuthStates);
  const providerIds = useProviderIds(agentSettings);
  const providerOptions = useProviderOptions(agentSettings);
  const embeddingProviderOptions = filterEmbeddingProviderOptions(providerOptions);
  const embeddingProviderConfig = getProviderConfig(agentSettings, agentSettings.semantic_embedding_provider);
  const providerHasDaemonAuth = (providerId: AgentProviderId) => providerAuthStates.some((state) => state.provider_id === providerId && state.authenticated);

  return (
    <SettingsGrid>
      <Panel section="Memory" title="Honcho memory">
        <SettingRow label="Honcho Memory" description="Store and recall long-term memory through a Honcho server."><Switch checked={agentSettings.enable_honcho_memory} onChange={(checked) => updateAgentSetting("enable_honcho_memory", checked)} /></SettingRow>
        <SecretRow label="API Key" description="Honcho API key." value={agentSettings.honcho_api_key} onChange={(value) => updateAgentSetting("honcho_api_key", value)} />
        <SettingRow label="Base URL" description="Honcho server URL. Empty uses the hosted service."><input className="zorai-input" value={agentSettings.honcho_base_url} disabled={!agentSettings.enable_honcho_memory} onChange={(event) => updateAgentSetting("honcho_base_url", event.target.value)} /></SettingRow>
        <SettingRow label="Workspace" description="Honcho workspace id."><input className="zorai-input" value={agentSettings.honcho_workspace_id} placeholder="zorai" disabled={!agentSettings.enable_honcho_memory} onChange={(event) => updateAgentSetting("honcho_workspace_id", event.target.value)} /></SettingRow>
      </Panel>
      <Panel section="Memory" title="Consolidation">
        <SettingRow label="Consolidation" description="Consolidate memory while you are idle."><Switch checked={agentSettings.consolidation_enabled} onChange={(checked) => updateAgentSetting("consolidation_enabled", checked)} /></SettingRow>
        <NumberRow label="Decay Half-Life (h)" description="Half-life in hours for memory fact decay." value={agentSettings.consolidation_memory_decay_half_life_hours} onChange={(value) => updateAgentSetting("consolidation_memory_decay_half_life_hours", value)} min={1} max={10000} />
        <NumberRow label="Promotion Threshold" description="Successful repetitions before a heuristic is promoted." value={agentSettings.consolidation_heuristic_promotion_threshold} onChange={(value) => updateAgentSetting("consolidation_heuristic_promotion_threshold", value)} min={1} max={100} />
      </Panel>
      <Panel section="Memory" title="Semantic embeddings">
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
    </SettingsGrid>
  );
}

const CAPABILITY_TIER_LABELS: Record<CapabilityTierOverride, string> = {
  "": "Automatic",
  newcomer: "Newcomer",
  familiar: "Familiar",
  power_user: "Power user",
  expert: "Expert",
};

function ProactivityPanel() {
  const agentSettings = useAgentStore((state) => state.agentSettings);
  const updateAgentSetting = useAgentStore((state) => state.updateAgentSetting);

  return (
    <SettingsGrid>
      <Panel section="Proactivity" title="Anticipatory support">
        <SettingRow label="Anticipatory Support" description="Enable proactive operator support."><Switch checked={agentSettings.anticipatory_enabled} onChange={(checked) => updateAgentSetting("anticipatory_enabled", checked)} /></SettingRow>
        <SettingRow label="Morning Brief" description="Include morning brief generation."><Switch checked={agentSettings.anticipatory_morning_brief} onChange={(checked) => updateAgentSetting("anticipatory_morning_brief", checked)} /></SettingRow>
        <SettingRow label="Predictive Hydration" description="Hydrate likely-needed context ahead of time."><Switch checked={agentSettings.anticipatory_predictive_hydration} onChange={(checked) => updateAgentSetting("anticipatory_predictive_hydration", checked)} /></SettingRow>
        <SettingRow label="Stuck Detection" description="Detect stalled work and stale execution."><Switch checked={agentSettings.anticipatory_stuck_detection} onChange={(checked) => updateAgentSetting("anticipatory_stuck_detection", checked)} /></SettingRow>
      </Panel>
      <Panel section="Proactivity" title="Operator model">
        <SettingRow label="Operator Model" description="Learn operator preferences from local activity."><Switch checked={agentSettings.operator_model_enabled} onChange={(checked) => updateAgentSetting("operator_model_enabled", checked)} /></SettingRow>
        <SettingRow label="Message Statistics" description="Allow learning from message statistics."><Switch checked={agentSettings.operator_model_allow_message_statistics} onChange={(checked) => updateAgentSetting("operator_model_allow_message_statistics", checked)} /></SettingRow>
        <SettingRow label="Approval Learning" description="Allow learning from approval decisions."><Switch checked={agentSettings.operator_model_allow_approval_learning} onChange={(checked) => updateAgentSetting("operator_model_allow_approval_learning", checked)} /></SettingRow>
        <SettingRow label="Attention Tracking" description="Allow tracking which surfaces have your attention."><Switch checked={agentSettings.operator_model_allow_attention_tracking} onChange={(checked) => updateAgentSetting("operator_model_allow_attention_tracking", checked)} /></SettingRow>
        <SettingRow label="Implicit Feedback" description="Allow learning from implicit feedback such as revisions and fallbacks."><Switch checked={agentSettings.operator_model_allow_implicit_feedback} onChange={(checked) => updateAgentSetting("operator_model_allow_implicit_feedback", checked)} /></SettingRow>
      </Panel>
      <Panel section="Proactivity" title="Feature tier">
        <SettingRow label="Tier Override" description="Which feature tier to surface. Automatic lets the daemon pick from your usage.">
          <select className="zorai-input" value={agentSettings.tier_user_override} onChange={(event) => updateAgentSetting("tier_user_override", normalizeCapabilityTierOverride(event.target.value))}>
            {CAPABILITY_TIER_OVERRIDES.map((tier) => <option key={tier} value={tier}>{CAPABILITY_TIER_LABELS[tier]}</option>)}
          </select>
        </SettingRow>
      </Panel>
    </SettingsGrid>
  );
}

function WorkflowsPanel() {
  const agentSettings = useAgentStore((state) => state.agentSettings);
  const updateAgentSetting = useAgentStore((state) => state.updateAgentSetting);
  const subAgents = useAgentStore((state) => state.subAgents);

  return (
    <SettingsGrid>
      <Panel section="Workflows" title="Collaboration">
        <SettingRow label="Collaboration" description="Enable multi-agent collaboration features."><Switch checked={agentSettings.collaboration_enabled} onChange={(checked) => updateAgentSetting("collaboration_enabled", checked)} /></SettingRow>
      </Panel>
      <Panel section="Workflows" title="Threads">
        <SettingRow label="Auto Title" description="Generate a thread title from the first operator message using Rarog, WELES, or leave titles unchanged.">
          <select className="zorai-input" value={agentSettings.auto_thread_title ?? "off"} onChange={(event) => updateAgentSetting("auto_thread_title", event.target.value as AgentSettings["auto_thread_title"])}>
            <option value="off">Off</option>
            <option value="rarog">Rarog</option>
            <option value="weles">Weles</option>
          </select>
        </SettingRow>
      </Panel>
      <Panel section="Workflows" title="Code review">
        <SettingRow label="Review Agent" description="Runs Find Issues in the Code tab's Source Control view. Pick a subagent to review with its own provider and model; Svarog, Rarog and Weles lead threads and cannot be spawned as reviewers.">
          <select className="zorai-input" value={subAgents.some((subAgent) => subAgent.enabled && subAgent.id === agentSettings.code_review_agent) ? agentSettings.code_review_agent : "thread"} onChange={(event) => updateAgentSetting("code_review_agent", event.target.value)}>
            <option value="thread">This thread&apos;s agent</option>
            {subAgents.filter((subAgent) => subAgent.enabled).map((subAgent) => <option key={subAgent.id} value={subAgent.id}>{subAgent.name} · {subAgent.provider}/{subAgent.model}</option>)}
          </select>
        </SettingRow>
        <NumberRow label="Commits per review" description="How many latest commits the reviewer examines." value={agentSettings.code_review_commit_window} onChange={(value) => updateAgentSetting("code_review_commit_window", value)} min={1} max={50} />
      </Panel>
    </SettingsGrid>
  );
}

function CompactionPanel() {
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
      context_window_tokens: providerConfig.context_window_tokens ?? 256000,
    });
  };

  return (
    <SettingsGrid>
      <Panel section="Compaction" title="Context compaction">
        <SettingRow label="Auto Compact Context" description="Compress older conversation context automatically."><Switch checked={agentSettings.auto_compact_context} onChange={(checked) => updateAgentSetting("auto_compact_context", checked)} /></SettingRow>
        <SettingRow label="Compaction Mode" description="Strategy used when active context needs compaction.">
          <select className="zorai-input" value={compaction.strategy} onChange={(event) => updateCompaction({ strategy: event.target.value as AgentSettings["compaction"]["strategy"] })}>
            <option value="heuristic">Heuristic</option>
            <option value="weles">WELES</option>
            <option value="custom_model">LLM provider</option>
          </select>
        </SettingRow>
        <NumberRow label="Compact Thres %" description="Token-budget threshold that triggers compaction." value={agentSettings.compact_threshold_pct} onChange={(value) => updateAgentSetting("compact_threshold_pct", value)} min={1} max={100} />
        <NumberRow label="Keep Recent" description="Recent messages preserved across compaction." value={agentSettings.keep_recent_on_compact} onChange={(value) => updateAgentSetting("keep_recent_on_compact", value)} min={0} max={100} />
        <NumberRow label="Heuristic Max Msgs" description="Conversation messages kept before compaction." value={agentSettings.max_context_messages} onChange={(value) => updateAgentSetting("max_context_messages", value)} min={10} max={500} />
      </Panel>
      <Panel section="Compaction" title="Compaction Strategy Settings">
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
            <SettingRow label="WELES Transport" description="API transport for WELES compaction. Inherit uses the provider default.">
              <select className="zorai-input" value={compaction.weles.api_transport ?? ""} onChange={(event) => updateCompactionWeles({ api_transport: event.target.value ? normalizeApiTransport(compaction.weles.provider, event.target.value) : null })}>
                <option value="">Inherit</option>
                {getSupportedApiTransports(compaction.weles.provider).map((transport) => <option key={transport} value={transport}>{formatTransportLabel(transport)}</option>)}
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
                  context_window_tokens: details?.predefinedModel?.contextWindow || details?.fetchedModel?.contextWindow || compaction.custom_model.context_window_tokens,
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

function ExecutionPanel() {
  const agentSettings = useAgentStore((state) => state.agentSettings);
  const updateAgentSetting = useAgentStore((state) => state.updateAgentSetting);

  return (
    <SettingsGrid>
      <Panel section="Execution" title="Tool loop">
        <NumberRow label="Max Tool Loops" description="Upper bound for tool-call cycles in one response." value={agentSettings.max_tool_loops} onChange={(value) => updateAgentSetting("max_tool_loops", value)} min={0} max={50} />
        <NumberRow label="Subagent Budget Extensions" description="How many times a child's token budget can be extended. 0 = unlimited." value={agentSettings.max_subagent_budget_extensions} onChange={(value) => updateAgentSetting("max_subagent_budget_extensions", value)} min={0} max={1000} />
        <NumberRow label="Tool Call Gap (ms)" description="Delay between tool calls." value={agentSettings.tool_call_delay_ms} onChange={(value) => updateAgentSetting("tool_call_delay_ms", value)} min={0} max={60000} />
        <NumberRow label="Message Loop (ms)" description="Delay between message loop iterations." value={agentSettings.message_loop_delay_ms} onChange={(value) => updateAgentSetting("message_loop_delay_ms", value)} min={0} max={60000} />
      </Panel>
      <Panel section="Execution" title="Retries and timeouts">
        <SettingRow label="Auto Retry" description="Retry recoverable provider and tool failures."><Switch checked={agentSettings.auto_retry} onChange={(checked) => updateAgentSetting("auto_retry", checked)} /></SettingRow>
        <NumberRow label="Max Retries" description="Provider and tool retry attempts." value={agentSettings.max_retries} onChange={(value) => updateAgentSetting("max_retries", value)} min={0} max={10} />
        <NumberRow label="Retry Delay (ms)" description="Delay between retries." value={agentSettings.retry_delay_ms} onChange={(value) => updateAgentSetting("retry_delay_ms", value)} min={0} max={60000} />
        <NumberRow label="LLM Stream Timeout (s)" description="Timeout while waiting for streamed model chunks." value={agentSettings.llm_stream_chunk_timeout_secs} onChange={(value) => updateAgentSetting("llm_stream_chunk_timeout_secs", value)} min={1} max={3600} />
      </Panel>
      <Panel section="Execution" title="Reviews">
        <NumberRow label="WELES Reviews" description="Concurrent WELES review limit." value={agentSettings.weles_max_concurrent_reviews} onChange={(value) => updateAgentSetting("weles_max_concurrent_reviews", value)} min={0} max={16} />
      </Panel>
    </SettingsGrid>
  );
}

function SafetyPanel() {
  const agentSettings = useAgentStore((state) => state.agentSettings);
  const updateAgentSetting = useAgentStore((state) => state.updateAgentSetting);
  const updateLooseAgentSetting = (key: keyof AgentSettings, value: unknown) => updateAgentSetting(key, value as never);

  return (
    <SettingsGrid>
      <Panel section="Safety" title="Managed commands">
        <SettingRow label="Sandbox Managed Cmds" description="Use daemon-managed command execution defaults."><Switch checked={Boolean(agentSettings.managed_sandbox_enabled)} onChange={(checked) => updateLooseAgentSetting("managed_sandbox_enabled", checked)} /></SettingRow>
        <SettingRow label="Managed Security" description="Default approval strictness for managed shell commands.">
          <select className="zorai-input" value={String(agentSettings.managed_security_level ?? "lowest")} onChange={(event) => updateLooseAgentSetting("managed_security_level", event.target.value)}>
            {["highest", "moderate", "lowest", "yolo"].map((value) => <option key={value} value={value}>{value}</option>)}
          </select>
        </SettingRow>
      </Panel>
      <Panel section="Safety" title="Compliance">
        <SettingRow label="Compliance" description="Audit mode for retained events.">
          <select className="zorai-input" value={agentSettings.compliance_mode} onChange={(event) => updateAgentSetting("compliance_mode", event.target.value as AgentSettings["compliance_mode"])}>
            {["standard", "soc2", "hipaa", "fedramp"].map((value) => <option key={value} value={value}>{value}</option>)}
          </select>
        </SettingRow>
        <NumberRow label="Retention Days" description="Compliance event retention window." value={agentSettings.compliance_retention_days} onChange={(value) => updateAgentSetting("compliance_retention_days", value)} min={1} max={3650} />
        <SettingRow label="Sign All Events" description="Sign compliance audit events."><Switch checked={agentSettings.compliance_sign_all_events} onChange={(checked) => updateAgentSetting("compliance_sign_all_events", checked)} /></SettingRow>
      </Panel>
    </SettingsGrid>
  );
}

function StoragePanel() {
  const settings = useSettingsStore((state) => state.settings);
  const updateSetting = useSettingsStore((state) => state.updateSetting);

  return (
    <SettingsGrid>
      <Panel section="Storage" title="Snapshots">
        <SettingRow label="Auto Cleanup" description="Delete the oldest snapshots once a limit below is reached."><Switch checked={settings.snapshotAutoCleanup} onChange={(checked) => updateSetting("snapshotAutoCleanup", checked)} /></SettingRow>
        <NumberRow label="Max Snapshots" description="Snapshots to keep; 0 means no count limit." value={settings.snapshotMaxCount} onChange={(value) => updateSetting("snapshotMaxCount", value)} min={0} max={100} />
        <NumberRow label="Max Size (MB)" description="Total snapshot storage allowed." value={settings.snapshotMaxSizeMb} onChange={(value) => updateSetting("snapshotMaxSizeMb", value)} min={1024} max={500000} />
      </Panel>
      <WorkspaceRepoMonitorPanel />
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
    <Panel section="Storage" title="Workspace git monitoring">
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

function SecretRow({ label, description = "Stored as a local gateway credential.", value, onChange }: { label: string; description?: string; value: string; onChange: (value: string) => void }) {
  return (
    <SettingRow label={label} description={description}>
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
