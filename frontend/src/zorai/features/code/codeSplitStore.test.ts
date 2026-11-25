import { beforeEach, describe, expect, it } from "vitest";
import { splitPaneIds } from "@/zorai/features/threads/threadSplitModel";
import { useCodeSplitStore } from "./codeSplitStore";

describe("code editor split store", () => {
  beforeEach(() => useCodeSplitStore.getState().clear());

  it("splits the active editor when a tab is dropped beside it and caps panes like thread splits", () => {
    const store = useCodeSplitStore.getState();
    store.insert("a.ts", "b.ts", "right");
    expect(splitPaneIds(useCodeSplitStore.getState().root)).toEqual(["a.ts", "b.ts"]);
    store.insertAtEdge("c.ts", "left");
    store.insert("a.ts", "d.ts", "bottom");
    store.insert("a.ts", "e.ts", "bottom");
    expect(splitPaneIds(useCodeSplitStore.getState().root).sort()).toEqual(["a.ts", "b.ts", "c.ts", "d.ts"]);
  });

  it("opening another file swaps it into the focused pane instead of growing the split", () => {
    const store = useCodeSplitStore.getState();
    store.insert("a.ts", "b.ts", "right");
    store.replacePane("b.ts", "c.ts");
    expect(splitPaneIds(useCodeSplitStore.getState().root)).toEqual(["a.ts", "c.ts"]);
    store.replacePane("c.ts", "a.ts");
    expect(splitPaneIds(useCodeSplitStore.getState().root)).toEqual(["a.ts", "c.ts"]);
  });

  it("closing a pane down to one editor leaves split view", () => {
    const store = useCodeSplitStore.getState();
    store.insert("a.ts", "b.ts", "right");
    store.removePane("b.ts");
    expect(useCodeSplitStore.getState().root).toBeNull();
  });
});
