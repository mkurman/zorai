import type {
  CanvasPanelStatus,
  CanvasViewSnapshot,
  PaneId,
  PersistedSession,
  Surface,
  SurfaceId,
  SurfaceLayoutMode,
  Workspace,
  WorkspaceId,
} from "../types";
import type { Direction, PresetLayout, SplitDirection } from "../bspTree";

export type WorkspaceBrowserState = {
  open: boolean;
  fullscreen: boolean;
  url: string;
  history: string[];
  historyIndex: number;
  reloadToken: number;
};

export type WorkspaceBrowserProjection = {
  webBrowserOpen: boolean;
  webBrowserFullscreen: boolean;
  webBrowserUrl: string;
  webBrowserHistory: string[];
  webBrowserHistoryIndex: number;
  webBrowserReloadToken: number;
};

export interface WorkspaceState {
  workspaces: Workspace[];
  activeWorkspaceId: WorkspaceId | null;
  sidebarVisible: boolean;
  sidebarWidth: number;
  zoomedPaneId: PaneId | null;
  commandPaletteOpen: boolean;
  searchOpen: boolean;
  settingsOpen: boolean;
  notificationPanelOpen: boolean;
  sessionVaultOpen: boolean;
  commandLogOpen: boolean;
  commandHistoryOpen: boolean;
  snippetPickerOpen: boolean;
  agentPanelOpen: boolean;
  systemMonitorOpen: boolean;
  fileManagerOpen: boolean;
  canvasOpen: boolean;
  timeTravelOpen: boolean;
  workspaceBrowserState: Record<WorkspaceId, WorkspaceBrowserState>;
  webBrowserOpen: boolean;
  webBrowserFullscreen: boolean;
  webBrowserUrl: string;
  webBrowserHistory: string[];
  webBrowserHistoryIndex: number;
  webBrowserReloadToken: number;
  createWorkspace: (name?: string, opts?: { layoutMode?: SurfaceLayoutMode; makeActive?: boolean; agentOwned?: boolean }) => WorkspaceId;
  renameWorkspace: (id: WorkspaceId, name: string) => void;
  setWorkspaceIcon: (id: WorkspaceId, icon: string) => void;
  closeWorkspace: (id: WorkspaceId) => void;
  setActiveWorkspace: (id: WorkspaceId) => void;
  switchWorkspaceByIndex: (index: number) => void;
  nextWorkspace: () => void;
  prevWorkspace: () => void;
  updateWorkspaceGit: (id: WorkspaceId, branch: string | null, dirty: boolean) => void;
  updateWorkspaceCwd: (id: WorkspaceId, cwd: string) => void;
  updateWorkspacePorts: (id: WorkspaceId, ports: number[]) => void;
  clearWorkspaceUnread: (id: WorkspaceId) => void;
  createSurface: (workspaceId?: WorkspaceId, opts?: { layoutMode?: SurfaceLayoutMode; makeActive?: boolean }) => SurfaceId | null;
  renameSurface: (surfaceId: SurfaceId, name: string) => void;
  setSurfaceIcon: (surfaceId: SurfaceId, icon: string) => void;
  closeSurface: (surfaceId: SurfaceId) => void;
  nextSurface: () => void;
  prevSurface: () => void;
  setActiveSurface: (surfaceId: SurfaceId) => void;
  splitActive: (
    direction: SplitDirection,
    newPaneName?: string,
    opts?: { sessionId?: string | null; paneIcon?: string },
  ) => void;
  closePane: (paneId: PaneId, opts?: { stopSession?: boolean; captureTranscript?: boolean }) => void;
  setActivePaneId: (paneId: PaneId) => void;
  clearActivePaneFocus: (surfaceId?: SurfaceId) => void;
  setPaneSessionId: (paneId: PaneId, sessionId: string) => void;
  setPaneName: (paneId: PaneId, name: string) => void;
  setPaneIcon: (paneId: PaneId, icon: string) => void;
  paneName: (paneId: PaneId) => string | null;
  focusDirection: (direction: Direction) => void;
  toggleZoom: () => void;
  applyPresetLayout: (preset: PresetLayout) => void;
  equalizeLayout: () => void;
  toggleSidebar: () => void;
  setSidebarWidth: (width: number) => void;
  toggleCommandPalette: () => void;
  toggleSearch: () => void;
  toggleSettings: () => void;
  toggleNotificationPanel: () => void;
  toggleSessionVault: () => void;
  toggleCommandLog: () => void;
  toggleCommandHistory: () => void;
  toggleSnippetPicker: () => void;
  toggleAgentPanel: () => void;
  toggleSystemMonitor: () => void;
  toggleFileManager: () => void;
  toggleCanvas: () => void;
  toggleTimeTravel: () => void;
  toggleWebBrowser: () => void;
  setWebBrowserOpen: (open: boolean) => void;
  navigateWebBrowser: (url: string) => void;
  webBrowserBack: () => void;
  webBrowserForward: () => void;
  webBrowserReload: () => void;
  toggleWebBrowserFullscreen: () => void;
  setWebBrowserFullscreen: (fullscreen: boolean) => void;
  updateNodeRatio: (paneId: PaneId, newRatio: number) => void;
  createCanvasPanel: (
    surfaceId?: SurfaceId,
    opts?: {
      paneName?: string;
      paneIcon?: string;
      sessionId?: string | null;
      panelType?: import("../types").CanvasPanelType;
      profileId?: string | null;
      url?: string;
      x?: number;
      y?: number;
      width?: number;
      height?: number;
    },
  ) => PaneId | null;
  updateCanvasPanelUrl: (paneId: PaneId, url: string) => void;
  updateCanvasPanelTitle: (paneId: PaneId, title: string) => void;
  updateCanvasPanelCwd: (paneId: PaneId, cwd: string) => void;
  renameCanvasPanel: (paneId: PaneId, name: string) => void;
  moveCanvasPanel: (paneId: PaneId, x: number, y: number) => void;
  resizeCanvasPanel: (paneId: PaneId, width: number, height: number) => void;
  arrangeCanvasPanels: (surfaceId?: SurfaceId) => void;
  setCanvasView: (surfaceId: SurfaceId, view: Partial<CanvasViewSnapshot>) => void;
  setCanvasPreviousView: (surfaceId: SurfaceId, snapshot: CanvasViewSnapshot | null) => void;
  focusCanvasPanel: (paneId: PaneId, opts?: { storePreviousView?: boolean }) => void;
  clearCanvasPanelStatus: (paneId: PaneId) => void;
  setCanvasPanelStatus: (paneId: PaneId, status: CanvasPanelStatus) => void;
  setCanvasPanelIcon: (paneId: PaneId, icon: string) => void;
  hydrateSession: (session: PersistedSession) => void;
  activeWorkspace: () => Workspace | undefined;
  activeSurface: () => Surface | undefined;
  activePaneId: () => PaneId | null;
}
