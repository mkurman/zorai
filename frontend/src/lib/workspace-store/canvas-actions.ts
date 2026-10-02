import {
  allLeafIds,
  normalizeBspTree,
  setSessionId,
  splitPane,
  syncPaneIdCounter,
} from "../bspTree";
import type { PersistedCanvasPanel, Surface, SurfaceLayoutMode } from "../types";
import { normalizeIconId } from "../iconRegistry";
import type { WorkspaceState } from "./types";
import type { WorkspaceStoreContext } from "./store-context";
import {
  CANVAS_MAX_ZOOM,
  CANVAS_MIN_ZOOM,
  buildCanvasPanel,
  findCanvasPlacement,
  normalizeCanvasPanels,
  sanitizeCanvasState,
  snapCanvasCoord,
} from "./canvas";
import {
  createSurfaceId,
  createDefaultSurface,
  createDefaultWorkspace,
  createWorkspaceId,
  pickAccent,
  syncWorkspaceAndSurfaceCounters,
} from "./helpers";
import { buildPaneIcons, buildPaneNames } from "./pane-metadata";
import {
  createDefaultWorkspaceBrowserState,
  DEFAULT_WEB_BROWSER_URL,
  normalizeWorkspaceBrowserState,
} from "./browser";

export function createCanvasActions(
  ctx: WorkspaceStoreContext,
): Pick<
  WorkspaceState,
  | "createCanvasPanel"
  | "updateCanvasPanelUrl"
  | "updateCanvasPanelTitle"
  | "updateCanvasPanelCwd"
  | "renameCanvasPanel"
  | "moveCanvasPanel"
  | "resizeCanvasPanel"
  | "arrangeCanvasPanels"
  | "setCanvasView"
  | "setCanvasPreviousView"
  | "focusCanvasPanel"
  | "clearCanvasPanelStatus"
  | "setCanvasPanelStatus"
  | "setCanvasPanelIcon"
  | "hydrateSession"
