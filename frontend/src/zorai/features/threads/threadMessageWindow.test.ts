import { describe, expect, it } from "vitest";

import { resolveThreadMessageWindow, stackedThreadMessageHeight } from "./threadMessageWindow";

const heights = Array.from({ length: 12 }, () => 100);

describe("resolveThreadMessageWindow", () => {
  it("leaves rows above the viewport unmounted", () => {
    const window = resolveThreadMessageWindow({
      count: heights.length,
      heights,
      scrollTop: 540,
      viewport: 200,
      overscan: 0,
      gap: 0,
      estimatedHeight: 100,
    });

    expect(window.start).toBe(5);
    expect(window.end).toBe(8);
    expect(window.topSpacer).toBe(500);
    expect(stackedThreadMessageHeight(heights, 0, window.start, 0, 100)).toBe(window.topSpacer);
  });

  it("keeps a row that still intersects the viewport", () => {
    const window = resolveThreadMessageWindow({
      count: heights.length,
      heights,
      scrollTop: 150,
      viewport: 100,
      overscan: 0,
      gap: 0,
      estimatedHeight: 100,
    });

    expect(window.start).toBe(1);
    expect(window.end).toBe(3);
  });

  it("keeps the tail mounted while the thread is pinned to the bottom", () => {
    const window = resolveThreadMessageWindow({
      count: heights.length,
      heights,
      scrollTop: 0,
      viewport: 250,
      overscan: 0,
      gap: 0,
      estimatedHeight: 100,
      anchor: "bottom",
    });

    expect(window.end).toBe(heights.length);
    expect(window.start).toBe(9);
    expect(window.bottomSpacer).toBe(0);
  });

  it("puts the gap between hidden rows into the spacer so scroll height does not collapse", () => {
    const window = resolveThreadMessageWindow({
      count: 4,
      heights: [100, 100, 100, 100],
      scrollTop: 216,
      viewport: 100,
      overscan: 0,
      gap: 8,
      estimatedHeight: 100,
    });

    expect(window.start).toBe(2);
    expect(window.end).toBe(3);
    expect(window.topSpacer).toBe(208);
    expect(window.bottomSpacer).toBe(100);
  });

  it("mounts the whole thread when it fits in the viewport", () => {
    const window = resolveThreadMessageWindow({
      count: 2,
      heights: [80, 80],
      scrollTop: 0,
      viewport: 400,
      overscan: 0,
      gap: 8,
      estimatedHeight: 80,
    });

    expect(window).toEqual({ start: 0, end: 2, topSpacer: 0, bottomSpacer: 0 });
  });
});
