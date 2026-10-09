import { useCallback, useEffect, useState } from "react";
import type { AgentMlflowTracingSettings, AgentSettings, MlflowCaptureMode } from "../../lib/agentStore";
import { getBridge } from "../../lib/bridge";
import {
  EMPTY_MLFLOW_STATUS,
  getMlflowTracingStatus,
  sendMlflowTracingTestTrace,
  testMlflowTracingConnection,
  type MlflowTracingStatus,
  type MlflowTracingTestResult,
} from "../../lib/mlflowTracing";
import { NumberInput, Section, SelectInput, SettingRow, TextInput, Toggle } from "./shared";

export function MlflowTracingTab({
  settings,
  updateSetting,
}: {
  settings: AgentSettings;
  updateSetting: <K extends keyof AgentSettings>(key: K, value: AgentSettings[K]) => void;
}) {
  const config = settings.mlflow_tracing;
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
    updateSetting("mlflow_tracing", { ...config, ...patch });
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
    <div style={{ maxWidth: 850 }}>
      <Section title="MLflow tracing">
        <p style={{ fontSize: "var(--text-sm)", color: "var(--text-secondary)", lineHeight: 1.5 }}>
          Export one OpenTelemetry trace per Zorai turn. Tracing is daemon-owned, bounded, and fail-open: an unavailable MLflow server never blocks a conversation.
        </p>
        <SettingRow label="Enabled">
          <Toggle value={config.enabled} onChange={(enabled) => updateConfig({ enabled })} />
        </SettingRow>
        <SettingRow label={`Tracking URI${overridden("tracking_uri") ? " (ENV)" : ""}`}>
          <TextInput value={config.tracking_uri} disabled={overridden("tracking_uri")}
            onChange={(tracking_uri) => updateConfig({ tracking_uri })} />
        </SettingRow>
        {nonLoopbackHttp ? <div style={{ color: "var(--warning, #f9e2af)", fontSize: "var(--text-sm)" }}>Plain HTTP outside loopback can expose traced content. Prefer HTTPS.</div> : null}
        <SettingRow label={`Experiment${overridden("experiment_name") ? " (ENV)" : ""}`}>
          <TextInput value={config.experiment_name} disabled={overridden("experiment_name")}
            onChange={(experiment_name) => updateConfig({ experiment_name })} />
        </SettingRow>
        <SettingRow label={`Experiment ID override${overridden("experiment_id") ? " (ENV)" : ""}`}>
          <TextInput value={config.experiment_id ?? ""} disabled={overridden("experiment_id")}
            placeholder="Resolve/create by name" onChange={(value) => updateConfig({ experiment_id: value.trim() || null })} />
        </SettingRow>
        <SettingRow label={`Capture mode${overridden("capture_mode") ? " (ENV)" : ""}`}>
          <SelectInput value={config.capture_mode} options={["metadata", "guarded", "full"]}
            onChange={(capture_mode) => updateConfig({ capture_mode: capture_mode as MlflowCaptureMode })} />
        </SettingRow>
        <div style={{ fontSize: "var(--text-sm)", color: "var(--text-secondary)", marginBottom: 8 }}>
          Guarded exports scrubbed dialogue, omits reasoning, and summarizes/caps tools. Full still removes credentials and binary bodies.
        </div>
      </Section>

      <Section title="Execution scopes">
        {([
          ["visible_operator", "Visible operator turns"],
          ["gateway", "Gateway turns"],
          ["goal_task", "Goal and task turns"],
          ["subagent", "Subagent turns"],
          ["concierge", "Concierge turns"],
          ["heartbeat_autonomous", "Heartbeat/autonomous maintenance"],
        ] as const).map(([key, label]) => (
          <SettingRow key={key} label={label}>
            <Toggle value={config.scopes[key]} onChange={(value) => updateScope(key, value)} />
          </SettingRow>
        ))}
      </Section>

      <Section title="Connection and status">
        <div style={{ display: "flex", gap: 8, marginBottom: 10 }}>
          <button disabled={busy !== null} onClick={() => void runAction("connection")}>Test connection</button>
          <button disabled={busy !== null} onClick={() => void runAction("trace")}>Send test trace</button>
          {status.experiment_id ? <button onClick={() => window.open(`${config.tracking_uri}/#/experiments/${status.experiment_id}`, "_blank", "noopener,noreferrer")}>Open experiment</button> : null}
        </div>
        {result ? <div style={{ fontSize: "var(--text-sm)", color: result.ok ? "var(--success, #a6e3a1)" : "var(--error, #f38ba8)", marginBottom: 8 }}>
          {result.ok ? `Connected to MLflow ${result.connection?.server_version ?? ""}` : result.error}
        </div> : null}
        <div style={{ display: "grid", gridTemplateColumns: "repeat(3, minmax(120px, 1fr))", gap: 8, fontSize: "var(--text-sm)" }}>
          <Metric label="State" value={status.state} />
          <Metric label="Server" value={status.server_version ?? "—"} />
          <Metric label="Experiment" value={status.experiment_id ?? "—"} />
          <Metric label="Queue" value={`${status.queue_depth}/${status.queue_capacity}`} />
          <Metric label="Exported" value={String(status.traces_exported)} />
          <Metric label="Dropped" value={String(status.traces_dropped)} />
        </div>
        {status.last_error ? <div style={{ marginTop: 8, color: "var(--error, #f38ba8)", fontSize: "var(--text-sm)" }}>{status.last_error}</div> : null}
      </Section>

      <Section title="Encrypted custom headers">
        <div style={{ display: "flex", gap: 8 }}>
          <input aria-label="Header name" value={headerName} placeholder="Authorization" onChange={(event) => setHeaderName(event.target.value)} />
          <input aria-label="Header value" type="password" value={headerValue} placeholder="Secret value" onChange={(event) => setHeaderValue(event.target.value)} />
          <button disabled={!headerName.trim() || !headerValue} onClick={async () => {
            const response = await getBridge()?.agentSetMlflowTracingHeader?.(headerName.trim(), headerValue);
            if (response && typeof response === "object" && "names" in response && Array.isArray(response.names)) setHeaderNames(response.names as string[]);
            setHeaderName(""); setHeaderValue("");
          }}>Store</button>
        </div>
        <div style={{ marginTop: 8, display: "flex", flexWrap: "wrap", gap: 6 }}>
          {headerNames.map((name) => <button key={name} title="Delete encrypted header" onClick={async () => {
            const response = await getBridge()?.agentDeleteMlflowTracingHeader?.(name);
            if (response && typeof response === "object" && "names" in response && Array.isArray(response.names)) setHeaderNames(response.names as string[]);
          }}>{name} ×</button>)}
        </div>
      </Section>

      <button onClick={() => setAdvanced((value) => !value)}>{advanced ? "Hide advanced" : "Show advanced"}</button>
      {advanced ? <Section title="Advanced bounds">
        {([
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
        ] as const).map(([key, label, min, max]) => <SettingRow key={key} label={label}>
          <NumberInput value={config[key]} min={min} max={max} onChange={(value) => updateConfig({ [key]: value })} />
        </SettingRow>)}
      </Section> : null}
    </div>
  );
}

function Metric({ label, value }: { label: string; value: string }) {
  return <div style={{ border: "1px solid var(--border)", padding: "8px 10px" }}><div style={{ color: "var(--text-secondary)" }}>{label}</div><strong>{value}</strong></div>;
}
