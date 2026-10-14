import { useEffect, useState } from "react";
import { useAgentStore } from "../../lib/agentStore";
import { Section, SettingRow, PasswordInput, TextInput, smallBtnStyle } from "./shared";

// ---------------------------------------------------------------------------
// Gateway config shape returned from daemon via IPC
// ---------------------------------------------------------------------------
interface DaemonGatewayConfig {
  slack_token?: string;
  discord_token?: string;
  telegram_token?: string;
  slack_channel_filter?: string;
  discord_channel_filter?: string;
  discord_allowed_users?: string;
  telegram_allowed_chats?: string;
  gateway_electron_bridges_enabled?: boolean;
}

// ---------------------------------------------------------------------------
// GatewayHealth — per-platform status badges from agentStore
// ---------------------------------------------------------------------------
export function GatewayHealth() {
  const gatewayStatuses = useAgentStore((s) => s.gatewayStatuses);
  const platforms = Object.entries(gatewayStatuses);

  if (platforms.length === 0) {
    return (
      <div style={{ fontSize: "var(--text-sm)", color: "var(--text-muted)", padding: "4px 0" }}>
        No gateway status events received yet. Start the daemon with a gateway token to see connection health.
      </div>
    );
  }

  const statusColors: Record<string, string> = {
    connected: "var(--success)",
    error: "var(--danger)",
    disconnected: "var(--text-muted)",
  };

  const statusLabels: Record<string, string> = {
    connected: "Connected",
    error: "Error",
    disconnected: "Disconnected",
  };

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
      {platforms.map(([platform, info]) => (
        <div key={platform} style={{
          display: "flex", alignItems: "center", gap: 8, padding: "4px 0",
        }}>
          <span style={{
            width: 8, height: 8, borderRadius: "50%",
            background: statusColors[info.status] ?? "var(--text-muted)",
            boxShadow: info.status === "connected" ? "0 0 8px var(--success)" : "none",
            flexShrink: 0,
          }} />
          <span style={{ fontSize: "var(--text-sm)", fontWeight: 600, textTransform: "capitalize", minWidth: 70 }}>
            {platform}
          </span>
          <span style={{
            fontSize: "var(--text-sm)",
            color: statusColors[info.status] ?? "var(--text-muted)",
          }}>
            {statusLabels[info.status] ?? info.status}
          </span>
          {info.consecutiveFailures != null && info.consecutiveFailures > 0 ? (
            <span style={{ fontSize: "var(--text-sm)", color: "var(--danger)", marginLeft: 4 }}>
              ({info.consecutiveFailures} failures)
            </span>
          ) : null}
          {info.lastError ? (
            <span style={{
              fontSize: "var(--text-sm)", color: "var(--danger)", marginLeft: 4,
              overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", maxWidth: 200,
            }}>
              {info.lastError}
            </span>
          ) : null}
        </div>
      ))}
    </div>
  );
}

