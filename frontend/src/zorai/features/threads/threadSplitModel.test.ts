import { beforeEach, describe, expect, it } from "vitest";
import { insertSplitPane, insertSplitPaneAtEdge, nextSplitCandidate, resizeSplitChildren, setSplitSizesAt, splitChildSizes, removeSplitPaneNode, splitDropZone, splitEdgeDropZone, splitLayoutTree, splitLayoutsFor, splitPaneIds, useThreadSplitStore, type ThreadSplitNode } from "./threadSplitModel";

const pane = (id: string): ThreadSplitNode => ({ kind: "pane", id });

describe("thread split layouts", () => {
  it("only offers layouts that fit the number of picked threads", () => {
    expect(splitLayoutsFor(1)).toEqual([]);
    expect(splitLayoutsFor(2).map((option) => option.layout)).toEqual(["columns", "rows"]);
    expect(splitLayoutsFor(3).map((option) => option.layout)).toContain("main-bottom");
    expect(splitLayoutsFor(4)[0].layout).toBe("grid");
    expect(splitLayoutsFor(5)).toEqual([]);
  });

  it("drops a thread on the edge of the pane it was released over", () => {
    const rect = { left: 0, top: 0, width: 1000, height: 600 };
    expect(splitDropZone(rect, 100, 300)).toBe("left");
    expect(splitDropZone(rect, 900, 300)).toBe("right");
    expect(splitDropZone(rect, 500, 40)).toBe("top");
    expect(splitDropZone(rect, 500, 580)).toBe("bottom");
  });

  it("turns a menu layout into the matching pane tree", () => {
    expect(splitLayoutTree("main-bottom", ["a", "b", "c"])).toEqual({
      kind: "split", direction: "column", children: [{ kind: "split", direction: "row", children: [pane("a"), pane("b")] }, pane("c")],
    });
    expect(splitPaneIds(splitLayoutTree("grid", ["a", "b", "c", "d"]))).toEqual(["a", "b", "c", "d"]);
    expect(splitLayoutTree("columns", ["a"])).toBeNull();
  });
});

describe("dropping on the outer edge of a split", () => {
  it("targets the whole split only within the edge band, so pane drops still work inside", () => {
    const rect = { left: 0, top: 0, width: 1000, height: 600 };
    expect(splitEdgeDropZone(rect, 500, 590)).toBe("bottom");
    expect(splitEdgeDropZone(rect, 10, 300)).toBe("left");
    expect(splitEdgeDropZone(rect, 500, 500)).toBeNull();
    expect(splitEdgeDropZone(rect, 500, 620)).toBeNull();
  });

  it("puts the dropped thread full width below two side-by-side panes", () => {
    const sideBySide = insertSplitPane(null, "a", "b", "right");
    expect(insertSplitPaneAtEdge(sideBySide, "c", "bottom")).toEqual({
      kind: "split", direction: "column", children: [{ kind: "split", direction: "row", children: [pane("a"), pane("b")] }, pane("c")],
    });
  });

  it("joins an existing row instead of nesting, and respects the pane limit", () => {
    const sideBySide = insertSplitPane(null, "a", "b", "right");
    expect(insertSplitPaneAtEdge(sideBySide, "c", "left")).toEqual({ kind: "split", direction: "row", children: [pane("c"), pane("a"), pane("b")] });
    const full = splitLayoutTree("grid", ["a", "b", "c", "d"]);
    expect(insertSplitPaneAtEdge(full, "e", "bottom")).toBe(full);
  });
});

