import type { Surface } from "../../lib/types";

export type InfiniteCanvasSurfaceProps = {
  surface: Surface;
};

export type CanvasContextMenuState = {
  x: number;
  y: number;
  paneId: string;
};

export type CanvasMarqueeState = {
  startX: number;
  startY: number;
  currentX: number;
  currentY: number;
  additive: boolean;
};

export type CanvasPanelRecord = Surface["canvasPanels"][number];