// ---------------------------------------------------------------------------
// GatewayConfigEditor — IPC-backed config editor reading from daemon
// ---------------------------------------------------------------------------
export function GatewayConfigEditor() {
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [config, setConfig] = useState<DaemonGatewayConfig>({});
  const [dirty, setDirty] = useState(false);

  useEffect(() => {
    loadConfig();
  }, []);

  async function loadConfig() {
    setLoading(true);
    try {
      const zorai = (window as unknown as Record<string, unknown>).zorai ?? (window as unknown as Record<string, unknown>).zorai;
      if (zorai && typeof (zorai as Record<string, unknown>).gatewayGetConfig === "function") {
        const gw = await ((zorai as Record<string, () => Promise<DaemonGatewayConfig>>).gatewayGetConfig)();
        setConfig((gw ?? {}) as DaemonGatewayConfig);
      }
    } catch {
      // IPC not available
    }
    setLoading(false);
  }

  function updateField<K extends keyof DaemonGatewayConfig>(key: K, value: DaemonGatewayConfig[K]) {
    setConfig((prev) => ({ ...prev, [key]: value }));
    setDirty(true);
  }

  async function saveConfig() {
    setSaving(true);
    try {
      const zorai = (window as unknown as Record<string, unknown>).zorai ?? (window as unknown as Record<string, unknown>).zorai;
      if (zorai && typeof (zorai as Record<string, unknown>).agentSetConfigItem === "function") {
        const setItem = (zorai as Record<string, (keyPath: string, value: unknown) => Promise<unknown>>).agentSetConfigItem;
        // Write each gateway field back to daemon config
        await setItem("gateway.slack_token", config.slack_token ?? "");
        await setItem("gateway.discord_token", config.discord_token ?? "");
        await setItem("gateway.telegram_token", config.telegram_token ?? "");
        await setItem("gateway.slack_channel_filter", config.slack_channel_filter ?? "");
        await setItem("gateway.discord_channel_filter", config.discord_channel_filter ?? "");
        await setItem("gateway.discord_allowed_users", config.discord_allowed_users ?? "");
        await setItem("gateway.telegram_allowed_chats", config.telegram_allowed_chats ?? "");
        setDirty(false);
      }
    } catch {
      // IPC error
    }
    setSaving(false);
  }

  if (loading) {
    return (
      <div style={{ fontSize: "var(--text-sm)", color: "var(--text-muted)", padding: "8px 0" }}>
        Loading gateway config from daemon...
      </div>
    );
  }

  return (
    <>
      <Section title="Daemon Gateway Tokens">
        <div style={{ fontSize: "var(--text-sm)", color: "var(--text-muted)", marginBottom: 8, lineHeight: 1.5 }}>
          Tokens are read from and saved to the daemon config (single source of truth).
          Changes are sent to the daemon via IPC.
        </div>
        <SettingRow label="Slack Token">
          <PasswordInput value={config.slack_token ?? ""}
            onChange={(value) => updateField("slack_token", value)}
            placeholder="xoxb-..." />
        </SettingRow>
        <SettingRow label="Slack Channels">
          <TextInput value={config.slack_channel_filter ?? ""}
            onChange={(value) => updateField("slack_channel_filter", value)}
            placeholder="general, ops (comma-separated)" />
        </SettingRow>

        <SettingRow label="Discord Token">
          <PasswordInput value={config.discord_token ?? ""}
            onChange={(value) => updateField("discord_token", value)}
            placeholder="Discord bot token" />
        </SettingRow>
        <SettingRow label="Discord Channels">
          <TextInput value={config.discord_channel_filter ?? ""}
            onChange={(value) => updateField("discord_channel_filter", value)}
            placeholder="channel_id1, channel_id2 (comma-separated)" />
        </SettingRow>
        <SettingRow label="Discord Users">
          <TextInput value={config.discord_allowed_users ?? ""}
            onChange={(value) => updateField("discord_allowed_users", value)}
            placeholder="user_id1, user_id2 (comma-separated)" />
        </SettingRow>

        <SettingRow label="Telegram Token">
          <PasswordInput value={config.telegram_token ?? ""}
            onChange={(value) => updateField("telegram_token", value)}
            placeholder="123456:ABC-DEF..." />
        </SettingRow>
        <SettingRow label="Telegram Chats">
          <TextInput value={config.telegram_allowed_chats ?? ""}
            onChange={(value) => updateField("telegram_allowed_chats", value)}
            placeholder="chat_id1, chat_id2 (comma-separated)" />
        </SettingRow>

        {dirty ? (
          <div style={{ marginTop: 8, display: "flex", gap: 8 }}>
            <button onClick={saveConfig} disabled={saving} style={{
              ...smallBtnStyle, color: "var(--success)", borderColor: "color-mix(in srgb, var(--success) 20%, transparent)",
            }}>
              {saving ? "Saving..." : "Save to Daemon"}
            </button>
            <button onClick={loadConfig} style={smallBtnStyle}>Discard</button>
          </div>
        ) : null}
      </Section>
    </>
  );
}

// ---------------------------------------------------------------------------
// GatewaySettingsPanel — combined health + IPC config
// ---------------------------------------------------------------------------
export function GatewaySettingsPanel() {
  return (
    <>
      <Section title="Connection Status">
        <GatewayHealth />
      </Section>
      <GatewayConfigEditor />
    </>
  );
}