> {
  return {
    createCanvasPanel: (surfaceId, opts) => {
      const surface = surfaceId ? ctx.findWsAndSurface(surfaceId)?.sf : ctx.getActiveSurface();
      if (!surface || surface.layoutMode !== "canvas") return null;
      const targetPaneId = surface.activePaneId ?? allLeafIds(surface.layout)[0];
      if (!targetPaneId) return null;
      const split = splitPane(surface.layout, targetPaneId, "horizontal");
      const nextLayout = typeof opts?.sessionId === "string" && opts.sessionId
        ? setSessionId(split.tree, split.newPaneId, opts.sessionId)
        : split.tree;
      const placement = findCanvasPlacement(surface, targetPaneId);
      const isBrowser = opts?.panelType === "browser";
      const requestedName = typeof opts?.paneName === "string" ? opts.paneName.trim() : "";
      const nextIcon = normalizeIconId(opts?.paneIcon ?? (isBrowser ? "web" : "terminal"));
      const persistedSessionId = !isBrowser && typeof opts?.sessionId === "string" && opts.sessionId
        ? opts.sessionId
        : undefined;

      ctx.updateSurface(surface.id, (currentSurface) => {
        const paneIds = allLeafIds(nextLayout);
        const paneNames = buildPaneNames(paneIds, {
          ...currentSurface.paneNames,
          [split.newPaneId]: requestedName || (isBrowser ? "Browser" : `Pane ${paneIds.length}`),
        });
        const paneIcons = buildPaneIcons(paneIds, {
          ...currentSurface.paneIcons,
          [split.newPaneId]: nextIcon,
        });
        const nextPanels = [
          ...currentSurface.canvasPanels,
          buildCanvasPanel({
            paneId: split.newPaneId,
            paneName: paneNames[split.newPaneId],
            index: currentSurface.canvasPanels.length,
            persisted: {
              x: Number.isFinite(opts?.x) ? Number(opts?.x) : placement.x,
              y: Number.isFinite(opts?.y) ? Number(opts?.y) : placement.y,
              width: Number.isFinite(opts?.width) ? Number(opts?.width) : 760,
              height: Number.isFinite(opts?.height) ? Number(opts?.height) : 440,
              icon: paneIcons[split.newPaneId],
              ...(persistedSessionId ? { sessionId: persistedSessionId } : {}),
              ...(isBrowser ? {
                panelType: "browser" as const,
                profileId: opts?.profileId ?? null,
                url: opts?.url ?? DEFAULT_WEB_BROWSER_URL,
              } : {}),
            },
            status: isBrowser ? "idle" : (persistedSessionId ? "running" : "idle"),
          }),
        ];

        return normalizeCanvasPanels({
          ...currentSurface,
          layout: nextLayout,
          paneNames,
          paneIcons,
          activePaneId: split.newPaneId,
          canvasPanels: nextPanels,
        });
      });

      return split.newPaneId;
    },

    updateCanvasPanelUrl: (paneId, url) => {
      const pair = ctx.findWsSurfaceAndPane(paneId);
      if (!pair || pair.sf.layoutMode !== "canvas") return;
      ctx.updateSurface(pair.sf.id, (surface) => ({
        ...surface,
        canvasPanels: surface.canvasPanels.map((panel) => (
          panel.paneId === paneId ? { ...panel, url, lastActivityAt: Date.now() } : panel
        )),
      }));
    },

    updateCanvasPanelTitle: (paneId, title) => {
      const pair = ctx.findWsSurfaceAndPane(paneId);
      if (!pair || pair.sf.layoutMode !== "canvas") return;
      const current = pair.sf.canvasPanels.find((panel) => panel.paneId === paneId);
      if (current?.userRenamed || current?.title === title) return;
      ctx.updateSurface(pair.sf.id, (surface) => ({
        ...surface,
        canvasPanels: surface.canvasPanels.map((panel) => (
          panel.paneId === paneId && !panel.userRenamed
            ? { ...panel, title, lastActivityAt: Date.now() }
            : panel
        )),
        paneNames: { ...surface.paneNames, [paneId]: title },
      }));
    },

    updateCanvasPanelCwd: (paneId, cwd) => {
      const pair = ctx.findWsSurfaceAndPane(paneId);
      if (!pair || pair.sf.layoutMode !== "canvas") return;
      const current = pair.sf.canvasPanels.find((panel) => panel.paneId === paneId);
      if (current?.cwd === cwd) return;
      ctx.updateSurface(pair.sf.id, (surface) => ({
        ...surface,
        canvasPanels: surface.canvasPanels.map((panel) => (
          panel.paneId === paneId ? { ...panel, cwd, lastActivityAt: Date.now() } : panel
        )),
      }));
    },

    renameCanvasPanel: (paneId, name) => {
      const pair = ctx.findWsSurfaceAndPane(paneId);
      if (!pair || pair.sf.layoutMode !== "canvas") return;
      ctx.updateSurface(pair.sf.id, (surface) => ({
        ...surface,
        canvasPanels: surface.canvasPanels.map((panel) => (
          panel.paneId === paneId
            ? { ...panel, title: name, userRenamed: true, lastActivityAt: Date.now() }
            : panel
        )),
        paneNames: { ...surface.paneNames, [paneId]: name },
      }));
    },

    moveCanvasPanel: (paneId, x, y) => {
      const pair = ctx.findWsSurfaceAndPane(paneId);
      if (!pair || pair.sf.layoutMode !== "canvas") return;
      ctx.updateSurface(pair.sf.id, (surface) => normalizeCanvasPanels({
        ...surface,
        canvasPanels: surface.canvasPanels.map((panel) => (
          panel.paneId === paneId ? { ...panel, x, y, lastActivityAt: Date.now() } : panel
        )),
      }));
    },

    resizeCanvasPanel: (paneId, width, height) => {
      const pair = ctx.findWsSurfaceAndPane(paneId);
      if (!pair || pair.sf.layoutMode !== "canvas") return;
      ctx.updateSurface(pair.sf.id, (surface) => normalizeCanvasPanels({
        ...surface,
        canvasPanels: surface.canvasPanels.map((panel) => (
          panel.paneId === paneId
            ? {
              ...panel,
              width: Math.max(320, width),
              height: Math.max(220, height),
              lastActivityAt: Date.now(),
            }
            : panel
        )),
      }));
    },

    arrangeCanvasPanels: (surfaceId) => {
      const surface = surfaceId ? ctx.findWsAndSurface(surfaceId)?.sf : ctx.getActiveSurface();
      if (!surface || surface.layoutMode !== "canvas" || surface.canvasPanels.length === 0) {
        return;
      }

      const panels = surface.canvasPanels;
      const maxWidth = Math.max(...panels.map((panel) => Math.max(320, panel.width)));
      const maxHeight = Math.max(...panels.map((panel) => Math.max(220, panel.height)));
      const columnCount = Math.max(1, Math.min(5, Math.round(Math.sqrt(panels.length * 1.4))));
      const stepX = maxWidth + 48;
      const stepY = maxHeight + 40;
      const ordered = [
        ...panels.filter((panel) => panel.paneId === surface.activePaneId),
        ...panels.filter((panel) => panel.paneId !== surface.activePaneId),
      ];
      const positions = new Map<string, { x: number; y: number }>();

      ordered.forEach((panel, index) => {
        const row = Math.floor(index / columnCount);
        const col = index % columnCount;
        positions.set(panel.paneId, {
          x: snapCanvasCoord(80 + col * stepX),
          y: snapCanvasCoord(60 + row * stepY),
        });
      });

      ctx.updateSurface(surface.id, (currentSurface) => normalizeCanvasPanels({
        ...currentSurface,
        canvasPanels: currentSurface.canvasPanels.map((panel) => {
          const position = positions.get(panel.paneId);
          if (!position) {
            return panel;
          }
          return {
            ...panel,
            x: position.x,
            y: position.y,
            lastActivityAt: Date.now(),
          };
        }),
      }));
    },

    setCanvasView: (surfaceId, view) => {
      ctx.updateSurface(surfaceId, (surface) => {
        if (surface.layoutMode !== "canvas") return surface;
        return normalizeCanvasPanels({
          ...surface,
          canvasState: sanitizeCanvasState({
            ...surface.canvasState,
            ...view,
          }),
        });
      });
    },

    setCanvasPreviousView: (surfaceId, snapshot) => {
      ctx.updateSurface(surfaceId, (surface) => {
        if (surface.layoutMode !== "canvas") return surface;
        const normalizedSnapshot = snapshot
          ? {
            panX: Number.isFinite(snapshot.panX) ? snapshot.panX : 0,
            panY: Number.isFinite(snapshot.panY) ? snapshot.panY : 0,
            zoomLevel: Math.max(CANVAS_MIN_ZOOM, Math.min(CANVAS_MAX_ZOOM, snapshot.zoomLevel)),
          }
          : null;
        return {
          ...surface,
          canvasState: {
            ...surface.canvasState,
            previousView: normalizedSnapshot,
          },
        };
      });
    },

    focusCanvasPanel: (paneId, opts) => {
      const pair = ctx.findWsSurfaceAndPane(paneId);
      if (!pair || pair.sf.layoutMode !== "canvas") return;
      const nextNonce = (pair.sf.canvasState.focusRequestNonce ?? 0) + 1;
      const previous = opts?.storePreviousView !== false
        ? {
          panX: pair.sf.canvasState.panX,
          panY: pair.sf.canvasState.panY,
          zoomLevel: pair.sf.canvasState.zoomLevel,
        }
        : pair.sf.canvasState.previousView;

      ctx.updateSurface(pair.sf.id, (surface) => normalizeCanvasPanels({
        ...surface,
        activePaneId: paneId,
        canvasState: sanitizeCanvasState({
          ...surface.canvasState,
          previousView: previous,
          focusRequestNonce: nextNonce,
        }),
      }));
    },

    clearCanvasPanelStatus: (paneId) => {
      const pair = ctx.findWsSurfaceAndPane(paneId);
      if (!pair || pair.sf.layoutMode !== "canvas") return;
      ctx.updateSurface(pair.sf.id, (surface) => normalizeCanvasPanels({
        ...surface,
        canvasPanels: surface.canvasPanels.map((panel) => (
          panel.paneId === paneId
            ? { ...panel, status: "running", lastActivityAt: Date.now() }
            : panel
        )),
      }));
    },

    setCanvasPanelStatus: (paneId, status) => {
      const pair = ctx.findWsSurfaceAndPane(paneId);
      if (!pair || pair.sf.layoutMode !== "canvas") return;
      ctx.updateSurface(pair.sf.id, (surface) => normalizeCanvasPanels({
        ...surface,
        canvasPanels: surface.canvasPanels.map((panel) => (
          panel.paneId === paneId ? { ...panel, status, lastActivityAt: Date.now() } : panel
        )),
      }));
    },

    setCanvasPanelIcon: (paneId, icon) => {
      const pair = ctx.findWsSurfaceAndPane(paneId);
      if (!pair || pair.sf.layoutMode !== "canvas") return;
      const nextIcon = normalizeIconId(icon);
      ctx.updateSurface(pair.sf.id, (surface) => normalizeCanvasPanels({
        ...surface,
        paneIcons: {
          ...surface.paneIcons,
          [paneId]: nextIcon,
        },
        canvasPanels: surface.canvasPanels.map((panel) => (
          panel.paneId === paneId ? { ...panel, icon: nextIcon } : panel
        )),
      }));
    },

    hydrateSession: (session) => {
      const sessionWorkspaces = Array.isArray(session.workspaces) ? session.workspaces : [];
      const hydratedWorkspaceBrowserState: WorkspaceState["workspaceBrowserState"] = {};

      const hydratedWorkspaces = sessionWorkspaces.map((workspace, workspaceIndex) => {
        const workspaceId = typeof workspace.id === "string" && workspace.id
          ? workspace.id
          : createWorkspaceId();
        hydratedWorkspaceBrowserState[workspaceId] = normalizeWorkspaceBrowserState({
          open: Boolean(workspace.browser?.open),
          fullscreen: Boolean(workspace.browser?.fullscreen),
          url: typeof workspace.browser?.url === "string" ? workspace.browser.url : DEFAULT_WEB_BROWSER_URL,
          history: Array.isArray(workspace.browser?.history) ? workspace.browser.history : undefined,
          historyIndex: typeof workspace.browser?.historyIndex === "number" ? workspace.browser.historyIndex : undefined,
        });

        const sessionSurfaces = Array.isArray(workspace.surfaces) ? workspace.surfaces : [];
        const surfaces = sessionSurfaces.length > 0
          ? sessionSurfaces.map((surface, surfaceIndex) => {
            const layoutMode: SurfaceLayoutMode = surface.layoutMode === "canvas" ? "canvas" : "bsp";
            const fallbackPaneIds = [
              ...(typeof surface.activePaneId === "string" ? [surface.activePaneId] : []),
              ...(Array.isArray(surface.panes)
                ? surface.panes
                  .map((pane) => pane?.id)
                  .filter((paneId): paneId is string => typeof paneId === "string" && paneId.length > 0)
                : []),
              ...(Array.isArray(surface.canvasPanels)
                ? surface.canvasPanels
                  .map((panel) => panel?.paneId)
                  .filter((paneId): paneId is string => typeof paneId === "string" && paneId.length > 0)
                : []),
            ];
            const layout = normalizeBspTree(surface.layout, fallbackPaneIds);
            const paneIds = allLeafIds(layout);
            const activePaneId = typeof surface.activePaneId === "string" && paneIds.includes(surface.activePaneId)
              ? surface.activePaneId
              : paneIds[0] ?? null;
            const paneNames = buildPaneNames(
              paneIds,
              typeof surface.paneNames === "object" && surface.paneNames
                ? surface.paneNames as Record<string, string>
                : undefined,
            );
            const paneIcons = buildPaneIcons(
              paneIds,
              typeof surface.paneIcons === "object" && surface.paneIcons
                ? surface.paneIcons as Record<string, string>
                : undefined,
            );
            const persistedPanelByPane = new Map(
              Array.isArray(surface.canvasPanels)
                ? surface.canvasPanels
                  .filter((panel): panel is PersistedCanvasPanel => Boolean(panel?.paneId))
                  .map((panel) => [panel.paneId, panel])
                : [],
            );
            const hydratedSurface: Surface = {
              id: typeof surface.id === "string" && surface.id ? surface.id : createSurfaceId(),
              workspaceId,
              name: typeof surface.name === "string" && surface.name ? surface.name : `Surface ${surfaceIndex + 1}`,
              icon: normalizeIconId(surface.icon),
              layoutMode,
              layout,
              paneNames,
              paneIcons,
              activePaneId,
              canvasState: sanitizeCanvasState(surface.canvasState),
              canvasPanels: layoutMode === "canvas"
                ? paneIds.map((paneId, index) => buildCanvasPanel({
                  paneId,
                  paneName: paneNames[paneId],
                  index,
                  persisted: persistedPanelByPane.get(paneId),
                  status: persistedPanelByPane.get(paneId)?.status,
                }))
                : [],
              createdAt: Date.now(),
            };

            return normalizeCanvasPanels(hydratedSurface);
          })
          : [createDefaultSurface(workspaceId)];

        const activeSurfaceId = typeof workspace.activeSurfaceId === "string"
          && surfaces.some((surface) => surface.id === workspace.activeSurfaceId)
          ? workspace.activeSurfaceId
          : surfaces[0]?.id ?? null;

        return {
          id: workspaceId,
          name: typeof workspace.name === "string" && workspace.name
            ? workspace.name
            : `Workspace ${workspaceIndex + 1}`,
          icon: normalizeIconId(workspace.icon),
          accentColor: typeof workspace.accentColor === "string" && workspace.accentColor
            ? workspace.accentColor
            : pickAccent(workspaceIndex),
          cwd: typeof workspace.cwd === "string" ? workspace.cwd : "",
          gitBranch: null,
          gitDirty: false,
          listeningPorts: [],
          unreadCount: 0,
          surfaces,
          activeSurfaceId,
          createdAt: Date.now(),
          agentOwned: workspace.agentOwned === true
            || (typeof workspace.name === "string" && workspace.name.startsWith("Agent - ")),
          lastActivityAt: typeof workspace.lastActivityAt === "number" && Number.isFinite(workspace.lastActivityAt)
            ? workspace.lastActivityAt
            : undefined,
        };
      });

      const normalizedWorkspaces = hydratedWorkspaces.length > 0
        ? hydratedWorkspaces
        : [createDefaultWorkspace()];

      for (const workspace of normalizedWorkspaces) {
        if (!hydratedWorkspaceBrowserState[workspace.id]) {
          hydratedWorkspaceBrowserState[workspace.id] = createDefaultWorkspaceBrowserState();
        }
      }

      for (const workspace of normalizedWorkspaces) {
        for (const surface of workspace.surfaces) {
          syncPaneIdCounter(surface.layout);
        }
      }

      syncWorkspaceAndSurfaceCounters(normalizedWorkspaces);

      const activeWorkspaceId = typeof session.activeWorkspaceId === "string"
        && normalizedWorkspaces.some((workspace) => workspace.id === session.activeWorkspaceId)
        ? session.activeWorkspaceId
        : normalizedWorkspaces[0]?.id ?? null;

      ctx.set((state) => ({
        ...state,
        workspaces: normalizedWorkspaces,
        activeWorkspaceId,
        ...ctx.activateWorkspaceBrowserState(hydratedWorkspaceBrowserState, activeWorkspaceId),
        sidebarVisible: typeof session.sidebarVisible === "boolean" ? session.sidebarVisible : state.sidebarVisible,
        sidebarWidth: typeof session.sidebarWidth === "number" && Number.isFinite(session.sidebarWidth)
          ? session.sidebarWidth
          : state.sidebarWidth,
        zoomedPaneId: null,
      }));
    },
  };
}
