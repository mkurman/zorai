import { useEffect, type CSSProperties } from "react";
import { CommandLogPanel } from "@/components/CommandLogPanel";
import { FileManagerPanel } from "@/components/FileManagerPanel";
import { InfiniteCanvasSurface } from "@/components/InfiniteCanvasSurface";
import { LayoutContainer } from "@/components/LayoutContainer";
import { SessionVaultPanel } from "@/components/SessionVaultPanel";
import { SystemMonitorPanel } from "@/components/SystemMonitorPanel";
import { WebBrowserPanel } from "@/components/WebBrowserPanel";
import type { Surface } from "@/lib/types";
import { useWorkspaceStore } from "@/lib/workspaceStore";
import { zoraiTools, type ZoraiToolId } from "./tools";
import { ContextRow } from "@/zorai/features/threads/ThreadContextRow";

type ToolsProps = {
  activeTool: ZoraiToolId;
  onSelectTool: (toolId: ZoraiToolId) => void;
};

const embeddedPanelStyle: CSSProperties = {
  position: "relative",
  inset: "auto",
  zIndex: "auto",
  width: "100%",
  minWidth: 0,
  maxWidth: "none",
  height: "100%",
  minHeight: 0,
  maxHeight: "none",
  padding: 0,
  background: "transparent",
  border: 0,
  borderRadius: 0,
  overflow: "hidden",
};

export function ToolsRail({ activeTool, onSelectTool }: ToolsProps) {
  return (
    <div className="zorai-rail-stack">
      {zoraiTools.map((tool) => (
        <button
          type="button"
          key={tool.id}
          className={[
            "zorai-rail-card",
            "zorai-rail-card--button",
            tool.id === activeTool ? "zorai-rail-card--active" : "",
          ].filter(Boolean).join(" ")}
          onClick={() => onSelectTool(tool.id)}
        >
          <strong className="zorai-rail-card__title">{tool.title}</strong>
          <span className="zorai-rail-card__status">{tool.description}</span>
        </button>
      ))}
    </div>
  );
}

export function ToolsView({ activeTool, onSelectTool }: ToolsProps) {
  const selectedTool = zoraiTools.find((tool) => tool.id === activeTool) ?? zoraiTools[0];

  return (
    <section className="zorai-feature-surface zorai-tools-surface">
      <div className="zorai-view-header zorai-tools-header">
        <div>
          <div className="zorai-kicker">Tools</div>
          <h1>{selectedTool.title}</h1>
          <p>{selectedTool.description}</p>
        </div>
      </div>

      <div className="zorai-tool-tab-strip" aria-label="Tool sections">
        {zoraiTools.map((tool) => (
          <button
            type="button"
            key={tool.id}
            className={["zorai-ghost-button", tool.id === activeTool ? "zorai-button--active" : ""].filter(Boolean).join(" ")}
            onClick={() => onSelectTool(tool.id)}
          >
            {tool.title}
          </button>
        ))}
      </div>

      <div className="zorai-tool-layout">
        <div className="zorai-tool-frame">
          <ToolSurface activeTool={activeTool} />
        </div>
      </div>
    </section>
  );
}

