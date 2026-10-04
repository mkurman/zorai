import { describe, expect, it } from "vitest";
import { canvasPanelIntersectsViewport, MAX_LIVE_CANVAS_PANES, selectLiveCanvasPaneIds } from "./canvasPanelVisibility";

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

describe("selectLiveCanvasPaneIds", () => {
  const panel = (paneId: string, x: number, y: number) => ({
    paneId, x, y, width: 480, height: 320,
  });

  it("keeps a readable on-screen panel live", () => {
    const live = selectLiveCanvasPaneIds(
      [panel("pane-a", 10, 10)],
      view,
      viewport,
      null,
    );
    expect(live.has("pane-a")).toBe(true);
  });

  it("does not start terminals for a zoomed-out canvas full of tiny panels", () => {
    const panels = Array.from({ length: 47 }, (_, index) => panel(`pane-${index}`, index * 40, 0));
    const live = selectLiveCanvasPaneIds(
      panels,
      { panX: 0, panY: 0, zoomLevel: 0.2 },
      viewport,
      "pane-0",
    );
    expect([...live]).toEqual(["pane-0"]);
  });

  it("caps live terminals when many panels are large enough to read", () => {
    const panels = Array.from({ length: 12 }, (_, index) => panel(`pane-${index}`, index * 20, 10));
    const live = selectLiveCanvasPaneIds(panels, view, viewport, "pane-0");
    expect(live.size).toBe(MAX_LIVE_CANVAS_PANES);
    expect(live.has("pane-0")).toBe(true);
  });
});
