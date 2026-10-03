import type { CanvasPanel, CanvasState } from "@/lib/types";

const VIEWPORT_MARGIN_PX = 480;

export function canvasPanelIntersectsViewport(
  panel: Pick<CanvasPanel, "x" | "y" | "width" | "height">,
  canvasState: Pick<CanvasState, "panX" | "panY" | "zoomLevel">,
  viewport: { width: number; height: number },
): boolean {
  const zoom = canvasState.zoomLevel > 0 ? canvasState.zoomLevel : 1;
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
