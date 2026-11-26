import { useEffect, useState } from "react";
import { getBridge } from "@/lib/bridge";
import { compoundMembers, useDebugStore, type Breakpoint, type DebugVariable } from "./debugStore";
import { useCodeTaskStore } from "../codeTaskStore";

function VariableRow({ variable, depth }: { variable: DebugVariable; depth: number }) {
  const [open, setOpen] = useState(false);
  const children = useDebugStore((state) => (variable.variablesReference ? state.variables[variable.variablesReference] : undefined));
  const loadVariables = useDebugStore((state) => state.loadVariables);
  const expandable = variable.variablesReference > 0;
  return (
    <>
      <button
        type="button"
        className="zorai-debug__variable"
        style={{ paddingLeft: 10 + depth * 12 }}
        title={variable.type ? `${variable.name}: ${variable.type}` : variable.name}
        onClick={() => {
          if (!expandable) return;
          if (!open && !children) void loadVariables(variable.variablesReference);
          setOpen(!open);
        }}
      >
        <span className="zorai-debug__twisty">{expandable ? (open ? "▾" : "▸") : ""}</span>
        <span className="zorai-debug__name">{variable.name}</span>
        <span className="zorai-debug__value">{variable.value}</span>
      </button>
      {open && children ? children.map((child) => <VariableRow key={`${variable.variablesReference}:${child.name}`} variable={child} depth={depth + 1} />) : null}
    </>
  );
}

const BREAKPOINT_FIELDS = [
  { key: "condition", label: "Condition", placeholder: "pause when, e.g. i > 3" },
  { key: "hitCondition", label: "Hit count", placeholder: "pause on hit, e.g. 5" },
  { key: "logMessage", label: "Log message", placeholder: "log instead of pausing: x = {x}" },
] as const;

function BreakpointField({ root, path, breakpoint }: { root: string; path: string; breakpoint: Breakpoint }) {
  const updateBreakpoint = useDebugStore((state) => state.updateBreakpoint);
  const [field, setField] = useState<(typeof BREAKPOINT_FIELDS)[number]["key"]>(breakpoint.logMessage ? "logMessage" : breakpoint.hitCondition ? "hitCondition" : "condition");
  const spec = BREAKPOINT_FIELDS.find((entry) => entry.key === field) ?? BREAKPOINT_FIELDS[0];
  return (
    <>
      <select className="zorai-debug__condition-kind" value={field} aria-label={`Breakpoint kind for ${path}:${breakpoint.line}`} onChange={(event) => setField(event.target.value as typeof field)}>
        {BREAKPOINT_FIELDS.map((entry) => <option key={entry.key} value={entry.key}>{entry.label}</option>)}
      </select>
      <input
        key={field}
        className="zorai-debug__condition"
        defaultValue={breakpoint[field] ?? ""}
        placeholder={spec.placeholder}
        aria-label={`${spec.label} for ${path}:${breakpoint.line}`}
        onBlur={(event) => updateBreakpoint(root, path, breakpoint.line, { [field]: event.target.value.trim() || undefined })}
      />
    </>
  );
}

