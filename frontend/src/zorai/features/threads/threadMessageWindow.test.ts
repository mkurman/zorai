import { describe, expect, it } from "vitest";

import {
  resolveThreadMessageWindow,
  stackedThreadMessageHeight,
  threadMessageScrollCompensation,
} from "./threadMessageWindow";

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

describe("threadMessageScrollCompensation", () => {
  it("keeps the view still when an older message first mounts above it taller than estimated", () => {
    expect(threadMessageScrollCompensation({
      heights: [undefined, undefined, 100, 100],
      index: 1,
      previous: undefined,
      height: 1500,
      scrollTop: 400,
      gap: 0,
      estimatedHeight: 160,
    })).toBe(1340);
  });

  it("keeps compensating re-measured rows above the viewport", () => {
    expect(threadMessageScrollCompensation({
      heights: [100, 300, 100],
      index: 1,
      previous: 300,
      height: 250,
      scrollTop: 450,
      gap: 0,
      estimatedHeight: 160,
    })).toBe(-50);
  });

  it("leaves the scroll alone for a row that reaches into the viewport", () => {
    expect(threadMessageScrollCompensation({
      heights: [100, undefined, 100],
      index: 1,
      previous: undefined,
      height: 1500,
      scrollTop: 200,
      gap: 0,
      estimatedHeight: 160,
    })).toBe(0);
  });
});
