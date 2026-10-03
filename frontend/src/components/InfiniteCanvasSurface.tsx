import { useEffect, useMemo, useRef, useState, type MouseEvent as ReactMouseEvent } from "react";
import { allLeafIds } from "../lib/bspTree";
import type { Surface } from "../lib/types";
import { useWorkspaceStore } from "../lib/workspaceStore";
import { TerminalPane } from "./TerminalPane";
import { CanvasBrowserPane } from "./web-browser-panel/CanvasBrowserPane";
import { CanvasMenus } from "./infinite-canvas-surface/CanvasMenus";
import { CanvasPanelShell } from "./infinite-canvas-surface/CanvasPanelShell";
import { CanvasToolbar } from "./infinite-canvas-surface/CanvasToolbar";
import type {
  CanvasContextMenuState,
  CanvasIconPickerState,
  InfiniteCanvasSurfaceProps,
} from "./infinite-canvas-surface/types";
import { canvasPanelIntersectsViewport } from "./infinite-canvas-surface/canvasPanelVisibility";
import { snapToGrid, useInfiniteCanvasViewport } from "./infinite-canvas-surface/useInfiniteCanvasViewport";
import { useInfiniteCanvasPaneActions } from "./infinite-canvas-surface/useInfiniteCanvasPaneActions";

const CANVAS_GRID_SIZE = 32;