export function ToolsContext({ activeTool, onSelectTool }: ToolsProps) {
  const workspaces = useWorkspaceStore((state) => state.workspaces);
  const activeWorkspaceId = useWorkspaceStore((state) => state.activeWorkspaceId);
  const setActiveWorkspace = useWorkspaceStore((state) => state.setActiveWorkspace);
  const setActiveSurface = useWorkspaceStore((state) => state.setActiveSurface);
  const closeWorkspace = useWorkspaceStore((state) => state.closeWorkspace);

  return (
    <div className="zorai-thread-context-stack">
      <div className="zorai-context-group">
        <div className="zorai-context-group__title">{activeToolLabel(activeTool)} workspaces</div>
        {workspaces.length === 0 ? (
          <p className="zorai-context-hint">No operational workspaces yet.</p>
        ) : workspaces.map((workspace) => (
          <ContextRow
            key={workspace.id}
            icon="terminal"
            label={workspace.name}
            value={`${workspace.id === activeWorkspaceId ? "active · " : ""}${workspace.surfaces.length} surface${workspace.surfaces.length === 1 ? "" : "s"}`}
            defaultOpen={workspace.id === activeWorkspaceId}
          >
            <div className="zorai-context-workspace">
              <code title={workspace.cwd}>{workspace.cwd}</code>
              <div className="zorai-context-chips">
                {workspace.surfaces.map((surface) => (
                  <button
                    type="button"
                    key={surface.id}
                    className="zorai-context-chip"
                    onClick={() => {
                      setActiveWorkspace(workspace.id);
                      setActiveSurface(surface.id);
                      onSelectTool(surface.layoutMode === "canvas" ? "canvas" : "terminal");
                    }}
                  >
                    {surface.name}
                  </button>
                ))}
              </div>
              <div className="zorai-context-workspace__actions">
                <button
                  type="button"
                  className="zorai-ghost-button zorai-ghost-button--compact"
                  onClick={() => {
                    setActiveWorkspace(workspace.id);
                    onSelectTool("terminal");
                  }}
                >
                  Open
                </button>
                <button type="button" className="zorai-ghost-button zorai-ghost-button--compact" aria-label={`Remove workspace ${workspace.name}`} onClick={() => closeWorkspace(workspace.id)}>Remove</button>
              </div>
            </div>
          </ContextRow>
        ))}
      </div>
    </div>
  );
}

function ToolSurface({ activeTool }: { activeTool: ZoraiToolId }) {
  useEmbeddedToolState(activeTool);

  if (activeTool === "terminal") return <TerminalTool />;
  if (activeTool === "canvas") return <CanvasTool />;
  if (activeTool === "files") return <FileManagerPanel style={embeddedPanelStyle} className="zorai-embedded-tool-panel" />;
  if (activeTool === "browser") return <WebBrowserPanel style={embeddedPanelStyle} className="zorai-embedded-tool-panel" />;
  if (activeTool === "history") return <CommandLogPanel style={embeddedPanelStyle} className="zorai-embedded-tool-panel" />;
  if (activeTool === "system") return <SystemMonitorPanel style={embeddedPanelStyle} className="zorai-embedded-tool-panel" />;

  return <SessionVaultPanel style={embeddedPanelStyle} className="zorai-embedded-tool-panel" />;
}

function TerminalTool() {
  const activeWorkspace = useWorkspaceStore((state) => state.activeWorkspace());
  const activeSurface = useWorkspaceStore((state) => state.activeSurface());
  const createWorkspace = useWorkspaceStore((state) => state.createWorkspace);
  const createSurface = useWorkspaceStore((state) => state.createSurface);
  const setActiveSurface = useWorkspaceStore((state) => state.setActiveSurface);
  const splitActive = useWorkspaceStore((state) => state.splitActive);
  const terminalSurface = resolveTerminalSurface(activeSurface, activeWorkspace?.surfaces ?? []);

  useEffect(() => {
    if (terminalSurface && activeSurface?.id !== terminalSurface.id) {
      setActiveSurface(terminalSurface.id);
    }
  }, [activeSurface?.id, setActiveSurface, terminalSurface]);

  const createTerminalSurface = () => {
    if (!activeWorkspace) {
      createWorkspace("Workspace", { layoutMode: "bsp" });
      return;
    }
    createSurface(activeWorkspace.id, { layoutMode: "bsp" });
  };

  const ensureTerminalSurface = () => {
    if (terminalSurface) {
      setActiveSurface(terminalSurface.id);
      return terminalSurface;
    }
    if (!activeWorkspace) {
      createWorkspace("Workspace", { layoutMode: "bsp" });
      return getActiveTerminalSurface();
    }
    const surfaceId = createSurface(activeWorkspace.id, { layoutMode: "bsp" });
    return getSurfaceById(surfaceId);
  };

  const splitTerminal = (direction: "horizontal" | "vertical") => {
    const surface = ensureTerminalSurface();
    if (!surface) return;
    setActiveSurface(surface.id);
    splitActive(direction, direction === "horizontal" ? "Right Terminal" : "Down Terminal");
  };

  return (
    <div className="zorai-tool-workbench">
      <div className="zorai-tool-actionbar">
        <div>
          <div className="zorai-section-label">Terminal surfaces</div>
          <strong>{terminalSurface ? terminalSurface.name : "No BSP terminal surface open"}</strong>
        </div>
        <div className="zorai-card-actions">
          <button type="button" className="zorai-primary-button zorai-tool-actionbar__primary" onClick={createTerminalSurface}>New terminal surface</button>
          <button type="button" className="zorai-ghost-button zorai-ghost-button--compact" onClick={() => splitTerminal("horizontal")}>Split right</button>
          <button type="button" className="zorai-ghost-button zorai-ghost-button--compact" onClick={() => splitTerminal("vertical")}>Split down</button>
          <CanvasCreateButton />
        </div>
      </div>

      <div className="zorai-terminal-tool">
        {terminalSurface && activeSurface?.id === terminalSurface.id ? (
          <LayoutContainer />
        ) : (
          <div className="zorai-tool-empty">
            <strong>No active terminal pane</strong>
            <span>Create a terminal surface or split the current surface to attach the terminal tool.</span>
          </div>
        )}
      </div>
    </div>
  );
}