export function DebugPanel({ root, activeFile, onOpenLocation }: { root: string; activeFile: string | null; onOpenLocation: (path: string, line: number, column: number) => void }) {
  const [configs, setConfigs] = useState<ZoraiDebugConfiguration[]>([]);
  const [adapters, setAdapters] = useState<Array<{ id: string; label: string; available: boolean; hint: string }>>([]);
  const [selected, setSelected] = useState(0);
  const [watchInput, setWatchInput] = useState("");
  const [error, setError] = useState<string | null>(null);
  const store = useDebugStore();
  const breakpoints = store.breakpoints[root] ?? {};
  const active = store.status === "running" || store.status === "stopped" || store.status === "starting";

  useEffect(() => {
    let cancelled = false;
    void getBridge()?.debugConfigurations?.(root, activeFile).then((result) => {
      if (cancelled) return;
      setConfigs(result.configurations);
      setAdapters(result.adapters);
      setSelected((current) => Math.min(current, Math.max(0, result.configurations.length - 1)));
    }).catch(() => undefined);
    return () => { cancelled = true; };
  }, [root, activeFile]);

  const config = configs[selected];
  const start = () => {
    if (!config) return;
    setError(null);
    void (async () => {
      const members = config.type === "compound" ? compoundMembers(config, configs) : [config];
      for (const entry of config.type === "compound" ? [config, ...members] : members) {
        const preLaunch = typeof entry.preLaunchTask === "string" ? entry.preLaunchTask : null;
        if (!preLaunch) continue;
        const tasks = await useCodeTaskStore.getState().discover(root);
        const task = tasks.find((candidate) => candidate.label === preLaunch);
        if (!task) throw new Error(`The preLaunchTask "${preLaunch}" was not found in tasks.json.`);
        const result = await useCodeTaskStore.getState().run(root, task);
        if (result?.status !== "succeeded") throw new Error(`The preLaunchTask "${preLaunch}" failed; see the Tasks panel.`);
      }
      if (config.type === "compound") await store.startCompound(root, activeFile, members);
      else await store.start(root, activeFile, config);
    })().catch((reason) => setError(reason instanceof Error ? reason.message : String(reason)));
  };
  const missing = config ? adapters.filter((adapter) => !adapter.available) : [];

  return (
    <div className="zorai-debug">
      <div className="zorai-debug__launch">
        <select value={selected} aria-label="Launch configuration" onChange={(event) => setSelected(Number(event.target.value))} disabled={configs.length === 0}>
          {configs.length === 0 ? <option>No launch configuration</option> : configs.map((entry, index) => <option key={`${entry.source}:${entry.name}`} value={index}>{entry.name}</option>)}
        </select>
        <button type="button" className="zorai-debug__start" disabled={!config || store.status === "starting"} title="Start Debugging (F5)" onClick={active && store.status === "stopped" ? () => void store.command("continue") : start}>▶</button>
      </div>
      {configs.length === 0 ? <p className="zorai-debug__hint">Add <code>.vscode/launch.json</code> or <code>.zorai/launch.json</code>, or open a Python, Node or Go file for a default configuration.</p> : null}
      {error ? <p className="zorai-debug__hint is-error">{error}</p> : null}
      {active ? (
        <div className="zorai-debug__toolbar" role="toolbar" aria-label="Debug controls">
          <span className="zorai-debug__status" title={[store.session, ...store.companions].map((entry) => entry?.config.name).filter(Boolean).join(", ")}>{store.status === "stopped" ? `Paused${store.stopReason ? ` · ${store.stopReason}` : ""}` : store.status === "starting" ? "Starting…" : "Running"}{store.companions.length > 0 ? ` · ${store.session?.config.name} +${store.companions.length}` : ""}</span>
          {store.status === "stopped"
            ? <button type="button" title="Continue (F5)" onClick={() => void store.command("continue")}>⏵</button>
            : <button type="button" title="Pause (F6)" onClick={() => void store.command("pause")}>⏸</button>}
          <button type="button" title="Step Over (F10)" disabled={store.status !== "stopped"} onClick={() => void store.command("next")}>↷</button>
          <button type="button" title="Step Into (F11)" disabled={store.status !== "stopped"} onClick={() => void store.command("stepIn")}>↓</button>
          <button type="button" title="Step Out (Shift+F11)" disabled={store.status !== "stopped"} onClick={() => void store.command("stepOut")}>↑</button>
          <button type="button" title="Restart" onClick={() => void store.restart()}>⟲</button>
          <button type="button" className="is-danger" title="Stop (Shift+F5)" onClick={() => void store.stop()}>■</button>
        </div>
      ) : null}
      <details className="zorai-debug__section" open>
        <summary>Variables</summary>
        {store.status !== "stopped" ? <p className="zorai-debug__hint">Variables appear when execution pauses.</p> : store.scopes.map((scope) => (
          <VariableRow key={scope.variablesReference} variable={{ name: scope.name, value: "", variablesReference: scope.variablesReference }} depth={0} />
        ))}
      </details>
      <details className="zorai-debug__section" open>
        <summary>Watch</summary>
        {store.watches.map((expression) => (
          <div key={expression} className="zorai-debug__watch">
            <span className="zorai-debug__name">{expression}</span>
            <span className="zorai-debug__value">{store.status === "stopped" ? store.watchValues[expression] ?? "…" : ""}</span>
            <button type="button" aria-label={`Remove watch ${expression}`} onClick={() => store.removeWatch(expression)}>×</button>
          </div>
        ))}
        <input className="zorai-debug__input" value={watchInput} placeholder="Add expression" aria-label="Add watch expression" onChange={(event) => setWatchInput(event.target.value)} onKeyDown={(event) => {
          if (event.key !== "Enter") return;
          store.addWatch(watchInput);
          setWatchInput("");
        }} />
      </details>
      <details className="zorai-debug__section" open>
        <summary>Call Stack</summary>
        {store.frames.length === 0 ? <p className="zorai-debug__hint">Not paused.</p> : store.frames.map((frame) => (
          <button key={frame.id} type="button" className={frame.id === store.frameId ? "zorai-debug__frame is-active" : "zorai-debug__frame"} onClick={() => {
            void store.selectFrame(frame.id);
            if (frame.path) onOpenLocation(frame.path, frame.line, frame.column);
          }}>
            <span className="zorai-debug__name">{frame.name}</span>
            <span className="zorai-debug__value">{frame.path ? `${frame.path.split("/").pop()}:${frame.line}` : frame.absolutePath ? `${frame.absolutePath.split(/[\\/]/).pop()}:${frame.line}` : ""}</span>
          </button>
        ))}
      </details>
      <details className="zorai-debug__section" open>
        <summary>Breakpoints</summary>
        {Object.entries(breakpoints).flatMap(([path, list]) => list.map((breakpoint) => (
          <div key={`${path}:${breakpoint.line}`} className="zorai-debug__breakpoint">
            <input type="checkbox" checked={breakpoint.enabled} aria-label={`Enable breakpoint ${path}:${breakpoint.line}`} onChange={(event) => store.updateBreakpoint(root, path, breakpoint.line, { enabled: event.target.checked })} />
            <button type="button" className="zorai-debug__name" onClick={() => onOpenLocation(path, breakpoint.line, 1)}>{path.split("/").pop()}:{breakpoint.line}</button>
            <BreakpointField root={root} path={path} breakpoint={breakpoint} />
            <button type="button" aria-label={`Remove breakpoint ${path}:${breakpoint.line}`} onClick={() => store.updateBreakpoint(root, path, breakpoint.line, null)}>×</button>
          </div>
        )))}
        {Object.values(breakpoints).every((list) => list.length === 0) ? <p className="zorai-debug__hint">Click the gutter left of a line number, or press F9.</p> : null}
      </details>
      {store.exceptionFilters.length > 0 ? (
        <details className="zorai-debug__section">
          <summary>Exceptions</summary>
          {store.exceptionFilters.map((entry) => (
            <label key={entry.filter} className="zorai-debug__breakpoint">
              <input type="checkbox" checked={(store.enabledExceptionFilters ?? store.exceptionFilters.filter((item) => item.default).map((item) => item.filter)).includes(entry.filter)} onChange={(event) => store.setExceptionFilter(entry.filter, event.target.checked)} />
              <span className="zorai-debug__value">{entry.label}</span>
            </label>
          ))}
        </details>
      ) : null}
      {missing.length > 0 ? (
        <details className="zorai-debug__section">
          <summary>Debug adapters</summary>
          {adapters.map((adapter) => <p key={adapter.id} className="zorai-debug__hint">{adapter.available ? "✓" : "✗"} {adapter.label}{adapter.available ? "" : ` — ${adapter.hint}`}</p>)}
        </details>
      ) : null}
    </div>
  );
}
