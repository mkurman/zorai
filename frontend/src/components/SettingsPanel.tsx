import { useEffect, useRef, useState, type CSSProperties } from "react";
import { getBridge } from "@/lib/bridge";
import { useWorkspaceStore } from "../lib/workspaceStore";
import { useSettingsStore } from "../lib/settingsStore";
import { useAgentStore } from "../lib/agentStore";
import { useAgentMissionStore } from "../lib/agentMissionStore";
import {
  buildDaemonAgentConfig,
  diffDaemonConfigEntries,
  getAgentBridge,
  shouldUseDaemonRuntime,
} from "../lib/agentDaemonConfig";
import { AboutTab } from "./settings-panel/AboutTab";
import { AppearanceTab } from "./settings-panel/AppearanceTab";
import { AgentTab } from "./settings-panel/AgentTab";
import { BehaviorTab } from "./settings-panel/BehaviorTab";
import { GatewayTab } from "./settings-panel/GatewayTab";
import { KeyboardTab } from "./settings-panel/KeyboardTab";
import { TerminalTab } from "./settings-panel/TerminalTab";
import { MlflowTracingTab } from "./settings-panel/MlflowTracingTab";
import { ProviderAuthTab } from "./settings-panel/ProviderAuthTab";
import { SubAgentsTab } from "./settings-panel/SubAgentsTab";
import { ConciergeSection } from "./settings-panel/ConciergeSection";
import { PluginsTab } from "./settings-panel/PluginsTab";
import { DatabaseTab } from "./settings-panel/DatabaseTab";
import { TierGatedSection } from "./base-components/TierGatedSection";
import {
  headerBtnStyle,
} from "./settings-panel/shared";
import { useTierStore, type CapabilityTier } from "../lib/tierStore";
import { CONCIERGE_AGENT_NAME, PRIMARY_AGENT_NAME } from "../lib/agentNames";

type SettingsTab = "appearance" | "terminal" | "behavior" | "auth" | "agent" | "mlflow" | "concierge" | "subagents" | "gateway" | "keyboard" | "about" | "plugins" | "database";

type SettingsPanelProps = {
  style?: CSSProperties;
  className?: string;
};

/**
 * Full settings panel matching zorai-windows SettingsWindow.
 * 6 sections: Appearance, Terminal, Behavior, Agent, Keyboard, About.
 */
