import { useEffect, useMemo, useRef, useState, type MouseEvent as ReactMouseEvent } from "react";
import { TerminalPane } from "@/components/TerminalPane";
import { getBridge } from "@/lib/bridge";
import { encodeTerminalInput } from "@/plugins/coding-agents/bridge";
import { useWorkspaceStore } from "@/lib/workspaceStore";
import { useComposerPanelHeight } from "../threads/useComposerPanelHeight";
import {
  agentTerminalSessions,
  agentTerminalViewPaneId,
  closeThreadTerminalTab,
  openThreadShell,
  setThreadTerminalOpen,
  splitThreadTerminal,
  takeShellCommand,
  threadTerminalPaneId,
  threadTerminalTabs,
  unsplitThreadTerminal,
  useThreadTerminalStore,
  viewThreadTerminal,
  type ThreadTerminalTab,
} from "./threadTerminalStore";
import { CodeContextMenu } from "../code/CodeToolbarMenu";

const NO_IDS: string[] = [];

export function ComposerTerminalPanel({ sessionKey, cwd, workspaceId }: { sessionKey: string; cwd?: string; workspaceId?: string | null }) {
  const resize = useComposerPanelHeight(340, 160);
  return (
    <div ref={resize.panelRef} className="zorai-terminal-panel" style={{ height: resize.height }}>
      <div className="zorai-composer-panel-resize" {...resize.handleProps} />
      <TerminalSessions sessionKey={sessionKey} cwd={cwd} workspaceId={workspaceId} onHide={() => setThreadTerminalOpen(sessionKey, false)} />
    </div>
  );
}

