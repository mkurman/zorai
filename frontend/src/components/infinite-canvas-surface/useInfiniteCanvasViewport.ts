import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useWorkspaceStore } from "../../lib/workspaceStore";
import type { Surface } from "../../lib/types";
import type { CanvasContextMenuState, CanvasMarqueeState, CanvasPanelRecord } from "./types";

const CANVAS_GRID_SIZE = 32;
const MIN_CANVAS_ZOOM = 0.04;
const MAX_CANVAS_ZOOM = 2.2;

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

export function snapToGrid(value: number, gridSize: number): number {
  return Math.round(value / gridSize) * gridSize;
}

type UseInfiniteCanvasViewportOptions = {
  surface: Surface;
  panels: CanvasPanelRecord[];
  setContextMenu: React.Dispatch<React.SetStateAction<CanvasContextMenuState | null>>;
  setSelectedPaneIds: React.Dispatch<React.SetStateAction<string[]>>;
};

export function useInfiniteCanvasViewport({
  surface,
  panels,
  setContextMenu,
  setSelectedPaneIds,
}: UseInfiniteCanvasViewportOptions) {
  const viewportRef = useRef<HTMLDivElement | null>(null);
  const rafRef = useRef<number | null>(null);
  const panCleanupRef = useRef<(() => void) | null>(null);
  const isPanningRef = useRef(false);
  const suppressContextMenuRef = useRef(false);
  const suppressContextMenuUntilRef = useRef(0);
  const activePaneRef = useRef<string | null>(surface.activePaneId ?? null);
  const lastFocusRequestNonceRef = useRef<number>(surface.canvasState.focusRequestNonce ?? 0);
  const viewRef = useRef({
    panX: surface.canvasState.panX,
    panY: surface.canvasState.panY,
    zoomLevel: surface.canvasState.zoomLevel,
  });
  const [isPanning, setIsPanning] = useState(false);
  const [animateTransform, setAnimateTransform] = useState(false);
  const [ctrlHeld, setCtrlHeld] = useState(false);
  const [marquee, setMarquee] = useState<CanvasMarqueeState | null>(null);

  const setCanvasView = useWorkspaceStore((state) => state.setCanvasView);
  const setCanvasPreviousView = useWorkspaceStore((state) => state.setCanvasPreviousView);
  const setActivePaneId = useWorkspaceStore((state) => state.setActivePaneId);
  const clearActivePaneFocus = useWorkspaceStore((state) => state.clearActivePaneFocus);

  useEffect(() => {
    isPanningRef.current = isPanning;
  }, [isPanning]);

  useEffect(() => {
    activePaneRef.current = surface.activePaneId ?? null;
  }, [surface.activePaneId]);

  useEffect(() => {
    const validPaneIds = new Set(panels.map((panel) => panel.paneId));
    setSelectedPaneIds((current) => current.filter((paneId) => validPaneIds.has(paneId)));
  }, [panels, setSelectedPaneIds]);

  useEffect(() => {
    viewRef.current = {
      panX: surface.canvasState.panX,
      panY: surface.canvasState.panY,
      zoomLevel: surface.canvasState.zoomLevel,
    };
  }, [surface.canvasState.panX, surface.canvasState.panY, surface.canvasState.zoomLevel]);

  useEffect(() => () => {
    if (panCleanupRef.current) {
      panCleanupRef.current();
      panCleanupRef.current = null;
    }
    if (rafRef.current !== null) {
      window.cancelAnimationFrame(rafRef.current);
      rafRef.current = null;
    }
  }, []);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Control") setCtrlHeld(true);
    };
    const onKeyUp = (event: KeyboardEvent) => {
      if (event.key === "Control") setCtrlHeld(false);
    };
    const onBlur = () => setCtrlHeld(false);
    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("keyup", onKeyUp);
    window.addEventListener("blur", onBlur);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("keyup", onKeyUp);
      window.removeEventListener("blur", onBlur);
    };
  }, []);

  const queueViewUpdate = useCallback((nextPanX: number, nextPanY: number, nextZoom: number) => {
    viewRef.current = {
      panX: nextPanX,
      panY: nextPanY,
      zoomLevel: nextZoom,
    };
    if (rafRef.current !== null) {
      window.cancelAnimationFrame(rafRef.current);
    }
    rafRef.current = window.requestAnimationFrame(() => {
      setCanvasView(surface.id, {
        panX: nextPanX,
        panY: nextPanY,
        zoomLevel: nextZoom,
      });
      rafRef.current = null;
    });
  }, [setCanvasView, surface.id]);

  const centerOnPanel = useCallback((paneId: string, opts?: { storePrevious?: boolean; zoomIn?: boolean }) => {
    const viewport = viewportRef.current;
    if (!viewport) return;
    const panel = surface.canvasPanels.find((entry) => entry.paneId === paneId);
    if (!panel) return;

    if (opts?.storePrevious) {
      setCanvasPreviousView(surface.id, {
        panX: surface.canvasState.panX,
        panY: surface.canvasState.panY,
        zoomLevel: surface.canvasState.zoomLevel,
      });
    }

    const rect = viewport.getBoundingClientRect();
    const requestedZoom = opts?.zoomIn
      ? Math.max(surface.canvasState.zoomLevel * 1.18, 1.05)
      : surface.canvasState.zoomLevel;
    const nextZoom = clamp(requestedZoom, MIN_CANVAS_ZOOM, MAX_CANVAS_ZOOM);
    const panelCenterX = panel.x + panel.width / 2;
    const panelCenterY = panel.y + panel.height / 2;
    const nextPanX = rect.width / 2 - panelCenterX * nextZoom;
    const nextPanY = rect.height / 2 - panelCenterY * nextZoom;

    setAnimateTransform(true);
    queueViewUpdate(nextPanX, nextPanY, nextZoom);
    window.setTimeout(() => setAnimateTransform(false), 260);
    setActivePaneId(paneId);
  }, [
    queueViewUpdate,
    setActivePaneId,
    setCanvasPreviousView,
    surface.canvasPanels,
    surface.canvasState.panX,
    surface.canvasState.panY,
    surface.canvasState.zoomLevel,
    surface.id,
  ]);

  useEffect(() => {
    if (!surface.activePaneId) return;
    const currentNonce = surface.canvasState.focusRequestNonce ?? 0;
    if (currentNonce === 0 || currentNonce === lastFocusRequestNonceRef.current) return;
    lastFocusRequestNonceRef.current = currentNonce;
    centerOnPanel(surface.activePaneId);
  }, [centerOnPanel, surface.activePaneId, surface.canvasState.focusRequestNonce]);

  const beginPan = useCallback((startX: number, startY: number, pointerId: number, target: HTMLElement, fromRightButton: boolean) => {
    if (panCleanupRef.current) {
      panCleanupRef.current();
      panCleanupRef.current = null;
    }

    setContextMenu(null);
    if (fromRightButton) {
      suppressContextMenuRef.current = true;
    }

    const startPanX = viewRef.current.panX;
    const startPanY = viewRef.current.panY;
    const startZoom = viewRef.current.zoomLevel;
    setIsPanning(true);
    isPanningRef.current = true;

    const suppressContextMenu = (event: Event) => {
      event.preventDefault();
      event.stopPropagation();
    };

    const onMove = (moveEvent: PointerEvent) => {
      const dx = moveEvent.clientX - startX;
      const dy = moveEvent.clientY - startY;
      queueViewUpdate(startPanX + dx, startPanY + dy, startZoom);
    };

    const onStop = () => {
      setIsPanning(false);
      isPanningRef.current = false;
      window.removeEventListener("pointermove", onMove, true);
      window.removeEventListener("pointerup", onStop, true);
      window.removeEventListener("pointercancel", onStop, true);
      window.removeEventListener("contextmenu", suppressContextMenu, true);
      if (fromRightButton) {
        window.setTimeout(() => {
          suppressContextMenuRef.current = false;
        }, 120);
      }
      panCleanupRef.current = null;
    };

    target.setPointerCapture(pointerId);
    window.addEventListener("pointermove", onMove, true);
    window.addEventListener("pointerup", onStop, true);
    window.addEventListener("pointercancel", onStop, true);
    window.addEventListener("contextmenu", suppressContextMenu, true);
    panCleanupRef.current = onStop;
  }, [queueViewUpdate, setContextMenu]);

  useEffect(() => {
    const viewport = viewportRef.current;
    if (!viewport) return;

    const onPointerDownCapture = (event: PointerEvent) => {
      const target = event.target as HTMLElement | null;
      const inToolbar = Boolean(target?.closest("[data-canvas-toolbar='true']"));
      const inPanel = Boolean(target?.closest("[data-canvas-panel='true']"));

      if (event.button === 0) {
        if (target?.closest("[data-canvas-toolbar='true'], [data-canvas-panel='true'], [data-canvas-menu='true']")) {
          return;
        }
        if (!event.ctrlKey && !event.metaKey) {
          setContextMenu(null);
          return;
        }

        event.preventDefault();
        event.stopPropagation();
        setContextMenu(null);

        const rect = viewport.getBoundingClientRect();
        const toWorld = (clientX: number, clientY: number) => {
          const current = viewRef.current;
          return {
            x: (clientX - rect.left - current.panX) / current.zoomLevel,
            y: (clientY - rect.top - current.panY) / current.zoomLevel,
          };
        };

        const worldStart = toWorld(event.clientX, event.clientY);
        setMarquee({
          startX: worldStart.x,
          startY: worldStart.y,
          currentX: worldStart.x,
          currentY: worldStart.y,
          additive: event.shiftKey,
        });

        const onMove = (moveEvent: PointerEvent) => {
          const worldPoint = toWorld(moveEvent.clientX, moveEvent.clientY);
          setMarquee((current) => (current ? { ...current, currentX: worldPoint.x, currentY: worldPoint.y } : current));
        };

        const onStop = () => {
          setMarquee((current) => {
            if (!current) return null;
            const minX = Math.min(current.startX, current.currentX);
            const minY = Math.min(current.startY, current.currentY);
            const maxX = Math.max(current.startX, current.currentX);
            const maxY = Math.max(current.startY, current.currentY);
            const nextSelection = panels
              .filter((panel) => (
                panel.x <= maxX
                && panel.x + panel.width >= minX
                && panel.y <= maxY
                && panel.y + panel.height >= minY
              ))
              .map((panel) => panel.paneId);

            setSelectedPaneIds((previous) => (
              current.additive ? Array.from(new Set([...previous, ...nextSelection])) : nextSelection
            ));
            return null;
          });
          window.removeEventListener("pointermove", onMove, true);
          window.removeEventListener("pointerup", onStop, true);
          window.removeEventListener("pointercancel", onStop, true);
        };

        window.addEventListener("pointermove", onMove, true);
        window.addEventListener("pointerup", onStop, true);
        window.addEventListener("pointercancel", onStop, true);
        return;
      }

      if (event.button === 2) {
        if (inToolbar || inPanel) return;

        setContextMenu(null);
        const startX = event.clientX;
        const startY = event.clientY;
        const pointerId = event.pointerId;
        let startedPan = false;

        const cleanupProbe = () => {
          window.removeEventListener("pointermove", onProbeMove, true);
          window.removeEventListener("pointerup", onProbeEnd, true);
          window.removeEventListener("pointercancel", onProbeEnd, true);
        };

        const onProbeMove = (moveEvent: PointerEvent) => {
          if (startedPan) return;
          if (Math.abs(moveEvent.clientX - startX) < 4 && Math.abs(moveEvent.clientY - startY) < 4) return;
          startedPan = true;
          suppressContextMenuUntilRef.current = Date.now() + 420;
          moveEvent.preventDefault();
          moveEvent.stopPropagation();
          cleanupProbe();
          beginPan(startX, startY, pointerId, viewport, true);
        };

        const onProbeEnd = () => {
          cleanupProbe();
        };

        window.addEventListener("pointermove", onProbeMove, true);
        window.addEventListener("pointerup", onProbeEnd, true);
        window.addEventListener("pointercancel", onProbeEnd, true);
        return;
      }

      if (event.button !== 1 || inToolbar) return;
      event.preventDefault();
      event.stopPropagation();
      beginPan(event.clientX, event.clientY, event.pointerId, viewport, false);
    };

    viewport.addEventListener("pointerdown", onPointerDownCapture, true);
    return () => {
      viewport.removeEventListener("pointerdown", onPointerDownCapture, true);
    };
  }, [beginPan, panels, setContextMenu, setSelectedPaneIds]);

  useEffect(() => {
    const viewport = viewportRef.current;
    if (!viewport) return;

    const onWheel = (event: WheelEvent) => {
      if (!event.ctrlKey) return;
      event.preventDefault();
      const activeElement = document.activeElement;
      if (activeElement instanceof HTMLElement && viewport.contains(activeElement)) {
        activeElement.blur();
      }
      if (activePaneRef.current) {
        clearActivePaneFocus(surface.id);
        activePaneRef.current = null;
      }
      const rect = viewport.getBoundingClientRect();
      const pointerX = event.clientX - rect.left;
      const pointerY = event.clientY - rect.top;
      const zoomFactor = event.deltaY > 0 ? 0.92 : 1.08;
      const current = viewRef.current;
      const nextZoom = clamp(current.zoomLevel * zoomFactor, MIN_CANVAS_ZOOM, MAX_CANVAS_ZOOM);
      const worldX = (pointerX - current.panX) / current.zoomLevel;
      const worldY = (pointerY - current.panY) / current.zoomLevel;
      const nextPanX = pointerX - worldX * nextZoom;
      const nextPanY = pointerY - worldY * nextZoom;
      queueViewUpdate(nextPanX, nextPanY, nextZoom);
    };

    viewport.addEventListener("wheel", onWheel, { passive: false });
    return () => viewport.removeEventListener("wheel", onWheel);
  }, [clearActivePaneFocus, queueViewUpdate, surface.id]);

  const handleCenterView = useCallback(() => {
    const viewport = viewportRef.current;
    if (!viewport || surface.canvasPanels.length === 0) return;
    const rect = viewport.getBoundingClientRect();
    const zoom = viewRef.current.zoomLevel;
    const minX = Math.min(...surface.canvasPanels.map((panel) => panel.x));
    const minY = Math.min(...surface.canvasPanels.map((panel) => panel.y));
    const maxX = Math.max(...surface.canvasPanels.map((panel) => panel.x + panel.width));
    const maxY = Math.max(...surface.canvasPanels.map((panel) => panel.y + panel.height));
    const centerX = (minX + maxX) / 2;
    const centerY = (minY + maxY) / 2;
    const nextPanX = rect.width / 2 - centerX * zoom;
    const nextPanY = rect.height / 2 - centerY * zoom;
    setAnimateTransform(true);
    queueViewUpdate(nextPanX, nextPanY, zoom);
    window.setTimeout(() => setAnimateTransform(false), 220);
  }, [queueViewUpdate, surface.canvasPanels]);

  const gridMetrics = useMemo(() => {
    const size = Math.max(8, CANVAS_GRID_SIZE * surface.canvasState.zoomLevel);
    const offsetX = (((surface.canvasState.panX - size / 2) % size) + size) % size;
    const offsetY = (((surface.canvasState.panY - size / 2) % size) + size) % size;
    return { size, offsetX, offsetY };
  }, [surface.canvasState.panX, surface.canvasState.panY, surface.canvasState.zoomLevel]);

  return {
    viewportRef,
    viewRef,
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
  };
}
