import { normalizeIconId } from "../iconRegistry";
import { allLeafIds } from "../bspTree";
import type { WorkspaceState } from "./types";
import type { WorkspaceStoreContext } from "./store-context";
import {
  captureTranscriptForPane,
  createDefaultWorkspace,
  stopPaneSessions,
} from "./helpers";
import { createDefaultWorkspaceBrowserState } from "./browser";

export function createWorkspaceActions(
  ctx: WorkspaceStoreContext,
): Pick<
  WorkspaceState,
  | "createWorkspace"
  | "renameWorkspace"
  | "setWorkspaceIcon"
  | "closeWorkspace"
  | "setActiveWorkspace"
  | "switchWorkspaceByIndex"
  | "nextWorkspace"
  | "prevWorkspace"
  | "updateWorkspaceGit"
  | "updateWorkspaceCwd"
  | "updateWorkspacePorts"
  | "clearWorkspaceUnread"
> {
  return {
    createWorkspace: (name, opts) => {
      const safeName = typeof name === "string" ? name.trim() : "";
      const layoutMode = opts?.layoutMode ?? "bsp";
      const makeActive = opts?.makeActive ?? true;
      const ws = createDefaultWorkspace(safeName || undefined, layoutMode);
      if (opts?.agentOwned) ws.agentOwned = true;
      ctx.set((state) => {
        const workspaceBrowserState = {
          ...state.workspaceBrowserState,
          [ws.id]: createDefaultWorkspaceBrowserState(),
        };
        const shouldActivate = makeActive || !state.activeWorkspaceId;
        if (!shouldActivate) {
          return {
            workspaces: [...state.workspaces, ws],
            workspaceBrowserState,
          };
        }
        return {
          workspaces: [...state.workspaces, ws],
          activeWorkspaceId: ws.id,
          ...ctx.activateWorkspaceBrowserState(workspaceBrowserState, ws.id),
        };
      });
      return ws.id;
    },

    renameWorkspace: (id, name) => {
      const nextName = typeof name === "string" ? name.trim() : "";
      if (!nextName) return;
      ctx.set((state) => ({
        workspaces: state.workspaces.map((ws) => (ws.id === id ? { ...ws, name: nextName } : ws)),
      }));
    },

    setWorkspaceIcon: (id, icon) => {
      const nextIcon = normalizeIconId(icon);
      ctx.set((state) => ({
        workspaces: state.workspaces.map((ws) => (ws.id === id ? { ...ws, icon: nextIcon } : ws)),
      }));
    },

    closeWorkspace: (id) => {
      const { workspaces, activeWorkspaceId } = ctx.get();
      const removedWorkspace = workspaces.find((workspace) => workspace.id === id);
      if (removedWorkspace) {
        const paneIds = removedWorkspace.surfaces.flatMap((surface) => {
          const ids = allLeafIds(surface.layout);
          for (const paneId of ids) {
            captureTranscriptForPane({
              paneId,
              workspaceId: removedWorkspace.id,
              surfaceId: surface.id,
              cwd: removedWorkspace.cwd,
              reason: "workspace-close",
            });
          }
          return ids;
        });
        stopPaneSessions(paneIds);
      }

      const remaining = workspaces.filter((workspace) => workspace.id !== id);
      if (remaining.length === 0) {
        const ws = createDefaultWorkspace();
        const workspaceBrowserState = {
          [ws.id]: createDefaultWorkspaceBrowserState(),
        };
        ctx.set({
          workspaces: [ws],
          activeWorkspaceId: ws.id,
          ...ctx.activateWorkspaceBrowserState(workspaceBrowserState, ws.id),
        });
        return;
      }

      ctx.set((state) => {
        const nextWorkspaceBrowserState = { ...state.workspaceBrowserState };
        delete nextWorkspaceBrowserState[id];
        const nextActiveWorkspaceId = activeWorkspaceId === id ? remaining[0].id : activeWorkspaceId;
        return {
          workspaces: remaining,
          activeWorkspaceId: nextActiveWorkspaceId,
          ...ctx.activateWorkspaceBrowserState(nextWorkspaceBrowserState, nextActiveWorkspaceId),
        };
      });
    },

    setActiveWorkspace: (id) => ctx.set((state) => ({
      activeWorkspaceId: id,
      zoomedPaneId: null,
      workspaces: state.workspaces.map((workspace) => (
        workspace.id === id ? { ...workspace, lastActivityAt: Date.now() } : workspace
      )),
      ...ctx.activateWorkspaceBrowserState(state.workspaceBrowserState, id),
    })),

    switchWorkspaceByIndex: (index) => {
      const { workspaces } = ctx.get();
      const target = index === 9 ? workspaces[workspaces.length - 1] : workspaces[index - 1];
      if (!target) return;
      ctx.set((state) => ({
        activeWorkspaceId: target.id,
        zoomedPaneId: null,
        ...ctx.activateWorkspaceBrowserState(state.workspaceBrowserState, target.id),
      }));
    },

    nextWorkspace: () => {
      const { workspaces, activeWorkspaceId } = ctx.get();
      const idx = workspaces.findIndex((workspace) => workspace.id === activeWorkspaceId);
      const next = workspaces[(idx + 1) % workspaces.length];
      if (!next) return;
      ctx.set((state) => ({
        activeWorkspaceId: next.id,
        zoomedPaneId: null,
        ...ctx.activateWorkspaceBrowserState(state.workspaceBrowserState, next.id),
      }));
    },

    prevWorkspace: () => {
      const { workspaces, activeWorkspaceId } = ctx.get();
      const idx = workspaces.findIndex((workspace) => workspace.id === activeWorkspaceId);
      const prev = workspaces[(idx - 1 + workspaces.length) % workspaces.length];
      if (!prev) return;
      ctx.set((state) => ({
        activeWorkspaceId: prev.id,
        zoomedPaneId: null,
        ...ctx.activateWorkspaceBrowserState(state.workspaceBrowserState, prev.id),
      }));
    },

    updateWorkspaceGit: (id, branch, dirty) => {
      ctx.set((state) => ({
        workspaces: state.workspaces.map((ws) => (
          ws.id === id ? { ...ws, gitBranch: branch, gitDirty: dirty } : ws
        )),
      }));
    },

    updateWorkspaceCwd: (id, cwd) => {
      ctx.set((state) => ({
        workspaces: state.workspaces.map((ws) => (ws.id === id ? { ...ws, cwd } : ws)),
      }));
    },

    updateWorkspacePorts: (id, ports) => {
      ctx.set((state) => ({
        workspaces: state.workspaces.map((ws) => (ws.id === id ? { ...ws, listeningPorts: ports } : ws)),
      }));
    },

    clearWorkspaceUnread: (id) => {
      ctx.set((state) => ({
        workspaces: state.workspaces.map((ws) => (ws.id === id ? { ...ws, unreadCount: 0 } : ws)),
      }));
    },
  };
}
