import { getBridge } from "../bridge";
import type { BspTree } from "../bspTree";
import {
  allLeafIds,
  createLeaf,
  findLeaf,
} from "../bspTree";
import type {
  PaneId,
  PersistedSession,
  Surface,
  SurfaceLayoutMode,
  Workspace,
  WorkspaceId,
} from "../types";
import { useSettingsStore } from "../settingsStore";
import { getTerminalSnapshot } from "../terminalRegistry";
import { useTranscriptStore } from "../transcriptStore";
import { buildCanvasPanel, createDefaultCanvasState } from "./canvas";
import { buildPaneIcons, buildPaneNames } from "./pane-metadata";

let workspaceIdCounter = 0;
let surfaceIdCounter = 0;

const ACCENT_COLORS = [
  "#89b4fa",
  "#a6e3a1",
  "#f9e2af",
  "#f38ba8",
  "#f5c2e7",
  "#94e2d5",
  "#fab387",
  "#cba6f7",
];

export function createWorkspaceId(): WorkspaceId {
  return `ws_${++workspaceIdCounter}`;
}

export function createSurfaceId(): string {
  return `sf_${++surfaceIdCounter}`;
}

export function pickAccent(index: number): string {
  return ACCENT_COLORS[index % ACCENT_COLORS.length];
}

export function syncWorkspaceAndSurfaceCounters(workspaces: Workspace[]) {
  let maxWorkspaceId = workspaceIdCounter;
  let maxSurfaceId = surfaceIdCounter;

  for (const workspace of workspaces) {
    const workspaceMatch = /^ws_(\d+)$/.exec(workspace.id);
    if (workspaceMatch) {
      maxWorkspaceId = Math.max(maxWorkspaceId, Number(workspaceMatch[1]));
    }

    for (const surface of workspace.surfaces) {
      const surfaceMatch = /^sf_(\d+)$/.exec(surface.id);
      if (surfaceMatch) {
        maxSurfaceId = Math.max(maxSurfaceId, Number(surfaceMatch[1]));
      }
    }
  }

  workspaceIdCounter = maxWorkspaceId;
  surfaceIdCounter = maxSurfaceId;
}

export function shortenHomePath(cwd: string): string {
  return cwd.replace(/^\/(?:home|Users)\/[^/]+/, "~");
}

export function createDefaultSurface(
  workspaceId: WorkspaceId,
  layoutMode: SurfaceLayoutMode = "bsp",
): Surface {
  const leaf = createLeaf();
  const paneNames = buildPaneNames([leaf.id], { [leaf.id]: "Pane 1" });
  const paneIcons = buildPaneIcons([leaf.id], { [leaf.id]: "terminal" });
  const panel = buildCanvasPanel({
    paneId: leaf.id,
    paneName: paneNames[leaf.id],
    persisted: { icon: paneIcons[leaf.id] },
    index: 0,
    status: "running",
  });

  return {
    id: createSurfaceId(),
    workspaceId,
    name: layoutMode === "canvas" ? "Infinite Canvas" : "Terminal",
    icon: layoutMode === "canvas" ? "canvas" : "terminal",
    layoutMode,
    layout: leaf,
    paneNames,
    paneIcons,
    activePaneId: leaf.id,
    canvasState: createDefaultCanvasState(),
    canvasPanels: layoutMode === "canvas" ? [panel] : [],
    createdAt: Date.now(),
  };
}

export function createDefaultWorkspace(
  name?: string,
  layoutMode: SurfaceLayoutMode = "bsp",
): Workspace {
  const id = createWorkspaceId();
  const surface = createDefaultSurface(id, layoutMode);
  return {
    id,
    name: name ?? `Workspace ${workspaceIdCounter}`,
    icon: "terminal",
    accentColor: pickAccent(workspaceIdCounter - 1),
    cwd: "",
    gitBranch: null,
    gitDirty: false,
    listeningPorts: [],
    unreadCount: 0,
    surfaces: [surface],
    activeSurfaceId: surface.id,
    createdAt: Date.now(),
    agentOwned: false,
    lastActivityAt: Date.now(),
  };
}