function CanvasTool() {
  const activeWorkspace = useWorkspaceStore((state) => state.activeWorkspace());
  const activeSurface = useWorkspaceStore((state) => state.activeSurface());
  const createWorkspace = useWorkspaceStore((state) => state.createWorkspace);
  const createSurface = useWorkspaceStore((state) => state.createSurface);
  const setActiveSurface = useWorkspaceStore((state) => state.setActiveSurface);
  const createCanvasPanel = useWorkspaceStore((state) => state.createCanvasPanel);
  const arrangeCanvasPanels = useWorkspaceStore((state) => state.arrangeCanvasPanels);
  const canvasSurface = resolveCanvasSurface(activeSurface, activeWorkspace?.surfaces ?? []);

  const ensureCanvasSurface = () => {
    if (canvasSurface) {
      setActiveSurface(canvasSurface.id);
      return canvasSurface;
    }
    if (!activeWorkspace) {
      createWorkspace("Workspace", { layoutMode: "canvas" });
      return getActiveCanvasSurface();
    }
    const surfaceId = createSurface(activeWorkspace.id, { layoutMode: "canvas" });
    return getSurfaceById(surfaceId);
  };

  const createCanvasSurface = () => {
    if (!activeWorkspace) {
      createWorkspace("Workspace", { layoutMode: "canvas" });
      return;
    }
    createSurface(activeWorkspace.id, { layoutMode: "canvas" });
  };

  const createCanvasTerminal = () => {
    const surface = ensureCanvasSurface();
    if (surface) createCanvasPanel(surface.id, { paneName: "Terminal", paneIcon: "terminal" });
  };

  const createCanvasBrowser = () => {
    const surface = ensureCanvasSurface();
    if (surface) {
      createCanvasPanel(surface.id, {
        panelType: "browser",
        paneName: "Browser",
        paneIcon: "web",
        url: "https://google.com",
      });
    }
  };

  return (
    <div className="zorai-tool-workbench">
      <div className="zorai-tool-actionbar">
        <div>
          <div className="zorai-section-label">Infinite Canvas</div>
          <strong>{canvasSurface ? canvasSurface.name : "No canvas surface open"}</strong>
        </div>
        <div className="zorai-card-actions">
          <button type="button" className="zorai-primary-button zorai-tool-actionbar__primary" onClick={createCanvasSurface}>New infinite canvas</button>
          <button type="button" className="zorai-ghost-button zorai-ghost-button--compact" onClick={createCanvasTerminal}>Add terminal panel</button>
          <button type="button" className="zorai-ghost-button zorai-ghost-button--compact" onClick={createCanvasBrowser}>Add browser panel</button>
          <button type="button" className="zorai-ghost-button zorai-ghost-button--compact" onClick={() => canvasSurface && arrangeCanvasPanels(canvasSurface.id)} disabled={!canvasSurface}>Arrange canvas</button>
        </div>
      </div>

      <div className="zorai-canvas-tool">
        {canvasSurface ? (
          <InfiniteCanvasSurface surface={canvasSurface} />
        ) : (
          <div className="zorai-tool-empty">
            <strong>Start an infinite canvas</strong>
            <span>Create a canvas surface to arrange terminal and browser panels freely.</span>
          </div>
        )}
      </div>
    </div>
  );
}

