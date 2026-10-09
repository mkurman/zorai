import { useCallback, useEffect, useState, type ReactNode } from "react";
import { useAgentStore, type AgentMlflowTracingSettings, type MlflowCaptureMode } from "@/lib/agentStore";
import { getBridge } from "@/lib/bridge";
import {
  EMPTY_MLFLOW_STATUS,
  getMlflowTracingStatus,
  sendMlflowTracingTestTrace,
  testMlflowTracingConnection,
  type MlflowTracingStatus,
  type MlflowTracingTestResult,
} from "@/lib/mlflowTracing";

const SCOPE_ROWS = [
  ["visible_operator", "Visible operator turns", "Trace operator-visible conversation turns."],
  ["gateway", "Gateway turns", "Trace Slack, Discord, Telegram, and WhatsApp turns."],
  ["goal_task", "Goal/task turns", "Trace goal and task execution turns."],
  ["subagent", "Subagent turns", "Trace delegated subagent turns."],
  ["concierge", "Concierge turns", "Trace Rarog concierge turns."],
  ["heartbeat_autonomous", "Heartbeat/autonomous", "Trace heartbeat and other autonomous maintenance."],
] as const;

const ADVANCED_BOUNDS = [
  ["max_user_chars", "User characters", 256, 1_000_000],
  ["max_assistant_chars", "Assistant characters", 256, 1_000_000],
  ["max_reasoning_chars", "Reasoning characters", 0, 1_000_000],
  ["max_tool_argument_chars", "Tool argument characters", 256, 1_000_000],
  ["max_tool_result_chars", "Tool result characters", 256, 4_000_000],
  ["batch_size", "Batch size", 1, 256],
  ["flush_interval_ms", "Flush interval (ms)", 100, 60_000],
  ["queue_capacity", "Queue capacity", 1, 10_000],
  ["request_timeout_ms", "Request timeout (ms)", 500, 120_000],
  ["max_retries", "Maximum retries", 0, 10],
] as const;

