import { describe, expect, it } from "vitest";
import type { Workspace } from "./types";
import {
  AGENT_WORKSPACE_IDLE_TIMEOUT_MS,
  selectIdleAgentWorkspaceIds,
} from "./agentWorkspaceReclaim";

function workspace(opts: {
  id: string;
  name: string;
  agentOwned?: boolean;
  lastActivityAt?: number;
  status?: "running" | "idle" | "needs_approval";
  paneId?: string;
}): Workspace {
  const paneId = opts.paneId ?? `${opts.id}-pane`;
  const lastActivityAt = opts.lastActivityAt ?? 1_000;
  return {
    id: opts.id,
    name: opts.name,
    icon: "terminal",
    accentColor: "#fff",
    cwd: "/tmp",
    gitBranch: null,
    gitDirty: false,
    listeningPorts: [],
    unreadCount: 0,
    activeSurfaceId: "surface",
    createdAt: 1,
    agentOwned: opts.agentOwned,
    lastActivityAt,
    surfaces: [
      {
        id: "surface",
        workspaceId: opts.id,
        name: "Agent Workspace",
        icon: "terminal",
        layoutMode: "canvas",
        layout: { type: "leaf", id: paneId },
        paneNames: {},
        paneIcons: {},
        activePaneId: paneId,
        canvasState: { panX: 0, panY: 0, zoomLevel: 1, previousView: null },
        canvasPanels: [
          {
            id: `panel-${paneId}`,
            paneId,
            panelType: "terminal",
            title: "Coordinator",
            icon: "terminal",
            x: 0,
            y: 0,
            width: 400,
            height: 300,
            status: opts.status ?? "running",
            sessionId: "sess",
            url: null,
            cwd: "/tmp",
            userRenamed: false,
            lastActivityAt,
          },
        ],
        createdAt: 1,
      },
    ],
  };
}

describe("selectIdleAgentWorkspaceIds", () => {
  const now = 1_000 + AGENT_WORKSPACE_IDLE_TIMEOUT_MS;

  it("closes agent workspaces that have been idle past the timeout", () => {
    const ids = selectIdleAgentWorkspaceIds([
      workspace({ id: "stale", name: "Agent - training", lastActivityAt: 1_000 }),
      workspace({ id: "fresh", name: "Agent - training", lastActivityAt: now - 1_000 }),
      workspace({ id: "operator", name: "Default", lastActivityAt: 1_000 }),
    ], now);

    expect(ids).toEqual(["stale"]);
  });

  it("keeps an idle agent workspace while a command or approval is in progress", () => {
    const ids = selectIdleAgentWorkspaceIds([
      workspace({ id: "busy", name: "Agent - training", paneId: "pane-busy" }),
      workspace({ id: "approval", name: "Agent - training", status: "needs_approval" }),
      workspace({ id: "streaming", name: "Agent - training" }),
    ], now, {
      busyPaneIds: new Set(["pane-busy"]),
      streamingWorkspaceIds: new Set(["streaming"]),
    });

    expect(ids).toEqual([]);
  });

  it("treats an explicitly owned workspace as reclaimable even after it is renamed", () => {
    const ids = selectIdleAgentWorkspaceIds([
      workspace({ id: "owned", name: "Renamed run", agentOwned: true, lastActivityAt: 1_000 }),
    ], now);

    expect(ids).toEqual(["owned"]);
  });
});
