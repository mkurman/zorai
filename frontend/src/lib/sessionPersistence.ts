import {
  PersistedSession,
  PersistedWorkspace,
  PersistedSurface,
  PersistedPane,
} from "./types";
import { allLeafIds, findLeaf } from "./bspTree";
import { useWorkspaceStore } from "./workspaceStore";
import {
  isReclaimableAgentWorkspace,
  workspaceLastActivityAt,
} from "./agentWorkspaceReclaim";
import {
  deletePersistedPath,
  readPersistedJson,
  scheduleJsonWrite,
} from "./persistence";

const SESSION_FILE = "session.json";
const TOPOLOGY_FILE = "workspace-topology.json";
const VERSION = 1;

/** Serialize the current workspace state into a PersistedSession blob. */
export function captureSession(): PersistedSession {
  const state = useWorkspaceStore.getState();

  return {
    version: VERSION,
    windowState: {
      x: window.screenX,
      y: window.screenY,
      width: window.innerWidth,
      height: window.innerHeight,
      maximized: false,
    },
    sidebarVisible: state.sidebarVisible,
    sidebarWidth: state.sidebarWidth,
    workspaces: state.workspaces.map(
      (ws): PersistedWorkspace => ({
        id: ws.id,
        name: ws.name,
        icon: ws.icon,
        accentColor: ws.accentColor,
        cwd: ws.cwd,
        agentOwned: isReclaimableAgentWorkspace(ws) || undefined,
        lastActivityAt: workspaceLastActivityAt(ws) || undefined,
        browser: (() => {
          const defaultUrl = "https://google.com";
          const fallbackUrl = ws.id === state.activeWorkspaceId ? state.webBrowserUrl : defaultUrl;
          const fallbackHistory = ws.id === state.activeWorkspaceId
            ? state.webBrowserHistory
            : [fallbackUrl];
          const fallbackHistoryIndex = ws.id === state.activeWorkspaceId
            ? state.webBrowserHistoryIndex
            : 0;
          const browser = state.workspaceBrowserState[ws.id];
          const normalizedUrl = browser?.url ?? fallbackUrl;
          const normalizedHistory = Array.isArray(browser?.history) && browser.history.length > 0
            ? browser.history
            : fallbackHistory.length > 0
              ? fallbackHistory
              : [normalizedUrl];
          const historyIndex = browser
            ? Math.max(0, Math.min(normalizedHistory.length - 1, browser.historyIndex))
            : Math.max(0, Math.min(normalizedHistory.length - 1, fallbackHistoryIndex));

          return {
            open: browser?.open ?? (ws.id === state.activeWorkspaceId ? state.webBrowserOpen : false),
            fullscreen: browser?.fullscreen ?? (ws.id === state.activeWorkspaceId ? state.webBrowserFullscreen : false),
            url: normalizedHistory[historyIndex] ?? normalizedUrl,
            history: normalizedHistory,
            historyIndex,
          };
        })(),
        surfaces: ws.surfaces.map(
          (sf): PersistedSurface => ({
            id: sf.id,
            name: sf.name,
            icon: sf.icon,
            layoutMode: sf.layoutMode,
            layout: sf.layout,
            activePaneId: sf.activePaneId,
            paneNames: sf.paneNames,
            paneIcons: sf.paneIcons,
            canvasState: sf.layoutMode === "canvas"
              ? {
                panX: sf.canvasState.panX,
                panY: sf.canvasState.panY,
                zoomLevel: sf.canvasState.zoomLevel,
                previousView: sf.canvasState.previousView,
              }
              : undefined,
            canvasPanels: sf.layoutMode === "canvas"
              ? sf.canvasPanels.map((panel) => ({
                id: panel.id,
                paneId: panel.paneId,
                panelType: panel.panelType,
                title: panel.title,
                icon: panel.icon,
                x: panel.x,
                y: panel.y,
                width: panel.width,
                height: panel.height,
                status: panel.status,
                sessionId: panel.sessionId,
                profileId: panel.profileId,
                url: panel.url,
                cwd: panel.cwd,
                userRenamed: panel.userRenamed || undefined,
                lastActivityAt: panel.lastActivityAt,
              }))
              : undefined,
            panes: allLeafIds(sf.layout).map(
              (paneId): PersistedPane => ({
                id: paneId,
                cwd: null,
                scrollback: null,
                commandHistory: [],
              })
            ),
          })
        ),
        activeSurfaceId: ws.activeSurfaceId,
      })
    ),
    activeWorkspaceId: state.activeWorkspaceId,
  };
}