export function InfiniteCanvasSurface({ surface }: InfiniteCanvasSurfaceProps) {
  const [snapEnabled, setSnapEnabled] = useState(true);
  const [selectedPaneIds, setSelectedPaneIds] = useState<string[]>([]);
  const [contextMenu, setContextMenu] = useState<CanvasContextMenuState | null>(null);
  const [iconPicker, setIconPicker] = useState<CanvasIconPickerState | null>(null);
  const [confirmClosePaneIds, setConfirmClosePaneIds] = useState<string[]>([]);
  const [showNewPanelMenu, setShowNewPanelMenu] = useState(false);
  const [viewportSize, setViewportSize] = useState({ width: 0, height: 0 });
  const dragGroupBaseRef = useRef<Map<string, { x: number; y: number }> | null>(null);

  const createCanvasPanel = useWorkspaceStore((state) => state.createCanvasPanel);
  const arrangeCanvasPanels = useWorkspaceStore((state) => state.arrangeCanvasPanels);
  const closePane = useWorkspaceStore((state) => state.closePane);
  const moveCanvasPanel = useWorkspaceStore((state) => state.moveCanvasPanel);
  const resizeCanvasPanel = useWorkspaceStore((state) => state.resizeCanvasPanel);
  const setCanvasPanelIcon = useWorkspaceStore((state) => state.setCanvasPanelIcon);
  const setCanvasPreviousView = useWorkspaceStore((state) => state.setCanvasPreviousView);
  const setActivePaneId = useWorkspaceStore((state) => state.setActivePaneId);
  const workspaces = useWorkspaceStore((state) => state.workspaces);

  const paneIds = useMemo(() => allLeafIds(surface.layout), [surface.layout]);
  const panelByPane = useMemo(() => new Map(surface.canvasPanels.map((panel) => [panel.paneId, panel])), [surface.canvasPanels]);
  const panels = useMemo(() => paneIds
    .map((paneId) => panelByPane.get(paneId))
    .filter((panel): panel is Surface["canvasPanels"][number] => Boolean(panel)), [paneIds, panelByPane]);
  const selectedPaneSet = useMemo(() => new Set(selectedPaneIds), [selectedPaneIds]);

  const {
    duplicatePaneIds,
    movePaneIdsToWorkspace,
    convertPaneIdsToBsp,
    requestClosePanes,
    resolveContextPaneIds,
    runSnippetForPanes,
    workspaceMoveTargets,
  } = useInfiniteCanvasPaneActions({
    surface,
    panelByPane,
    selectedPaneIds,
    selectedPaneSet,
    workspaces,
    setSelectedPaneIds,
    setContextMenu,
  });

  const {
    viewportRef,
    isPanning,
    isPanningRef,
    ctrlHeld,
    animateTransform,
    setAnimateTransform,
    marquee,
    suppressContextMenuRef,
    suppressContextMenuUntilRef,
    queueViewUpdate,
    centerOnPanel,
    handleCenterView,
    gridMetrics,
  } = useInfiniteCanvasViewport({
    surface,
    panels,
    setContextMenu,
    setIconPicker,
    setSelectedPaneIds,
  });

  useEffect(() => {
    const node = viewportRef.current;
    if (!node) return;
    const update = () => setViewportSize({ width: node.clientWidth, height: node.clientHeight });
    update();
    const observer = new ResizeObserver(update);
    observer.observe(node);
    return () => observer.disconnect();
  }, [viewportRef]);

  const livePaneIds = useMemo(() => {
    const live = new Set<string>();
    if (surface.activePaneId) live.add(surface.activePaneId);
    if (viewportSize.width <= 0 || viewportSize.height <= 0) return live;
    for (const panel of panels) {
      if (canvasPanelIntersectsViewport(panel, surface.canvasState, viewportSize)) {
        live.add(panel.paneId);
      }
    }
    return live;
  }, [panels, surface.activePaneId, surface.canvasState, viewportSize]);

  useEffect(() => {
    if (!contextMenu && !iconPicker && !showNewPanelMenu) return;
    const closeMenus = (event: MouseEvent) => {
      const target = event.target as HTMLElement | null;
      if (target?.closest("[data-canvas-menu='true']")) return;
      setContextMenu(null);
      setIconPicker(null);
      setShowNewPanelMenu(false);
    };
    window.addEventListener("mousedown", closeMenus);
    return () => window.removeEventListener("mousedown", closeMenus);
  }, [contextMenu, iconPicker, showNewPanelMenu]);

  const handleCreatePanel = () => {
    const createdPaneId = createCanvasPanel(surface.id);
    if (!createdPaneId) return;
    window.setTimeout(() => centerOnPanel(createdPaneId), 0);
    setShowNewPanelMenu(false);
  };

  const handleCreateBrowserPanel = () => {
    const createdPaneId = createCanvasPanel(surface.id, {
      panelType: "browser",
      paneIcon: "web",
      paneName: "Browser",
      url: "https://google.com",
    });
    if (!createdPaneId) return;
    window.setTimeout(() => centerOnPanel(createdPaneId), 0);
    setShowNewPanelMenu(false);
  };

  const selectPanel = (paneId: string, opts?: { toggle?: boolean; additive?: boolean; preserveIfAlreadySelected?: boolean }) => {
    const toggle = Boolean(opts?.toggle);
    const additive = Boolean(opts?.additive);
    const selected = selectedPaneSet.has(paneId);

    if (toggle) {
      setSelectedPaneIds((current) => (
        current.includes(paneId) ? current.filter((id) => id !== paneId) : [...current, paneId]
      ));
      return;
    }
    if (additive) {
      setSelectedPaneIds((current) => (current.includes(paneId) ? current : [...current, paneId]));
      return;
    }
    if (opts?.preserveIfAlreadySelected && selected && selectedPaneIds.length > 1) {
      return;
    }
    setSelectedPaneIds([paneId]);
  };

  const handlePanelActivate = (paneId: string, event: ReactMouseEvent<HTMLDivElement>) => {
    dragGroupBaseRef.current = null;
    const toggle = event.metaKey || event.ctrlKey;
    const additive = event.shiftKey;
    selectPanel(paneId, { toggle, additive });
    if (toggle || additive) return;
    setActivePaneId(paneId);
    setContextMenu(null);
    setIconPicker(null);
  };

  const handlePanelContextMenu = (paneId: string, event: ReactMouseEvent<HTMLDivElement>) => {
    event.preventDefault();
    dragGroupBaseRef.current = null;
    const toggle = event.metaKey || event.ctrlKey;
    const additive = event.shiftKey;
    selectPanel(paneId, { toggle, additive, preserveIfAlreadySelected: !toggle && !additive });
    setContextMenu({ paneId, x: event.clientX, y: event.clientY });
    setIconPicker(null);
  };

  const handlePanelMoveStart = (paneId: string) => {
    setContextMenu(null);
    setIconPicker(null);
    if (selectedPaneSet.has(paneId) && selectedPaneIds.length > 1) {
      dragGroupBaseRef.current = new Map(
        panels
          .filter((panel) => selectedPaneSet.has(panel.paneId))
          .map((panel) => [panel.paneId, { x: panel.x, y: panel.y }]),
      );
      return;
    }
    dragGroupBaseRef.current = null;
  };

  const handlePanelMove = (paneId: string, nextX: number, nextY: number) => {
    const basePositions = dragGroupBaseRef.current;
    if (basePositions && basePositions.has(paneId)) {
      const sourceBase = basePositions.get(paneId);
      if (!sourceBase) return;
      const dx = nextX - sourceBase.x;
      const dy = nextY - sourceBase.y;
      for (const selectedPaneId of selectedPaneIds) {
        const base = basePositions.get(selectedPaneId);
        if (!base) continue;
        moveCanvasPanel(selectedPaneId, base.x + dx, base.y + dy);
      }
      return;
    }
    moveCanvasPanel(paneId, nextX, nextY);
  };

  const handlePanelMoveEnd = (paneId: string, nextX: number, nextY: number) => {
    const basePositions = dragGroupBaseRef.current;
    if (basePositions && basePositions.has(paneId)) {
      if (snapEnabled) {
        for (const selectedPaneId of selectedPaneIds) {
          const panel = panelByPane.get(selectedPaneId);
          if (!panel) continue;
          moveCanvasPanel(
            selectedPaneId,
            snapToGrid(panel.x, CANVAS_GRID_SIZE),
            snapToGrid(panel.y, CANVAS_GRID_SIZE),
          );
        }
      }
      dragGroupBaseRef.current = null;
      return;
    }
    if (snapEnabled) {
      moveCanvasPanel(
        paneId,
        snapToGrid(nextX, CANVAS_GRID_SIZE),
        snapToGrid(nextY, CANVAS_GRID_SIZE),
      );
    }
  };

  const contextPaneIds = useMemo(() => {
    if (!contextMenu) return [];
    return resolveContextPaneIds(contextMenu.paneId);
  }, [contextMenu, resolveContextPaneIds]);

  return (
    <div
      ref={viewportRef}
      style={{
        width: "100%",
        height: "100%",
        position: "relative",
        overflow: "hidden",
        background: "radial-gradient(circle at top right, rgba(96, 165, 250, 0.09), transparent 58%), var(--bg-deep)",
        cursor: ctrlHeld ? "zoom-in" : isPanning ? "grabbing" : "grab",
      }}
      onContextMenu={(event) => {
        if (event.buttons === 2 || Date.now() < suppressContextMenuUntilRef.current || isPanningRef.current || suppressContextMenuRef.current) {
          event.preventDefault();
          event.stopPropagation();
          return;
        }
        const target = event.target as HTMLElement | null;
        if (!target?.closest("[data-canvas-panel='true'], [data-canvas-toolbar='true'], [data-canvas-menu='true']") && selectedPaneIds.length > 0) {
          event.preventDefault();
          setContextMenu({
            paneId: selectedPaneIds[0],
            x: event.clientX,
            y: event.clientY,
          });
        }
      }}
    >
      <div
        aria-hidden
        style={{
          position: "absolute",
          inset: 0,
          pointerEvents: "none",
          zIndex: 0,
          backgroundImage: snapEnabled ? "radial-gradient(circle, rgba(148, 163, 184, 0.26) 1px, transparent 1.2px)" : "none",
          backgroundSize: snapEnabled ? `${gridMetrics.size}px ${gridMetrics.size}px` : undefined,
          backgroundPosition: snapEnabled ? `${gridMetrics.offsetX}px ${gridMetrics.offsetY}px` : undefined,
        }}
      />

      <div
        style={{
          position: "absolute",
          inset: 0,
          zIndex: 1,
          transform: `translate(${surface.canvasState.panX}px, ${surface.canvasState.panY}px) scale(${surface.canvasState.zoomLevel})`,
          transformOrigin: "0 0",
          transition: animateTransform ? "transform 240ms ease" : "none",
        }}
      >
        {panels.map((panel) => (
          <CanvasPanelShell
            key={panel.paneId}
            panel={panel}
            zoomLevel={surface.canvasState.zoomLevel}
            active={panel.paneId === surface.activePaneId}
            selected={selectedPaneSet.has(panel.paneId)}
            ctrlHeld={ctrlHeld}
            onActivate={(event) => handlePanelActivate(panel.paneId, event)}
            onContextMenu={(event) => handlePanelContextMenu(panel.paneId, event)}
            onMoveStart={() => handlePanelMoveStart(panel.paneId)}
            onMove={(x, y) => handlePanelMove(panel.paneId, x, y)}
            onMoveEnd={(x, y) => handlePanelMoveEnd(panel.paneId, x, y)}
            onResize={(width, height) => resizeCanvasPanel(panel.paneId, width, height)}
            onDoubleClick={() => centerOnPanel(panel.paneId, { storePrevious: true, zoomIn: true })}
            onRequestClose={() => setConfirmClosePaneIds(requestClosePanes(panel.paneId))}
          >
            {livePaneIds.has(panel.paneId) ? (
              panel.panelType === "browser" ? (
                <CanvasBrowserPane paneId={panel.paneId} initialUrl={panel.url ?? "https://google.com"} />
              ) : (
                <TerminalPane paneId={panel.paneId} sessionId={panel.sessionId ?? undefined} hideHeader />
              )
            ) : null}
          </CanvasPanelShell>
        ))}
      </div>

      {marquee ? (
        <div
          aria-hidden
          style={{
            position: "absolute",
            zIndex: 25,
            pointerEvents: "none",
            left: Math.min(marquee.startX, marquee.currentX) * surface.canvasState.zoomLevel + surface.canvasState.panX,
            top: Math.min(marquee.startY, marquee.currentY) * surface.canvasState.zoomLevel + surface.canvasState.panY,
            width: Math.abs(marquee.currentX - marquee.startX) * surface.canvasState.zoomLevel,
            height: Math.abs(marquee.currentY - marquee.startY) * surface.canvasState.zoomLevel,
            border: "1px solid var(--accent)",
            background: "rgba(94, 231, 223, 0.14)",
          }}
        />
      ) : null}

      <CanvasToolbar
        showNewPanelMenu={showNewPanelMenu}
        snapEnabled={snapEnabled}
        hasPreviousView={Boolean(surface.canvasState.previousView)}
        onToggleNewPanelMenu={() => setShowNewPanelMenu((value) => !value)}
        onCreatePanel={handleCreatePanel}
        onCreateBrowserPanel={handleCreateBrowserPanel}
        onArrangePanels={() => arrangeCanvasPanels(surface.id)}
        onToggleSnap={() => setSnapEnabled((value) => !value)}
        onCenterView={handleCenterView}
        onRestorePreviousView={() => {
          setAnimateTransform(true);
          queueViewUpdate(
            surface.canvasState.previousView?.panX ?? 0,
            surface.canvasState.previousView?.panY ?? 0,
            surface.canvasState.previousView?.zoomLevel ?? 1,
          );
          setCanvasPreviousView(surface.id, null);
          window.setTimeout(() => setAnimateTransform(false), 260);
        }}
      />

      <CanvasMenus
        contextMenu={contextMenu}
        iconPicker={iconPicker}
        contextPaneIds={contextPaneIds}
        confirmClosePaneIds={confirmClosePaneIds}
        panelByPane={panelByPane}
        paneNames={surface.paneNames}
        workspaceMoveTargets={workspaceMoveTargets.map((workspace) => ({ id: workspace.id, name: workspace.name }))}
        onZoomIn={() => {
          if (contextPaneIds.length > 0) {
            centerOnPanel(contextPaneIds[0], { storePrevious: true, zoomIn: true });
          }
          setContextMenu(null);
        }}
        onRenamePanel={() => {
          const id = contextPaneIds[0];
          if (!id) return;
          const current = panelByPane.get(id);
          const name = prompt("Rename panel:", current?.title ?? "");
          if (name != null && name.trim()) {
            useWorkspaceStore.getState().renameCanvasPanel(id, name.trim());
          }
          setContextMenu(null);
        }}
        onDuplicatePanels={() => {
          void duplicatePaneIds(contextPaneIds);
          setContextMenu(null);
        }}
        onConvertToBsp={() => convertPaneIdsToBsp(contextPaneIds)}
        onRunSnippet={() => {
          runSnippetForPanes(contextPaneIds);
          setContextMenu(null);
        }}
        onOpenIconPicker={() => {
          if (!contextMenu) return;
          setIconPicker({ x: contextMenu.x, y: contextMenu.y, paneIds: contextPaneIds });
          setContextMenu(null);
        }}
        onMoveToWorkspace={(workspaceId) => movePaneIdsToWorkspace(contextPaneIds, workspaceId)}
        onRequestClosePanels={() => {
          if (contextMenu) {
            setConfirmClosePaneIds(requestClosePanes(contextMenu.paneId));
          }
          setContextMenu(null);
        }}
        onCloseIconPicker={() => setIconPicker(null)}
        onSetCanvasPanelIcon={setCanvasPanelIcon}
        onConfirmClosePanels={() => {
          if (confirmClosePaneIds.length === 0) return;
          for (const paneId of confirmClosePaneIds) {
            closePane(paneId);
          }
          setSelectedPaneIds((current) => current.filter((paneId) => !confirmClosePaneIds.includes(paneId)));
          setConfirmClosePaneIds([]);
        }}
        onCancelClosePanels={() => setConfirmClosePaneIds([])}
      />
    </div>
  );
}
