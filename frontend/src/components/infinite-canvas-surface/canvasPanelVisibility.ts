import type { CanvasPanel, CanvasState } from "@/lib/types";

const VIEWPORT_MARGIN_PX = 480;
const MIN_LIVE_SCREEN_PX = 160;
export const MAX_LIVE_CANVAS_PANES = 4;

type CanvasViewport = Pick<CanvasState, "panX" | "panY" | "zoomLevel">;
type ViewportSize = { width: number; height: number };
type PanelBox = Pick<CanvasPanel, "paneId" | "x" | "y" | "width" | "height">;

function zoomOf(canvasState: CanvasViewport): number {
  return canvasState.zoomLevel > 0 ? canvasState.zoomLevel : 1;
}

export function canvasPanelIntersectsViewport(
  panel: Pick<CanvasPanel, "x" | "y" | "width" | "height">,
  canvasState: CanvasViewport,
  viewport: ViewportSize,
): boolean {
  const zoom = zoomOf(canvasState);
  const margin = VIEWPORT_MARGIN_PX / zoom;
  const viewLeft = -canvasState.panX / zoom - margin;
  const viewTop = -canvasState.panY / zoom - margin;
  const viewRight = (-canvasState.panX + viewport.width) / zoom + margin;
  const viewBottom = (-canvasState.panY + viewport.height) / zoom + margin;
  return panel.x + panel.width >= viewLeft
    && panel.x <= viewRight
    && panel.y + panel.height >= viewTop
    && panel.y <= viewBottom;
}

export function canvasPanelScreenSize(
  panel: Pick<CanvasPanel, "width" | "height">,
  canvasState: CanvasViewport,
): { width: number; height: number } {
  const zoom = zoomOf(canvasState);
  return { width: panel.width * zoom, height: panel.height * zoom };
}

export function selectLiveCanvasPaneIds(
  panels: readonly PanelBox[],
  canvasState: CanvasViewport,
  viewport: ViewportSize,
  activePaneId: string | null,
): Set<string> {
  const live = new Set<string>();
  if (activePaneId) live.add(activePaneId);
  if (viewport.width <= 0 || viewport.height <= 0) return live;

  const candidates = panels
    .filter((panel) => panel.paneId !== activePaneId)
    .filter((panel) => canvasPanelIntersectsViewport(panel, canvasState, viewport))
    .map((panel) => ({ panel, screen: canvasPanelScreenSize(panel, canvasState) }))
    .filter((entry) => entry.screen.width >= MIN_LIVE_SCREEN_PX && entry.screen.height >= MIN_LIVE_SCREEN_PX)
    .sort((left, right) => (right.screen.width * right.screen.height) - (left.screen.width * left.screen.height));

  for (const entry of candidates) {
    if (live.size >= MAX_LIVE_CANVAS_PANES) break;
    live.add(entry.panel.paneId);
  }
  return live;
}
