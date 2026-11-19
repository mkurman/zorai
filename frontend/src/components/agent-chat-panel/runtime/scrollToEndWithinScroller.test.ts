import { afterEach, describe, expect, it, vi } from "vitest";
import { scrollToEndWithinScroller } from "./useAgentChatPanelProviderValue";

type FakeNode = { parentElement: FakeNode | null; overflowY: string; scrollHeight: number; clientHeight: number; scrollTop: number };

const node = (overflowY: string, parentElement: FakeNode | null, scrollHeight = 100, clientHeight = 100): FakeNode =>
  ({ parentElement, overflowY, scrollHeight, clientHeight, scrollTop: 0 });

afterEach(() => vi.unstubAllGlobals());

describe("scrolling a thread to its latest message", () => {
  it("moves only the message scroller, never the split pane around it, so focusing a pane does not nudge the layout", () => {
    vi.stubGlobal("getComputedStyle", (element: FakeNode) => ({ overflowY: element.overflowY }));
    const pane = node("hidden", null, 900, 800);
    const scroller = node("auto", pane, 5000, 700);
    const marker = node("visible", scroller);

    scrollToEndWithinScroller(marker as unknown as HTMLElement);

    expect(scroller.scrollTop).toBe(5000);
    expect(pane.scrollTop).toBe(0);
  });

  it("does nothing when no ancestor actually scrolls", () => {
    vi.stubGlobal("getComputedStyle", (element: FakeNode) => ({ overflowY: element.overflowY }));
    const shortList = node("auto", null, 300, 700);
    scrollToEndWithinScroller(node("visible", shortList) as unknown as HTMLElement);
    expect(shortList.scrollTop).toBe(0);
  });
});