export function TerminalSessions({ sessionKey, cwd, workspaceId, onHide, autoStart = false }: { sessionKey: string; cwd?: string; workspaceId?: string | null; onHide?: () => void; autoStart?: boolean }) {
  const workspaces = useWorkspaceStore((state) => state.workspaces);
  const shells = useThreadTerminalStore((state) => state.shells[sessionKey] ?? NO_IDS);
  const dismissed = useThreadTerminalStore((state) => state.dismissed[sessionKey] ?? NO_IDS);
  const viewingId = useThreadTerminalStore((state) => state.viewing[sessionKey] ?? null);
  const shellCwds = useThreadTerminalStore((state) => state.shellCwds);
  const shellTitles = useThreadTerminalStore((state) => state.shellTitles);
  const shellPaths = useThreadTerminalStore((state) => state.shellPaths);
  const splitPeer = useThreadTerminalStore((state) => state.splitPeers[sessionKey] ?? null);
  const [shellMenu, setShellMenu] = useState<{ x: number; y: number; shells: Array<{ name: string; path: string }> } | null>(null);
  const tabs = useMemo(
    () => threadTerminalTabs(shells, agentTerminalSessions(workspaces, workspaceId), dismissed, shellTitles),
    [shells, workspaces, workspaceId, dismissed, shellTitles],
  );
  const current = tabs.find((tab) => tab.id === viewingId) ?? tabs[0] ?? null;
  const shellCwd = current?.kind === "shell" ? shellCwds[current.id] ?? cwd : cwd;
  const shownCwd = current?.kind === "agent" ? current.cwd ?? undefined : shellCwd;
  const currentShellId = current?.kind === "shell" ? current.id : null;
  const peerTab = splitPeer && splitPeer !== current?.id ? tabs.find((tab) => tab.id === splitPeer && tab.kind === "shell") ?? null : null;
  const openShellMenu = (event: ReactMouseEvent<HTMLButtonElement>) => {
    const rect = event.currentTarget.getBoundingClientRect();
    void (getBridge()?.getAvailableShells?.() ?? Promise.resolve([])).then((available: Array<{ name: string; path: string; args?: string }>) => {
      setShellMenu({ x: rect.left, y: rect.bottom + 2, shells: available.filter((shell) => !shell.args) });
    }).catch(() => setShellMenu({ x: rect.left, y: rect.bottom + 2, shells: [] }));
  };
  const autoStartedRef = useRef(false);

  useEffect(() => {
    if (!autoStart || autoStartedRef.current) return;
    autoStartedRef.current = true;
    if (tabs.length === 0) openThreadShell(sessionKey);
  }, [autoStart, sessionKey, tabs.length]);

  useEffect(() => {
    if (!currentShellId) return;
    const command = takeShellCommand(currentShellId);
    if (!command) return;
    const paneId = threadTerminalPaneId(sessionKey, currentShellId);
    let attempts = 0;
    const send = async () => {
      const sent = await getBridge()?.sendTerminalInput?.(paneId, encodeTerminalInput(`${command}\r`)).catch(() => false);
      if (!sent && attempts++ < 80) window.setTimeout(send, 250);
    };
    window.setTimeout(send, 300);
  }, [sessionKey, currentShellId]);

  const closeTab = (tab: ThreadTerminalTab) => {
    closeThreadTerminalTab(sessionKey, tab, tabs);
    const paneId = tab.kind === "shell" ? threadTerminalPaneId(sessionKey, tab.id) : agentTerminalViewPaneId(tab.id);
    void getBridge()?.stopTerminalSession?.(paneId, tab.kind === "shell");
  };

  return (
    <>
      <div className="zorai-terminal-panel__bar">
        <div className="zorai-terminal-panel__sessions" role="tablist" aria-label="Terminal sessions">
          {tabs.map((tab) => (
            <div
              key={tab.id}
              role="tab"
              aria-selected={current?.id === tab.id}
              className={["zorai-terminal-panel__session", tab.kind === "agent" ? "zorai-terminal-panel__session--agent" : "", current?.id === tab.id ? "is-active" : ""].filter(Boolean).join(" ")}
              title={tab.kind === "agent" ? `Agent terminal ${tab.id}` : "Your shell for this thread"}
              onClick={() => viewThreadTerminal(sessionKey, tab.id)}
            >
              <span className="zorai-terminal-panel__session-label">{tab.title}</span>
              <button
                type="button"
                className="zorai-browser-panel__tab-close"
                title={tab.kind === "agent" ? "Stop watching (the agent's shell keeps running)" : "Close this shell"}
                aria-label={`Close ${tab.title}`}
                onClick={(event) => {
                  event.stopPropagation();
                  closeTab(tab);
                }}
              >
                ×
              </button>
            </div>
          ))}
          <button type="button" className="zorai-browser-panel__icon" title="New shell" aria-label="New shell" onClick={() => openThreadShell(sessionKey)}>+</button>
          <button type="button" className="zorai-browser-panel__icon" title="New shell of a specific type" aria-label="Choose shell" onClick={openShellMenu}>▾</button>
          {currentShellId ? (
            <button type="button" className={peerTab ? "zorai-browser-panel__icon is-active" : "zorai-browser-panel__icon"} title={peerTab ? "Unsplit (the other shell keeps running as a tab)" : "Split: open another shell beside this one"} aria-label={peerTab ? "Unsplit terminal" : "Split terminal"} aria-pressed={Boolean(peerTab)} onClick={() => (peerTab ? unsplitThreadTerminal(sessionKey) : splitThreadTerminal(sessionKey, currentShellId, shellCwd))}>⊟</button>
          ) : null}
        </div>
        {shownCwd ? <span className="zorai-terminal-panel__cwd" title={shownCwd}>{shownCwd}</span> : null}
        {onHide ? (
          <button
            type="button"
            className="zorai-browser-panel__icon zorai-browser-panel__hide"
            title="Hide terminal (the shells keep running)"
            aria-label="Hide terminal"
            onClick={onHide}
          >
            ×
          </button>
        ) : null}
      </div>
      {current?.kind === "agent" ? <div className="zorai-terminal-panel__note">Live view of the agent&apos;s terminal. What you type goes to the agent&apos;s shell.</div> : null}
      <div className="zorai-terminal-panel__body">
        {current?.kind === "agent" ? (
          <TerminalPane key={current.id} paneId={agentTerminalViewPaneId(current.id)} sessionId={current.id} hideHeader hideSharedCursor />
        ) : current && peerTab ? (
          <div className="zorai-terminal-panel__split">
            <TerminalPane key={current.id} paneId={threadTerminalPaneId(sessionKey, current.id)} cwd={shellCwd} shell={shellPaths[current.id]} hideHeader hideSharedCursor />
            <TerminalPane key={peerTab.id} paneId={threadTerminalPaneId(sessionKey, peerTab.id)} cwd={shellCwds[peerTab.id] ?? cwd} shell={shellPaths[peerTab.id]} hideHeader hideSharedCursor />
          </div>
        ) : current ? (
          <TerminalPane key={current.id} paneId={threadTerminalPaneId(sessionKey, current.id)} cwd={shellCwd} shell={shellPaths[current.id]} hideHeader hideSharedCursor />
        ) : (
          <button type="button" className="zorai-terminal-panel__empty" onClick={() => openThreadShell(sessionKey)}>Open a new shell</button>
        )}
      </div>
      {shellMenu ? (
        <CodeContextMenu
          x={shellMenu.x}
          y={shellMenu.y}
          label="New shell"
          items={shellMenu.shells.length > 0
            ? shellMenu.shells.map((shell) => ({ key: shell.path, label: `${shell.name} — ${shell.path}`, onSelect: () => openThreadShell(sessionKey, undefined, { shell: shell.path, title: shell.name }) }))
            : [{ key: "none", label: "No other shells were found", disabled: true }]}
          onClose={() => setShellMenu(null)}
        />
      ) : null}
    </>
  );
}
