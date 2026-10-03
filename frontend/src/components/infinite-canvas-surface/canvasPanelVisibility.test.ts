import { describe, expect, it } from "vitest";
import { canvasPanelIntersectsViewport } from "./canvasPanelVisibility";

const view = { panX: 0, panY: 0, zoomLevel: 1 };
const viewport = { width: 800, height: 600 };

describe("canvasPanelIntersectsViewport", () => {
  it("keeps a panel inside the viewport mounted", () => {
    expect(canvasPanelIntersectsViewport(
      { x: 10, y: 10, width: 200, height: 120 },
      view,
      viewport,
    )).toBe(true);
  });

  it("skips a panel far outside the viewport so idle canvases do not keep every terminal alive", () => {
    expect(canvasPanelIntersectsViewport(
      { x: 8000, y: 8000, width: 200, height: 120 },
      view,
      viewport,
    )).toBe(false);
  });
});