describe("splitting a single pane", () => {
  it("pairs the dropped thread with the one on screen, oriented by the drop edge", () => {
    expect(insertSplitPane(null, "a", "b", "left")).toEqual({ kind: "split", direction: "row", children: [pane("b"), pane("a")] });
    expect(insertSplitPane(null, "a", "b", "bottom")).toEqual({ kind: "split", direction: "column", children: [pane("a"), pane("b")] });
    expect(insertSplitPane(null, "a", "a", "right")).toBeNull();
  });

  it("splits only the pane under the drop, so two side by side can get one below the left", () => {
    const sideBySide = insertSplitPane(null, "a", "b", "right");
    expect(insertSplitPane(sideBySide, "a", "c", "bottom")).toEqual({
      kind: "split", direction: "row", children: [{ kind: "split", direction: "column", children: [pane("a"), pane("c")] }, pane("b")],
    });
  });

  it("adds a sibling instead of nesting when splitting along the same direction", () => {
    const sideBySide = insertSplitPane(null, "a", "b", "right");
    expect(insertSplitPane(sideBySide, "a", "c", "right")).toEqual({ kind: "split", direction: "row", children: [pane("a"), pane("c"), pane("b")] });
  });

  it("moves a pane that is already shown instead of duplicating it, and stops at four", () => {
    const three = insertSplitPane(insertSplitPane(null, "a", "b", "right"), "b", "c", "right");
    expect(splitPaneIds(insertSplitPane(three, "c", "a", "bottom"))).toEqual(["b", "c", "a"]);
    const four = insertSplitPane(three, "a", "d", "bottom");
    expect(insertSplitPane(four, "a", "e", "right")).toBe(four);
  });

  it("collapses empty branches when a pane closes", () => {
    const tree = insertSplitPane(insertSplitPane(null, "a", "b", "right"), "a", "c", "bottom");
    expect(removeSplitPaneNode(tree, "c")).toEqual({ kind: "split", direction: "row", children: [pane("a"), pane("b")] });
    expect(removeSplitPaneNode(removeSplitPaneNode(tree, "c"), "b")).toEqual(pane("a"));
  });
});

describe("thread split store", () => {
  beforeEach(() => useThreadSplitStore.getState().clear());

  it("falls back to a single thread when fewer than two panes remain", () => {
    useThreadSplitStore.getState().setLayout(["a", "b", "c"], "main-left");
    useThreadSplitStore.getState().removePane("c");
    expect(splitPaneIds(useThreadSplitStore.getState().root)).toEqual(["a", "b"]);
    useThreadSplitStore.getState().removePane("b");
    expect(useThreadSplitStore.getState().root).toBeNull();
  });

  it("opens a thread chosen elsewhere in the focused pane instead of hiding the split", () => {
    useThreadSplitStore.getState().setLayout(["a", "b", "c"], "columns");
    useThreadSplitStore.getState().setFocused("b");
    useThreadSplitStore.getState().replaceFocused("x");
    expect(splitPaneIds(useThreadSplitStore.getState().root)).toEqual(["a", "x", "c"]);
  });
});

describe("choosing a thread for Split Right / Split Down", () => {
  it("prefers an open tab, then falls back to the most recent thread when every tab is already on screen", () => {
    expect(nextSplitCandidate({ activeId: "a", shownIds: [], openTabIds: ["a", "b"], recentIds: ["c"] })).toBe("b");
    expect(nextSplitCandidate({ activeId: "a", shownIds: ["a", "b"], openTabIds: ["a", "b"], recentIds: ["b", "c"] })).toBe("c");
  });

  it("finds nothing when no other thread exists or four panes are already shown", () => {
    expect(nextSplitCandidate({ activeId: "a", shownIds: [], openTabIds: ["a"], recentIds: ["a"] })).toBeNull();
    expect(nextSplitCandidate({ activeId: "a", shownIds: ["a", "b", "c", "d"], openTabIds: [], recentIds: ["e"] })).toBeNull();
  });
});

describe("resizing split panes", () => {
  it("moves only the divider being dragged and keeps the pair's total", () => {
    expect(resizeSplitChildren([1, 1, 1], 0, 0.5)).toEqual([1.5, 0.5, 1]);
  });

  it("never squeezes a pane below the minimum, so it cannot vanish under the divider", () => {
    const [left, right] = resizeSplitChildren([1, 1], 0, 0.99);
    expect(right / (left + right)).toBeCloseTo(0.12);
    expect(resizeSplitChildren([1, 1], 0, -1)[0] / 2).toBeCloseTo(0.12);
  });

  it("stores sizes on the split at the dragged path and survives saving", () => {
    const tree = splitLayoutTree("main-left", ["a", "b", "c"]);
    const resized = setSplitSizesAt(tree, [1], [2, 1]);
    expect(resized).toMatchObject({ kind: "split", children: [pane("a"), { kind: "split", sizes: [2, 1] }] });
    expect(JSON.parse(JSON.stringify(resized))).toEqual(resized);
  });

  it("falls back to equal sizes when saved sizes no longer match the panes", () => {
    const resized = setSplitSizesAt(insertSplitPane(null, "a", "b", "right"), [], [3, 1]);
    const withThird = insertSplitPane(resized, "b", "c", "right");
    if (!withThird || withThird.kind !== "split") throw new Error("expected split");
    expect(splitChildSizes(withThird)).toEqual([1, 1, 1]);
    expect(splitChildSizes({ kind: "split", direction: "row", children: [pane("a"), pane("b")], sizes: [1, Number.NaN] })).toEqual([1, 1]);
  });
});