/** Capture workspace topology for the daemon's `list_terminals` tool. */
function captureWorkspaceTopology() {
  const state = useWorkspaceStore.getState();
  return {
    workspaces: state.workspaces.map((ws) => {
      const isActiveWorkspace = ws.id === state.activeWorkspaceId;
      return {
        workspace_id: ws.id,
        workspace_name: ws.name,
        agent_owned: isReclaimableAgentWorkspace(ws),
        last_activity_at: workspaceLastActivityAt(ws),
        surfaces: ws.surfaces.map((sf) => {
          const isActiveSurface = sf.id === ws.activeSurfaceId;
          const canvasPanelMap = new Map(
            sf.canvasPanels?.map((p) => [p.paneId, p]) ?? [],
          );
          const paneIds = allLeafIds(sf.layout);
          return {
            surface_id: sf.id,
            surface_name: sf.name,
            layout_mode: sf.layoutMode,
            is_active: isActiveWorkspace && isActiveSurface,
            panes: paneIds.map((paneId) => {
              const canvasPanel = canvasPanelMap.get(paneId);
              const panelType = canvasPanel?.panelType ?? "terminal";
              const leaf = findLeaf(sf.layout, paneId);
              return {
                pane_id: paneId,
                pane_name: sf.paneNames[paneId] || paneId,
                pane_type: panelType,
                is_active: isActiveWorkspace && isActiveSurface && paneId === sf.activePaneId,
                session_id: panelType === "terminal"
                  ? (canvasPanel?.sessionId ?? leaf?.sessionId ?? null)
                  : null,
                url: panelType === "browser" ? (canvasPanel?.url ?? null) : null,
                title: panelType === "browser" ? (canvasPanel?.title ?? null) : null,
                cwd: panelType === "terminal" ? (canvasPanel?.cwd ?? null) : null,
              };
            }),
          };
        }),
      };
    }),
  };
}

let lastTopologyJson = "";

/** Write workspace topology to the data dir for the daemon (skips if unchanged). */
export function saveWorkspaceTopology(): void {
  const data = captureWorkspaceTopology();
  const next = JSON.stringify(data);
  if (next === lastTopologyJson) return;
  lastTopologyJson = next;
  scheduleJsonWrite(TOPOLOGY_FILE, data, 200);
}

/** Save the current session to the zorai data directory. */
export function saveSession(): void {
  const data = captureSession();
  scheduleJsonWrite(SESSION_FILE, data, 300);
  saveWorkspaceTopology();
}

/** Load a persisted session from the zorai data directory. Returns null if nothing stored. */
export async function loadSession(): Promise<PersistedSession | null> {
  const diskSession = await readPersistedJson<PersistedSession>(SESSION_FILE);
  if (diskSession?.version === VERSION) {
    return diskSession;
  }

  return null;
}

/** Clear the persisted session. */
export function clearSession(): void {
  void deletePersistedPath(SESSION_FILE);
}

/** Start auto-saving the session at a regular interval. Returns cleanup fn. */
export function startAutoSave(intervalMs: number = 30_000): () => void {
  const id = setInterval(saveSession, intervalMs);
  // Sync topology more frequently so daemon agents see changes quickly.
  const topologyId = setInterval(saveWorkspaceTopology, 5_000);
  // Also save on unload.
  const onBeforeUnload = () => saveSession();
  window.addEventListener("beforeunload", onBeforeUnload);
  return () => {
    clearInterval(id);
    clearInterval(topologyId);
    window.removeEventListener("beforeunload", onBeforeUnload);
  };
}