export function SettingsPanel({ style, className }: SettingsPanelProps = {}) {
  const open = useWorkspaceStore((s) => s.settingsOpen);
  const toggle = useWorkspaceStore((s) => s.toggleSettings);
  const settings = useSettingsStore((s) => s.settings);
  const updateSetting = useSettingsStore((s) => s.updateSetting);
  const resetSettings = useSettingsStore((s) => s.resetSettings);
  const profiles = useSettingsStore((s) => s.profiles);
  const addProfile = useSettingsStore((s) => s.addProfile);
  const removeProfile = useSettingsStore((s) => s.removeProfile);
  const updateProfile = useSettingsStore((s) => s.updateProfile);
  const setDefaultProfile = useSettingsStore((s) => s.setDefaultProfile);
  const agentSettings = useAgentStore((s) => s.agentSettings);
  const agentSettingsHydrated = useAgentStore((s) => s.agentSettingsHydrated);
  const updateAgentSetting = useAgentStore((s) => s.updateAgentSetting);
  const resetAgentSettings = useAgentStore((s) => s.resetAgentSettings);
  const refreshAgentSettingsFromDaemon = useAgentStore((s) => s.refreshAgentSettingsFromDaemon);
  const markAgentSettingsSynced = useAgentStore((s) => s.markAgentSettingsSynced);
  const approvals = useAgentMissionStore((s) => s.approvals);
  const snapshots = useAgentMissionStore((s) => s.snapshots);
  const currentTier = useTierStore((s) => s.currentTier);

  const handleTierOverride = async (newTier: string) => {
    const bridge = getBridge();
    if (!bridge) return;
    const tierValue = newTier === "auto" ? null : newTier;
    await bridge.agentSetTierOverride?.(tierValue);
    if (tierValue) {
      useTierStore.getState().setTier(tierValue as CapabilityTier);
    }
  };

  const [tab, setTab] = useState<SettingsTab>("appearance");
  const [systemFonts, setSystemFonts] = useState<string[]>([]);
  const [availableShells, setAvailableShells] = useState<{ name: string; path: string; args?: string }[]>([]);
  const [isFullscreen, setIsFullscreen] = useState(true);
  const lastDaemonConfigJsonRef = useRef<string | null>(null);

  useEffect(() => {
    if (open && systemFonts.length === 0) {
      // Load system fonts via Electron IPC
      if (typeof window !== "undefined" && ("zorai" in window || "zorai" in window)) {
        const bridge = getBridge();
        bridge?.getSystemFonts?.().then((fonts: string[]) => {
          setSystemFonts(fonts);
        });
        bridge?.getAvailableShells?.().then(
          (shells: { name: string; path: string; args?: string }[]) => setAvailableShells(shells),
        ).catch(() => { });
      }
    }
  }, [open]);

  useEffect(() => {
    if (open) {
      setIsFullscreen(true);
    }
  }, [open]);

  useEffect(() => {
    if (!open) return;
    void refreshAgentSettingsFromDaemon().then((ok) => {
      if (!ok) return;
      const latestAgentSettings = useAgentStore.getState().agentSettings;
      lastDaemonConfigJsonRef.current = JSON.stringify(
        buildDaemonAgentConfig(latestAgentSettings, settings),
      );
    });
  }, [open, refreshAgentSettingsFromDaemon, settings]);

  useEffect(() => {
    if (!open) return;
    if (!agentSettingsHydrated) return;
    if (!shouldUseDaemonRuntime(agentSettings.agent_backend)) return;
    const bridge = getAgentBridge();
    if (!bridge?.agentSetConfigItem) return;
    const nextConfig = buildDaemonAgentConfig(agentSettings, settings);
    const nextConfigJson = JSON.stringify(nextConfig);
    if (lastDaemonConfigJsonRef.current === null) return;
    if (lastDaemonConfigJsonRef.current === nextConfigJson) return;
    const previousConfig = JSON.parse(lastDaemonConfigJsonRef.current);
    const providerChanged = previousConfig.provider !== nextConfig.provider;
    const modelChanged = previousConfig.model !== nextConfig.model;
    const canSwitchProviderModel = (providerChanged || modelChanged)
      && typeof bridge.agentSetProviderModel === "function";
    const changes = diffDaemonConfigEntries(previousConfig, nextConfig);
    const remainingChanges = canSwitchProviderModel
      ? changes.filter(({ keyPath }) => keyPath !== "/provider" && keyPath !== "/model" && keyPath !== "/base_url")
      : changes;
    const syncConfig = async () => {
      if (canSwitchProviderModel) {
        await bridge.agentSetProviderModel?.(nextConfig.provider, nextConfig.model);
      }
      if (remainingChanges.length > 0) {
        await Promise.all(
          remainingChanges.map(({ keyPath, value }) => bridge.agentSetConfigItem?.(keyPath, value)),
        );
      }
    };
    void syncConfig().then(() => {
      lastDaemonConfigJsonRef.current = nextConfigJson;
      markAgentSettingsSynced();
    }).catch(() => { });
  }, [
    open,
    settings,
    agentSettings,
    agentSettingsHydrated,
    markAgentSettingsSynced,
    settings,
  ]);

  useEffect(() => {
    const handleOpenTab = (event: Event) => {
      const customEvent = event as CustomEvent<{ tab?: SettingsTab }>;
      const requestedTab = customEvent.detail?.tab;
      if (!requestedTab) return;

      if (["appearance", "terminal", "behavior", "auth", "agent", "concierge", "subagents", "gateway", "keyboard", "about", "plugins", "database"].includes(requestedTab)) {
        setTab(requestedTab);
      }
    };

    window.addEventListener("zorai-open-settings-tab", handleOpenTab as EventListener);
    window.addEventListener("zorai-open-settings-tab", handleOpenTab as EventListener);
    return () => {
      window.removeEventListener("zorai-open-settings-tab", handleOpenTab as EventListener);
      window.removeEventListener("zorai-open-settings-tab", handleOpenTab as EventListener);
    };
  }, []);

  if (!open) return null;

  const tabs: { id: SettingsTab; label: string }[] = [
    { id: "appearance", label: "Interface" },
    { id: "terminal", label: "Execution" },
    { id: "behavior", label: "Operator" },
    { id: "auth", label: "Auth" },
    { id: "agent", label: PRIMARY_AGENT_NAME },
    { id: "mlflow", label: "MLflow" },
    { id: "concierge", label: CONCIERGE_AGENT_NAME },
    { id: "subagents", label: "Sub-Agents" },
    { id: "gateway", label: "Gateway" },
    { id: "keyboard", label: "Bindings" },
    { id: "about", label: "Runtime" },
    { id: "plugins", label: "Plugins" },
    { id: "database", label: "Database" },
  ];

  return (
    <div
      style={{
        ...(isFullscreen
          ? {
            position: "fixed" as const,
            inset: 0,
            zIndex: 3600,
            width: "100vw",
            height: "100vh",
            borderRadius: 0,
          }
          : {
            width: 720,
            minWidth: 420,
            maxWidth: 1100,
            height: "100%",
            resize: "horizontal" as const,
          }),
        display: "flex",
        flexDirection: "column",
        padding: "20px",
        background: "color-mix(in srgb, var(--shade) 80%, transparent)",
        border: "1px solid var(--border)",
        borderRadius: isFullscreen ? 0 : "var(--radius-xl)",
        overflow: "hidden",
        ...(style ?? {}),
      }}
      className={className}
    >
      <div
        style={{
          background: "var(--zorai-bg)",
          borderBottom: "1px solid var(--border)",
          display: "flex",
          height: "100%",
          flexDirection: "column",
          overflow: "hidden",
        }}
      >
        <div style={{
          display: "grid", gap: 14,
          padding: "18px 22px 16px", borderBottom: "1px solid color-mix(in srgb, var(--text-primary) 8%, transparent)",
        }}>
          <div style={{ display: "flex", alignItems: "start", justifyContent: "space-between", gap: 16 }}>
            <div style={{ display: "grid", gap: 6 }}>
              <span className="zorai-panel-title" style={{ color: "var(--mission)" }}>Operator Configuration</span>
              <span style={{ fontSize: "var(--text-2xl)", fontWeight: 800 }}>Mission Runtime Settings</span>
              <span style={{ fontSize: "var(--text-sm)", color: "var(--text-secondary)", lineHeight: 1.45 }}>
                Tune visual hierarchy, terminal execution, providers, bindings, and runtime ergonomics from one control plane.
              </span>
            </div>
            <div style={{ display: "flex", gap: 8 }}>
              <button
                onClick={() => setIsFullscreen((prev) => !prev)}
                style={headerBtnStyle}
                title={isFullscreen ? "Switch to resizable window" : "Expand fullscreen"}
              >
                {isFullscreen ? "Window" : "Fullscreen"}
              </button>
              <button onClick={() => { resetSettings(); resetAgentSettings(); }} style={headerBtnStyle} title="Reset all">Reset</button>
              <button onClick={toggle} style={headerBtnStyle}>✕</button>
            </div>
          </div>
          <div style={{ display: "grid", gridTemplateColumns: "repeat(4, minmax(0, 1fr))", gap: 10 }}>
            <SettingsMetric label="Theme" value={settings.themeName} />
            <SettingsMetric label={`${PRIMARY_AGENT_NAME} Provider`} value={agentSettings.active_provider} />
            <SettingsMetric label="Approvals" value={String(approvals.filter((entry) => entry.status === "pending").length)} />
            <SettingsMetric label="Snapshots" value={String(snapshots.length)} />
          </div>
          <div style={{ display: "flex", alignItems: "center", gap: 10, marginTop: 6 }}>
            <span style={{ fontSize: "var(--text-sm)", color: "var(--text-secondary)", fontWeight: 600 }}>Experience Level</span>
            <select
              value={currentTier}
              onChange={(e) => void handleTierOverride(e.target.value)}
              style={{
                background: "color-mix(in srgb, var(--bg-surface) 80%, transparent)",
                border: "1px solid color-mix(in srgb, var(--text-primary) 12%, transparent)",
                color: "var(--text-primary)",
                padding: "4px 8px",
                fontSize: "var(--text-sm)",
                borderRadius: 4,
                fontFamily: "inherit",
              }}
            >
              <option value="auto">Automatic</option>
              <option value="newcomer">Newcomer</option>
              <option value="familiar">Familiar</option>
              <option value="power_user">Power User</option>
              <option value="expert">Expert</option>
            </select>
            <span style={{ fontSize: "var(--text-sm)", color: "var(--text-secondary)", opacity: 0.7 }}>
              Controls which features are visible
            </span>
          </div>
        </div>

        <div style={{
          display: "flex", borderBottom: "1px solid color-mix(in srgb, var(--text-primary) 8%, transparent)", padding: "0 16px",
          gap: 0, overflow: "auto",
        }}>
          {tabs.map((t) => (
            <button key={t.id} onClick={() => setTab(t.id)} style={{
              background: tab === t.id ? "color-mix(in srgb, var(--info) 10%, transparent)" : "none", border: "none", borderBottom: tab === t.id ? "2px solid var(--accent)" : "2px solid transparent",
              color: tab === t.id ? "var(--text-primary)" : "var(--text-secondary)",
              padding: "12px 14px", fontSize: "var(--text-sm)", cursor: "pointer", fontWeight: tab === t.id ? 700 : 500,
              fontFamily: "inherit", whiteSpace: "nowrap",
            }}>
              {t.label}
            </button>
          ))}
        </div>

        <div style={{ flex: 1, overflow: "auto", padding: "18px 22px" }}>
          {tab === "appearance" && (
            <AppearanceTab settings={settings} updateSetting={updateSetting} systemFonts={systemFonts} />
          )}
          {tab === "terminal" && (
            <TerminalTab
              settings={settings}
              updateSetting={updateSetting}
              availableShells={availableShells}
              profiles={profiles}
              addProfile={addProfile}
              removeProfile={removeProfile}
              updateProfile={updateProfile}
              setDefaultProfile={setDefaultProfile}
            />
          )}
          {tab === "behavior" && (
            <TierGatedSection requiredTier="familiar" label="Task Queue & Scheduling">
              <BehaviorTab settings={settings} updateSetting={updateSetting} />
            </TierGatedSection>
          )}
          {tab === "auth" && <ProviderAuthTab />}
          {tab === "agent" && (
            <AgentTab
              settings={agentSettings}
              updateSetting={updateAgentSetting}
              resetSettings={resetAgentSettings}
            />
          )}
          {tab === "mlflow" && (
            <MlflowTracingTab settings={agentSettings} updateSetting={updateAgentSetting} />
          )}
          {tab === "concierge" && <ConciergeSection />}
          {tab === "subagents" && (
            <TierGatedSection requiredTier="power_user" label="Sub-Agent Management">
              <SubAgentsTab />
            </TierGatedSection>
          )}
          {tab === "gateway" && (
            <TierGatedSection requiredTier="familiar" label="Gateway Configuration">
              <GatewayTab settings={agentSettings} updateSetting={updateAgentSetting} />
            </TierGatedSection>
          )}
          {tab === "keyboard" && <KeyboardTab />}
          {tab === "about" && (
            <TierGatedSection requiredTier="expert" label="Memory & Learning Controls">
              <AboutTab />
            </TierGatedSection>
          )}
          {tab === "plugins" && (
            <TierGatedSection requiredTier="familiar" label="Plugins">
              <PluginsTab />
            </TierGatedSection>
          )}
          {tab === "database" && <DatabaseTab />}
        </div>
      </div>
    </div>
  );
}

function SettingsMetric({ label, value }: { label: string; value: string }) {
  return (
    <div style={{ borderRadius: 0, padding: "10px 12px", border: "1px solid color-mix(in srgb, var(--text-primary) 6%, transparent)", background: "color-mix(in srgb, var(--bg-surface) 80%, transparent)", display: "grid", gap: 4 }}>
      <span className="zorai-panel-title">{label}</span>
      <span style={{ fontSize: "var(--text-base)", fontWeight: 700 }}>{value}</span>
    </div>
  );
}