export function MlflowPanel() {
  const config = useAgentStore((state) => state.agentSettings.mlflow_tracing);
  const updateAgentSetting = useAgentStore((state) => state.updateAgentSetting);
  const [status, setStatus] = useState<MlflowTracingStatus>(EMPTY_MLFLOW_STATUS);
  const [busy, setBusy] = useState<"connection" | "trace" | null>(null);
  const [result, setResult] = useState<MlflowTracingTestResult | null>(null);
  const [headerNames, setHeaderNames] = useState<string[]>([]);
  const [headerName, setHeaderName] = useState("");
  const [headerValue, setHeaderValue] = useState("");
  const [advanced, setAdvanced] = useState(false);

  const refresh = useCallback(async () => {
    setStatus(await getMlflowTracingStatus());
    const response = await getBridge()?.agentListMlflowTracingHeaders?.();
    if (response && typeof response === "object" && "names" in response && Array.isArray(response.names)) {
      setHeaderNames(response.names.filter((name): name is string => typeof name === "string"));
    }
  }, []);

  useEffect(() => {
    void refresh();
    const timer = window.setInterval(() => {
      if (!document.hidden) void refresh();
    }, 3000);
    return () => window.clearInterval(timer);
  }, [refresh]);

  const updateConfig = (patch: Partial<AgentMlflowTracingSettings>) => {
    updateAgentSetting("mlflow_tracing", { ...config, ...patch });
  };
  const updateScope = (key: keyof AgentMlflowTracingSettings["scopes"], value: boolean) => {
    updateConfig({ scopes: { ...config.scopes, [key]: value } });
  };
  const runAction = async (kind: "connection" | "trace") => {
    setBusy(kind);
    const next = kind === "connection"
      ? await testMlflowTracingConnection()
      : await sendMlflowTracingTestTrace();
    setResult(next);
    await refresh();
    setBusy(null);
  };
  const overridden = (field: string) => Boolean(status.overrides[field]);
  const nonLoopbackHttp = /^http:\/\/(?!localhost(?::|\/)|127\.0\.0\.1(?::|\/)|\[::1\](?::|\/))/i.test(config.tracking_uri);

  return (
    <div className="zorai-settings-grid">
      <section className="zorai-panel">
        <div>
          <div className="zorai-section-label">MLflow</div>
          <h2>Conversation tracing</h2>
        </div>
        <p className="zorai-empty-state">
          One fail-open OTLP trace per Zorai turn. An unavailable MLflow server never blocks a conversation.
        </p>
        <SettingRow label="Enabled" description="Export daemon-wide agent turns to MLflow.">
          <Switch checked={config.enabled} onChange={(enabled) => updateConfig({ enabled })} />
        </SettingRow>
        {SCOPE_ROWS.map(([key, label, description]) => (
          <SettingRow key={key} label={label} description={description}>
            <Switch checked={config.scopes[key]} onChange={(value) => updateScope(key, value)} />
          </SettingRow>
        ))}
        <SettingRow
          label={`Tracking URI${overridden("tracking_uri") ? " (ENV)" : ""}`}
          description="MLflow tracking server, for example http://127.0.0.1:5000."
        >
          <input
            className="zorai-input"
            value={config.tracking_uri}
            disabled={overridden("tracking_uri")}
            onChange={(event) => updateConfig({ tracking_uri: event.target.value })}
          />
        </SettingRow>
        {nonLoopbackHttp ? (
          <p className="zorai-empty-state">Plain HTTP outside loopback can expose traced content. Prefer HTTPS.</p>
        ) : null}
        <SettingRow
          label={`Experiment${overridden("experiment_name") ? " (ENV)" : ""}`}
          description="Experiment name used when no experiment ID is set."
        >
          <input
            className="zorai-input"
            value={config.experiment_name}
            disabled={overridden("experiment_name")}
            onChange={(event) => updateConfig({ experiment_name: event.target.value })}
          />
        </SettingRow>
        <SettingRow
          label={`Experiment ID${overridden("experiment_id") ? " (ENV)" : ""}`}
          description="Optional numeric experiment ID. Leave empty to resolve or create by name."
        >
          <input
            className="zorai-input"
            value={config.experiment_id ?? ""}
            disabled={overridden("experiment_id")}
            placeholder="<by name>"
            onChange={(event) => updateConfig({ experiment_id: event.target.value.trim() || null })}
          />
        </SettingRow>
        <SettingRow
          label={`Capture mode${overridden("capture_mode") ? " (ENV)" : ""}`}
          description="Guarded exports scrubbed dialogue and omits reasoning. Full still removes credentials."
        >
          <select
            className="zorai-input"
            value={config.capture_mode}
            disabled={overridden("capture_mode")}
            onChange={(event) => updateConfig({ capture_mode: event.target.value as MlflowCaptureMode })}
          >
            {["metadata", "guarded", "full"].map((mode) => (
              <option key={mode} value={mode}>{mode}</option>
            ))}
          </select>
        </SettingRow>
      </section>

      <section className="zorai-panel">
        <div>
          <div className="zorai-section-label">MLflow</div>
          <h2>Connection and status</h2>
        </div>
        <div className="zorai-card-actions">
          <button type="button" className="zorai-ghost-button" disabled={busy !== null} onClick={() => void runAction("connection")}>
            {busy === "connection" ? "Testing..." : "Test connection"}
          </button>
          <button type="button" className="zorai-ghost-button" disabled={busy !== null} onClick={() => void runAction("trace")}>
            {busy === "trace" ? "Sending..." : "Send test trace"}
          </button>
          {status.experiment_id ? (
            <button
              type="button"
              className="zorai-ghost-button"
              onClick={() => window.open(`${config.tracking_uri.replace(/\/$/, "")}/#/experiments/${status.experiment_id}`, "_blank", "noopener,noreferrer")}
            >
              Open experiment
            </button>
          ) : null}
        </div>
        {result ? (
          <p className="zorai-empty-state">
            {result.ok
              ? `Test: connected${result.connection?.server_version ? ` to MLflow ${result.connection.server_version}` : ""}`
              : `Test: ${result.error ?? "failed"}`}
          </p>
        ) : null}
        <div className="zorai-setting-row"><div><strong>Status</strong><span>{status.state}</span></div></div>
        <div className="zorai-setting-row"><div><strong>Server</strong><span>{status.server_version ?? "—"}</span></div></div>
        <div className="zorai-setting-row"><div><strong>Experiment ID</strong><span>{status.experiment_id ?? "—"}</span></div></div>
        <div className="zorai-setting-row"><div><strong>Queue</strong><span>{status.queue_depth}/{status.queue_capacity}</span></div></div>
        <div className="zorai-setting-row"><div><strong>Exported</strong><span>{String(status.traces_exported)}</span></div></div>
        <div className="zorai-setting-row"><div><strong>Dropped</strong><span>{String(status.traces_dropped)}</span></div></div>
        {status.last_error ? <p className="zorai-empty-state">{status.last_error}</p> : null}
      </section>

      <section className="zorai-panel">
        <div>
          <div className="zorai-section-label">MLflow</div>
          <h2>Encrypted custom headers</h2>
        </div>
        <p className="zorai-empty-state">Header values are encrypted locally. Only names are shown after storage.</p>
        <SettingRow label="Header name" description="For example Authorization.">
          <input className="zorai-input" value={headerName} placeholder="Authorization" onChange={(event) => setHeaderName(event.target.value)} />
        </SettingRow>
        <SettingRow label="Header value" description="Secret value stored by the daemon.">
          <input className="zorai-input" type="password" value={headerValue} placeholder="Secret value" onChange={(event) => setHeaderValue(event.target.value)} />
        </SettingRow>
        <button
          type="button"
          className="zorai-ghost-button"
          disabled={!headerName.trim() || !headerValue}
          onClick={async () => {
            const response = await getBridge()?.agentSetMlflowTracingHeader?.(headerName.trim(), headerValue);
            if (response && typeof response === "object" && "names" in response && Array.isArray(response.names)) {
              setHeaderNames(response.names as string[]);
            }
            setHeaderName("");
            setHeaderValue("");
          }}
        >
          Store header
        </button>
        <div className="zorai-card-actions">
          {headerNames.map((name) => (
            <button
              key={name}
              type="button"
              className="zorai-ghost-button"
              title="Delete encrypted header"
              onClick={async () => {
                const response = await getBridge()?.agentDeleteMlflowTracingHeader?.(name);
                if (response && typeof response === "object" && "names" in response && Array.isArray(response.names)) {
                  setHeaderNames(response.names as string[]);
                }
              }}
            >
              {name} ×
            </button>
          ))}
        </div>
      </section>

      <button type="button" className="zorai-ghost-button" onClick={() => setAdvanced((value) => !value)}>
        {advanced ? "Hide advanced bounds" : "Show advanced bounds"}
      </button>
      {advanced ? (
        <section className="zorai-panel">
          <div>
            <div className="zorai-section-label">MLflow</div>
            <h2>Advanced bounds</h2>
          </div>
          {ADVANCED_BOUNDS.map(([key, label, min, max]) => (
            <SettingRow key={key} label={label} description={`Clamp ${min}–${max}.`}>
              <input
                className="zorai-input"
                type="number"
                min={min}
                max={max}
                value={config[key]}
                onChange={(event) => updateConfig({ [key]: Number(event.target.value) })}
              />
            </SettingRow>
          ))}
        </section>
      ) : null}
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

function Switch({ checked, onChange }: { checked: boolean; onChange: (checked: boolean) => void }) {
  return (
    <button
      type="button"
      className={["zorai-switch", checked ? "zorai-switch--on" : ""].filter(Boolean).join(" ")}
      aria-pressed={checked}
      onClick={() => onChange(!checked)}
    >
      <span />
    </button>
  );
}