export function stopPaneSessions(paneIds: string[], killSessions: boolean = true) {
  const zorai = getBridge();
  if (!zorai?.stopTerminalSession) return;

  for (const paneId of paneIds) {
    void zorai.stopTerminalSession(paneId, killSessions);
  }
}

export function resolvePaneSessionId(workspaces: Workspace[], paneId: PaneId): string | null {
  for (const workspace of workspaces) {
    for (const surface of workspace.surfaces) {
      const leaf = findLeaf(surface.layout, paneId);
      if (!leaf) continue;
      const panelSessionId = surface.canvasPanels.find((panel) => panel.paneId === paneId)?.sessionId ?? null;
      return panelSessionId ?? leaf.sessionId ?? null;
    }
  }
  return null;
}

export function findPaneIdsForSession(workspaces: Workspace[], sessionId: string): PaneId[] {
  const paneIds: PaneId[] = [];
  for (const workspace of workspaces) {
    for (const surface of workspace.surfaces) {
      for (const paneId of allLeafIds(surface.layout)) {
        const leaf = findLeaf(surface.layout, paneId);
        if (!leaf) continue;
        const panelSessionId = surface.canvasPanels.find((panel) => panel.paneId === paneId)?.sessionId ?? null;
        if ((panelSessionId ?? leaf.sessionId ?? null) === sessionId) {
          paneIds.push(paneId);
        }
      }
    }
  }
  return paneIds;
}

export function hasAnotherPaneForSession(
  workspaces: Workspace[],
  sessionId: string,
  excludingPaneId: PaneId,
): boolean {
  for (const workspace of workspaces) {
    for (const surface of workspace.surfaces) {
      for (const paneId of allLeafIds(surface.layout)) {
        if (paneId === excludingPaneId) continue;
        const leaf = findLeaf(surface.layout, paneId);
        if (!leaf) continue;
        const panelSessionId = surface.canvasPanels.find((panel) => panel.paneId === paneId)?.sessionId ?? null;
        if ((panelSessionId ?? leaf.sessionId ?? null) === sessionId) {
          return true;
        }
      }
    }
  }
  return false;
}

export function captureTranscriptForPane(opts: {
  paneId: string;
  workspaceId?: string | null;
  surfaceId?: string | null;
  cwd?: string | null;
  reason: "pane-close" | "surface-close" | "workspace-close";
}) {
  const settings = useSettingsStore.getState().settings;
  if (!settings.captureTranscriptsOnClose) {
    return;
  }

  const content = getTerminalSnapshot(opts.paneId).trim();
  if (!content) {
    return;
  }

  useTranscriptStore.getState().addTranscript({
    content,
    reason: opts.reason,
    workspaceId: opts.workspaceId ?? null,
    surfaceId: opts.surfaceId ?? null,
    paneId: opts.paneId,
    cwd: opts.cwd ?? null,
  });
}

type PaneSnapshot = {
  id: string;
  sessionId?: string;
};

export function collectPaneSnapshots(tree: BspTree, activePaneId: PaneId | null): PaneSnapshot[] {
  const paneIds = allLeafIds(tree);
  const orderedPaneIds = activePaneId && paneIds.includes(activePaneId)
    ? [activePaneId, ...paneIds.filter((paneId) => paneId !== activePaneId)]
    : paneIds;

  const panes: PaneSnapshot[] = [];

  for (const paneId of orderedPaneIds) {
    const leaf = findLeaf(tree, paneId);
    if (!leaf) {
      continue;
    }

    panes.push({
      id: leaf.id,
      sessionId: leaf.sessionId,
    });
  }

  return panes;
}

export function applyPaneSnapshots(tree: BspTree, panes: PaneSnapshot[]): BspTree {
  let index = 0;

  const visit = (node: BspTree): BspTree => {
    if (node.type === "leaf") {
      const preservedPane = panes[index];
      index += 1;

      if (!preservedPane) {
        return node;
      }

      return {
        type: "leaf",
        id: preservedPane.id,
        sessionId: preservedPane.sessionId,
      };
    }

    return {
      ...node,
      first: visit(node.first),
      second: visit(node.second),
    };
  };

  return visit(tree);
}

export type HydratedSession = PersistedSession;
