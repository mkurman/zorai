import { useEffect, useRef, useState, type ReactNode } from "react";
import { getBridge } from "@/lib/bridge";
import { trackPointerDrag } from "@/lib/pointerDrag";
import { TerminalSessions } from "../terminal/ComposerTerminalPanel";
import { DebugConsole } from "./debug/DebugConsole";
import { CodeTasksPanel } from "./CodeTasksPanel";
import { clampCodePanelHeight, useCodeBottomPanelStore, type CodeBottomPanelTab } from "./codeBottomPanelStore";

type GitOutputEntry = { cwd: string; command: string; ok: boolean; output: string; at: number };

const TABS: Array<{ id: CodeBottomPanelTab; label: string }> = [
  { id: "problems", label: "Problems" },
  { id: "output", label: "Output" },
  { id: "terminal", label: "Terminal" },
  { id: "tasks", label: "Tasks" },
  { id: "debug", label: "Debug Console" },
];

export function CodeBottomPanel({ terminalSessionKey, cwd, problems, problemCount = 0 }: {
  terminalSessionKey: string | null;
  cwd?: string;
  problems?: ReactNode;
  problemCount?: number;
}) {
  const { tab, height, show, close, setHeight } = useCodeBottomPanelStore();
  const panelRef = useRef<HTMLDivElement | null>(null);

  return (
    <div ref={panelRef} className="zorai-code-panel" style={{ height }}>
      <div
        className="zorai-code-panel__resize"
        role="separator"
        aria-orientation="horizontal"
        aria-label="Resize panel"
        onPointerDown={(event) => {
          const area = panelRef.current?.parentElement;
          if (event.button !== 0) return;
          event.preventDefault();
          const startY = event.clientY;
          const startHeight = height;
          const available = area?.getBoundingClientRect().height ?? window.innerHeight;
          trackPointerDrag((move) => setHeight(clampCodePanelHeight(startHeight + startY - move.clientY, available)));
        }}
      />
      <div className="zorai-code-panel__bar" role="tablist" aria-label="Panel">
        {TABS.map((item) => (
          <button key={item.id} type="button" role="tab" aria-selected={tab === item.id} className={tab === item.id ? "is-active" : undefined} onClick={() => show(item.id)}>
            {item.label}
            {item.id === "problems" && problemCount > 0 ? <span className="zorai-code-panel__badge">{problemCount}</span> : null}
          </button>
        ))}
        <button type="button" className="zorai-code-panel__close" title="Hide panel" aria-label="Hide panel" onClick={close}>×</button>
      </div>
      <div className="zorai-code-panel__body">
        {tab === "problems" ? problems ?? <p className="zorai-code-panel__empty">Open a folder to see its problems.</p> : null}
        {tab === "output" ? <OutputChannels /> : null}
        {tab === "debug" ? <DebugConsole /> : null}
        {tab === "tasks" ? <CodeTasksPanel root={cwd ?? null} /> : null}
        {tab === "terminal" ? (
          terminalSessionKey ? (
            <div className="zorai-code-panel__terminal">
              <TerminalSessions sessionKey={terminalSessionKey} cwd={cwd} workspaceId={null} autoStart />
            </div>
          ) : <p className="zorai-code-panel__empty">Open a folder to start a terminal in it.</p>
        ) : null}
      </div>
    </div>
  );
}

type LspLogEntry = { at: number; server: string; root: string; level: string; text: string };

function OutputChannels() {
  const [channel, setChannel] = useState<"git" | "lsp">("git");
  return (
    <div className="zorai-code-output-channels">
      <div className="zorai-code-output-channels__bar">
        <select value={channel} aria-label="Output channel" onChange={(event) => setChannel(event.target.value as "git" | "lsp")}>
          <option value="git">Git</option>
          <option value="lsp">Language Servers</option>
        </select>
      </div>
      {channel === "git" ? <GitOutputList /> : <LspOutputList />}
    </div>
  );
}

function LspOutputList() {
  const [entries, setEntries] = useState<LspLogEntry[]>([]);
  const endRef = useRef<HTMLDivElement | null>(null);
  useEffect(() => {
    let cancelled = false;
    const load = () => void getBridge()?.workspaceLspLog?.().then((next) => {
      if (!cancelled) setEntries((current) => (current.length === next.length && current[current.length - 1]?.at === next[next.length - 1]?.at ? current : next));
    });
    load();
    const timer = window.setInterval(load, 2000);
    return () => { cancelled = true; window.clearInterval(timer); };
  }, []);
  useEffect(() => { endRef.current?.scrollIntoView({ block: "end" }); }, [entries]);
  if (entries.length === 0) return <p className="zorai-code-panel__empty">Language server start-up, messages and errors show up here once a server runs.</p>;
  return (
    <div className="zorai-code-panel__output zorai-code-lsp-log">
      {entries.map((entry, index) => (
        <div key={`${entry.at}:${index}`} className={entry.level === "error" ? "is-error" : entry.level === "warning" ? "is-warning" : undefined}>
          <span className="zorai-code-panel__prompt">[{new Date(entry.at).toLocaleTimeString()}] {entry.server}</span> {entry.text}
        </div>
      ))}
      <div ref={endRef} />
    </div>
  );
}

function GitOutputList() {
  const [entries, setEntries] = useState<GitOutputEntry[]>([]);
  useEffect(() => {
    let cancelled = false;
    const load = () => void getBridge()?.workspaceGitOutput?.().then((next) => {
      if (!cancelled) setEntries((current) => (current.length === next.length && current[current.length - 1]?.at === next[next.length - 1]?.at ? current : next));
    });
    load();
    const timer = window.setInterval(load, 2000);
    return () => { cancelled = true; window.clearInterval(timer); };
  }, []);
  if (entries.length === 0) return <p className="zorai-code-panel__empty">Git commands run from Source Control show their output here.</p>;
  return (
    <div className="zorai-code-panel__output">
      {entries.map((entry, index) => (
        <div key={`${entry.at}:${index}`} className={entry.ok ? undefined : "is-error"}>
          <span className="zorai-code-panel__prompt">[{new Date(entry.at).toLocaleTimeString()}] {entry.command}</span>
          {entry.output ? <pre>{entry.output}</pre> : null}
        </div>
      ))}
    </div>
  );
}
