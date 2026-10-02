import type { Workspace } from "./types";
import { getBridge } from "./bridge";
import { allLeafIds, findLeaf } from "./bspTree";
import { useAgentStore } from "./agentStore";
import { useWorkspaceStore } from "./workspaceStore";

export const AGENT_WORKSPACE_IDLE_TIMEOUT_MS = 10 * 60 * 1000;
export const AGENT_WORKSPACE_NAME_PREFIX = "Agent - ";
const NEW_AGENT_WORKSPACE_GRACE_MS = 15_000;

const liveCommandPaneIds = new Set<string>();

export function notePaneCommandActivity(paneId: string, active: boolean): void {
  if (!paneId) return;
  if (active) liveCommandPaneIds.add(paneId);
  else liveCommandPaneIds.delete(paneId);
}

export function isReclaimableAgentWorkspace(workspace: { name: string; agentOwned?: boolean }): boolean {
  return workspace.agentOwned === true || workspace.name.startsWith(AGENT_WORKSPACE_NAME_PREFIX);
}

export function workspaceLastActivityAt(workspace: Pick<Workspace, "lastActivityAt" | "surfaces">): number {
  let latest = workspace.lastActivityAt ?? 0;
  for (const surface of workspace.surfaces) {
    for (const panel of surface.canvasPanels) {
      if (panel.lastActivityAt > latest) latest = panel.lastActivityAt;
    }
  }
  return latest;
}

function workspacePaneIds(workspace: Workspace): string[] {
  const paneIds: string[] = [];
  for (const surface of workspace.surfaces) {
    paneIds.push(...allLeafIds(surface.layout));
  }
  return paneIds;
}

export function workspaceHasPendingApproval(workspace: Workspace): boolean {
  return workspace.surfaces.some((surface) => (
    surface.canvasPanels.some((panel) => panel.status === "needs_approval")
  ));
}

export function workspaceHasTerminalSession(workspace: Workspace): boolean {
  for (const surface of workspace.surfaces) {
    for (const paneId of allLeafIds(surface.layout)) {
      const panelSessionId = surface.canvasPanels.find((panel) => panel.paneId === paneId)?.sessionId ?? null;
      const leafSessionId = findLeaf(surface.layout, paneId)?.sessionId ?? null;
      if (panelSessionId || leafSessionId) return true;
    }
  }
  return false;
}

export function selectIdleAgentWorkspaceIds(
  workspaces: Workspace[],
  now: number,
  opts?: {
    timeoutMs?: number;
    busyPaneIds?: ReadonlySet<string>;
    streamingWorkspaceIds?: ReadonlySet<string>;
  },
): string[] {
  const timeoutMs = opts?.timeoutMs ?? AGENT_WORKSPACE_IDLE_TIMEOUT_MS;
  const busyPaneIds = opts?.busyPaneIds ?? liveCommandPaneIds;
  const streamingWorkspaceIds = opts?.streamingWorkspaceIds;
  return workspaces.filter((workspace) => {
    if (!isReclaimableAgentWorkspace(workspace)) return false;
    if (streamingWorkspaceIds?.has(workspace.id)) return false;
    if (workspaceHasPendingApproval(workspace)) return false;
    if (workspacePaneIds(workspace).some((paneId) => busyPaneIds.has(paneId))) return false;
    const lastActivityAt = workspaceLastActivityAt(workspace);
    if (lastActivityAt <= 0) return true;
    return now - lastActivityAt >= timeoutMs;
  }).map((workspace) => workspace.id);
}

function streamingWorkspaceIds(): Set<string> {
  const ids = new Set<string>();
  const state = useAgentStore.getState();
  for (const thread of state.threads) {
    if (!thread.workspaceId) continue;
    const messages = state.messages[thread.id] ?? [];
    if (messages.some((message) => message.isStreaming)) ids.add(thread.workspaceId);
  }
  return ids;
}

export async function busyTerminalPaneIds(): Promise<Set<string>> {
  const busy = new Set(liveCommandPaneIds);
  const listed = await getBridge()?.listBusyTerminalPanes?.();
  if (Array.isArray(listed)) {
    for (const paneId of listed) {
      if (typeof paneId === "string" && paneId) busy.add(paneId);
    }
  }
  return busy;
}

export async function reclaimIdleAgentWorkspaces(now = Date.now()): Promise<string[]> {
  const store = useWorkspaceStore.getState();
  const ids = selectIdleAgentWorkspaceIds(store.workspaces, now, {
    busyPaneIds: await busyTerminalPaneIds(),
    streamingWorkspaceIds: streamingWorkspaceIds(),
  });
  for (const workspaceId of ids) {
    const current = useWorkspaceStore.getState();
    if (current.workspaces.some((workspace) => workspace.id === workspaceId)) {
      current.closeWorkspace(workspaceId);
    }
  }
  return ids;
}

export function closeAgentWorkspacesWithoutSessions(now = Date.now()): string[] {
  const closed: string[] = [];
  const workspaces = useWorkspaceStore.getState().workspaces.filter((workspace) => (
    isReclaimableAgentWorkspace(workspace)
    && !workspaceHasTerminalSession(workspace)
    && now - workspace.createdAt >= NEW_AGENT_WORKSPACE_GRACE_MS
  ));
  for (const workspace of workspaces) {
    const current = useWorkspaceStore.getState();
    if (!current.workspaces.some((entry) => entry.id === workspace.id)) continue;
    current.closeWorkspace(workspace.id);
    closed.push(workspace.id);
  }
  return closed;
}

export function startAgentWorkspaceReclaim(intervalMs = 30_000): () => void {
  void reclaimIdleAgentWorkspaces();
  const timer = window.setInterval(() => {
    void reclaimIdleAgentWorkspaces();
  }, intervalMs);
  return () => window.clearInterval(timer);
}
