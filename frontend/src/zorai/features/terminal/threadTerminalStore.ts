import { create } from "zustand";
import { allLeafIds, findLeaf } from "@/lib/bspTree";
import type { Workspace } from "@/lib/types";

type ThreadTerminalState = {
  open: Record<string, boolean>;
  viewing: Record<string, string | null>;
  shells: Record<string, string[]>;
  dismissed: Record<string, string[]>;
  shellCwds: Record<string, string>;
};

export type ThreadTerminalTab =
  | { kind: "shell"; id: string; title: string }
  | { kind: "agent"; id: string; title: string; cwd: string | null };

export type AgentTerminalSession = {
  sessionId: string;
  title: string;
  cwd: string | null;
};

export const useThreadTerminalStore = create<ThreadTerminalState>(() => ({ open: {}, viewing: {}, shells: {}, dismissed: {}, shellCwds: {} }));

let shellSerial = 0;

export function setThreadTerminalOpen(sessionKey: string, open: boolean) {
  useThreadTerminalStore.setState((state) => ({ open: { ...state.open, [sessionKey]: open } }));
  if (open && (useThreadTerminalStore.getState().shells[sessionKey] ?? []).length === 0) openThreadShell(sessionKey);
}

export function viewThreadTerminal(sessionKey: string, tabId: string | null) {
  useThreadTerminalStore.setState((state) => ({ viewing: { ...state.viewing, [sessionKey]: tabId } }));
}

export function openThreadShell(sessionKey: string, cwd?: string): string {
  shellSerial += 1;
  const id = `shell-${Date.now().toString(36)}-${shellSerial}`;
  useThreadTerminalStore.setState((state) => ({
    shells: { ...state.shells, [sessionKey]: [...(state.shells[sessionKey] ?? []), id] },
    viewing: { ...state.viewing, [sessionKey]: id },
    shellCwds: cwd ? { ...state.shellCwds, [id]: cwd } : state.shellCwds,
  }));
  return id;
}

export function threadTerminalTabs(shells: string[], agentSessions: AgentTerminalSession[], dismissed: string[]): ThreadTerminalTab[] {
  return [
    ...shells.map((id, index) => ({ kind: "shell" as const, id, title: index === 0 ? "My shell" : `My shell ${index + 1}` })),
    ...agentSessions
      .filter((session) => !dismissed.includes(session.sessionId))
      .map((session) => ({ kind: "agent" as const, id: session.sessionId, title: session.title, cwd: session.cwd })),
  ];
}

export function closeThreadTerminalTab(sessionKey: string, tab: ThreadTerminalTab, tabs: ThreadTerminalTab[]): void {
  const index = tabs.findIndex((candidate) => candidate.id === tab.id);
  const remaining = tabs.filter((candidate) => candidate.id !== tab.id);
  const neighbour = remaining[Math.min(Math.max(index, 0), remaining.length - 1)]?.id ?? null;
  useThreadTerminalStore.setState((state) => ({
    shells: tab.kind === "shell" ? { ...state.shells, [sessionKey]: (state.shells[sessionKey] ?? []).filter((id) => id !== tab.id) } : state.shells,
    dismissed: tab.kind === "agent" ? { ...state.dismissed, [sessionKey]: [...(state.dismissed[sessionKey] ?? []), tab.id] } : state.dismissed,
    viewing: state.viewing[sessionKey] === tab.id ? { ...state.viewing, [sessionKey]: neighbour } : state.viewing,
  }));
}

export function threadTerminalPaneId(sessionKey: string, shellId: string): string {
  return `thread-terminal:${sessionKey}:${shellId}`;
}

export function agentTerminalViewPaneId(agentSessionId: string): string {
  return `thread-terminal-view:${agentSessionId}`;
}

export function agentTerminalSessions(workspaces: Workspace[], workspaceId: string | null | undefined): AgentTerminalSession[] {
  const workspace = workspaceId ? workspaces.find((candidate) => candidate.id === workspaceId) : undefined;
  if (!workspace) return [];
  const sessions = new Map<string, AgentTerminalSession>();
  for (const surface of workspace.surfaces) {
    if (surface.layoutMode === "canvas") {
      for (const panel of surface.canvasPanels) {
        if (panel.panelType === "terminal" && panel.sessionId && !sessions.has(panel.sessionId)) {
          sessions.set(panel.sessionId, { sessionId: panel.sessionId, title: panel.title || surface.paneNames[panel.paneId] || "Terminal", cwd: panel.cwd });
        }
      }
    } else {
      for (const paneId of allLeafIds(surface.layout)) {
        const sessionId = findLeaf(surface.layout, paneId)?.sessionId;
        if (sessionId && !sessions.has(sessionId)) {
          sessions.set(sessionId, { sessionId, title: surface.paneNames[paneId] || "Terminal", cwd: null });
        }
      }
    }
  }
  return [...sessions.values()];
}