function CanvasCreateButton() {
  const activeWorkspace = useWorkspaceStore((state) => state.activeWorkspace());
  const createWorkspace = useWorkspaceStore((state) => state.createWorkspace);
  const createSurface = useWorkspaceStore((state) => state.createSurface);

  const createCanvasSurface = () => {
    if (!activeWorkspace) {
      createWorkspace("Workspace", { layoutMode: "canvas" });
      return;
    }
    createSurface(activeWorkspace.id, { layoutMode: "canvas" });
  };

  return <button type="button" className="zorai-ghost-button zorai-ghost-button--compact" onClick={createCanvasSurface}>New infinite canvas</button>;
}

function resolveCanvasSurface(activeSurface: Surface | undefined, surfaces: Surface[]): Surface | null {
  if (activeSurface?.layoutMode === "canvas") return activeSurface;
  return surfaces.find((surface) => surface.layoutMode === "canvas") ?? null;
}

function resolveTerminalSurface(activeSurface: Surface | undefined, surfaces: Surface[]): Surface | null {
  if (activeSurface?.layoutMode === "bsp") return activeSurface;
  return surfaces.find((surface) => surface.layoutMode === "bsp") ?? null;
}

function getActiveCanvasSurface(): Surface | null {
  const surface = useWorkspaceStore.getState().activeSurface();
  return surface?.layoutMode === "canvas" ? surface : null;
}

function getActiveTerminalSurface(): Surface | null {
  const surface = useWorkspaceStore.getState().activeSurface();
  return surface?.layoutMode === "bsp" ? surface : null;
}

function getSurfaceById(surfaceId: string | null): Surface | null {
  if (!surfaceId) return null;
  return useWorkspaceStore.getState().workspaces
    .flatMap((workspace) => workspace.surfaces)
    .find((surface) => surface.id === surfaceId) ?? null;
}

function activeToolLabel(activeTool: ZoraiToolId): string {
  return zoraiTools.find((tool) => tool.id === activeTool)?.title ?? "Operator Tools";
}

function useEmbeddedToolState(activeTool: ZoraiToolId) {
  const fileManagerOpen = useWorkspaceStore((state) => state.fileManagerOpen);
  const commandLogOpen = useWorkspaceStore((state) => state.commandLogOpen);
  const sessionVaultOpen = useWorkspaceStore((state) => state.sessionVaultOpen);
  const systemMonitorOpen = useWorkspaceStore((state) => state.systemMonitorOpen);
  const webBrowserOpen = useWorkspaceStore((state) => state.webBrowserOpen);
  const setWebBrowserOpen = useWorkspaceStore((state) => state.setWebBrowserOpen);
  const setWebBrowserFullscreen = useWorkspaceStore((state) => state.setWebBrowserFullscreen);

  useEffect(() => {
    useWorkspaceStore.setState({
      fileManagerOpen: activeTool === "files",
      commandLogOpen: activeTool === "history",
      sessionVaultOpen: activeTool === "vault",
      systemMonitorOpen: activeTool === "system",
    });
    setWebBrowserOpen(activeTool === "browser");
    if (activeTool === "browser") setWebBrowserFullscreen(false);

    return () => {
      useWorkspaceStore.setState({
        fileManagerOpen: false,
        commandLogOpen: false,
        sessionVaultOpen: false,
        systemMonitorOpen: false,
      });
      setWebBrowserOpen(false);
    };
  }, [activeTool, setWebBrowserFullscreen, setWebBrowserOpen]);

  useEffect(() => {
    if (activeTool === "files" && !fileManagerOpen) {
      useWorkspaceStore.setState({ fileManagerOpen: true });
    }
    if (activeTool === "history" && !commandLogOpen) {
      useWorkspaceStore.setState({ commandLogOpen: true });
    }
    if (activeTool === "vault" && !sessionVaultOpen) {
      useWorkspaceStore.setState({ sessionVaultOpen: true });
    }
    if (activeTool === "system" && !systemMonitorOpen) {
      useWorkspaceStore.setState({ systemMonitorOpen: true });
    }
    if (activeTool === "browser" && !webBrowserOpen) {
      setWebBrowserOpen(true);
    }
  }, [
    activeTool,
    commandLogOpen,
    fileManagerOpen,
    sessionVaultOpen,
    setWebBrowserOpen,
    systemMonitorOpen,
    webBrowserOpen,
  ]);
}
