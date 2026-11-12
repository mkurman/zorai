import { useEffect, useRef, useState } from "react";
import { getBridge } from "@/lib/bridge";
import { trackPointerDrag } from "@/lib/pointerDrag";
import { TerminalSessions } from "../terminal/ComposerTerminalPanel";
import { clampCodePanelHeight, useCodeBottomPanelStore, type CodeBottomPanelTab } from "./codeBottomPanelStore";

type GitOutputEntry = { cwd: string; command: string; ok: boolean; output: string; at: number };

const TABS: Array<{ id: CodeBottomPanelTab; label: string }> = [
  { id: "output", label: "Output" },
  { id: "terminal", label: "Terminal" },
];

export function CodeBottomPanel({ terminalSessionKey, cwd }: {
  terminalSessionKey: string | null;
  cwd?: string;
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
          </button>
        ))}
        <button type="button" className="zorai-code-panel__close" title="Hide panel" aria-label="Hide panel" onClick={close}>×</button>
      </div>
      <div className="zorai-code-panel__body">
        {tab === "output" ? <GitOutputList /> : null}
        {tab === "terminal" ? (
          terminalSessionKey ? (
            <div className="zorai-code-panel__terminal">
              <TerminalSessions sessionKey={terminalSessionKey} cwd={cwd} workspaceId={null} />
            </div>
          ) : <p className="zorai-code-panel__empty">Open a folder to start a terminal in it.</p>
        ) : null}
      </div>
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
