import { afterEach, describe, expect, it, vi } from "vitest";
import {
  consumeThreadHistoryScroll,
  fillThreadHistoryIfUnscrollable,
  resetThreadHistoryPagination,
  resetThreadHistoryScrollStateForTest,
  resolveThreadHistoryScrollAction,
  setFollowThreadHistoryBottom,
  shouldFollowThreadHistoryBottom,
  THREAD_HISTORY_OLDER_LOAD_DEBOUNCE_MS,
  resolveOlderThreadPageMessageOffset,
  threadHasOlderHistory,
} from "./threadHistoryScroll";

describe("resolveThreadHistoryScrollAction", () => {
  afterEach(() => {
    resetThreadHistoryScrollStateForTest();
  });

  it("does not load older history or trim when the thread still fits on screen", () => {
    const action = resolveThreadHistoryScrollAction({
      scrollTop: 0,
      scrollHeight: 400,
      clientHeight: 400,
    });

    expect(action).toBe("none");
    expect(shouldFollowThreadHistoryBottom()).toBe(true);
  });

  it("loads older history when the latest page fits on screen but older rows remain", () => {
    const action = resolveThreadHistoryScrollAction({
      scrollTop: 0,
      scrollHeight: 400,
      clientHeight: 400,
      hasOlderHistory: true,
    });

    expect(action).toBe("load-older");
    expect(shouldFollowThreadHistoryBottom()).toBe(true);
  });

  it("loads older history only when the operator has scrolled away from the latest window", () => {
    const action = resolveThreadHistoryScrollAction({
      scrollTop: 8,
      scrollHeight: 2000,
      clientHeight: 400,
    });

    expect(action).toBe("load-older");
    expect(shouldFollowThreadHistoryBottom()).toBe(false);
  });

  it("returns to bottom-follow mode without trimming loaded history", () => {
    setFollowThreadHistoryBottom(false);

    const action = resolveThreadHistoryScrollAction({
      scrollTop: 1576,
      scrollHeight: 2000,
      clientHeight: 400,
    });

    expect(action).toBe("none");
    expect(shouldFollowThreadHistoryBottom()).toBe(true);
  });
});

describe("consumeThreadHistoryScroll", () => {
  afterEach(() => {
    vi.useRealTimers();
    resetThreadHistoryScrollStateForTest();
  });

  it("never invokes a trim callback when expanded tool content reaches the bottom", () => {
    const trimLatest = vi.fn(() => true);
    const onTrimmed = vi.fn();

    consumeThreadHistoryScroll({
      scroller: makeScroller(9_600, 10_000, 400),
      ...({ trimLatest, onTrimmed } as any),
    });

    expect(trimLatest).not.toHaveBeenCalled();
    expect(onTrimmed).not.toHaveBeenCalled();
    expect(shouldFollowThreadHistoryBottom()).toBe(true);
  });

  it("debounces older loads and ignores extra top events while a fetch is in flight", async () => {
    vi.useFakeTimers();
    const scroller = makeScroller(8);
    let resolveLoad: (value: boolean) => void = () => {};
    const loadOlder = vi.fn(() => new Promise<boolean>((resolve) => {
      resolveLoad = resolve;
    }));

    consumeThreadHistoryScroll({ scroller, loadOlder });
    consumeThreadHistoryScroll({ scroller, loadOlder });
    expect(loadOlder).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(THREAD_HISTORY_OLDER_LOAD_DEBOUNCE_MS);
    expect(loadOlder).toHaveBeenCalledTimes(1);

    consumeThreadHistoryScroll({ scroller, loadOlder });
    await vi.advanceTimersByTimeAsync(THREAD_HISTORY_OLDER_LOAD_DEBOUNCE_MS);
    expect(loadOlder).toHaveBeenCalledTimes(1);

    resolveLoad(true);
    await Promise.resolve();
  });

  it("stops paging once an older fetch finds no new messages until the operator leaves the top", async () => {
    vi.useFakeTimers();
    const scroller = makeScroller(8);
    const loadOlder = vi.fn(async () => false);

    consumeThreadHistoryScroll({ scroller, loadOlder });
    await vi.advanceTimersByTimeAsync(THREAD_HISTORY_OLDER_LOAD_DEBOUNCE_MS);
    await Promise.resolve();
    await vi.advanceTimersByTimeAsync(0);

    consumeThreadHistoryScroll({ scroller, loadOlder });
    await vi.advanceTimersByTimeAsync(THREAD_HISTORY_OLDER_LOAD_DEBOUNCE_MS);
    expect(loadOlder).toHaveBeenCalledTimes(1);

    consumeThreadHistoryScroll({ scroller: makeScroller(200), loadOlder });
    consumeThreadHistoryScroll({ scroller, loadOlder });
    await vi.advanceTimersByTimeAsync(THREAD_HISTORY_OLDER_LOAD_DEBOUNCE_MS);
    expect(loadOlder).toHaveBeenCalledTimes(2);
  });

  it("loads one older page per scroll instead of walking the whole thread", async () => {
    vi.useFakeTimers();
    const scroller = makeScroller(0);
    const loadOlder = vi.fn()
      .mockResolvedValueOnce(true)
      .mockResolvedValueOnce(false);

    consumeThreadHistoryScroll({ scroller, loadOlder });
    await vi.advanceTimersByTimeAsync(THREAD_HISTORY_OLDER_LOAD_DEBOUNCE_MS);
    await vi.runAllTimersAsync();

    expect(loadOlder).toHaveBeenCalledTimes(1);
  });

  it("resets exhausted pagination when the active thread changes", async () => {
    vi.useFakeTimers();
    const scroller = makeScroller(8);
    const loadOlder = vi.fn(async () => false);

    consumeThreadHistoryScroll({ scroller, loadOlder });
    await vi.advanceTimersByTimeAsync(THREAD_HISTORY_OLDER_LOAD_DEBOUNCE_MS);
    await Promise.resolve();

    resetThreadHistoryPagination();
    consumeThreadHistoryScroll({ scroller, loadOlder });
    await vi.advanceTimersByTimeAsync(THREAD_HISTORY_OLDER_LOAD_DEBOUNCE_MS);

    expect(loadOlder).toHaveBeenCalledTimes(2);
  });
});

describe("resolveOlderThreadPageMessageOffset", () => {
  it("returns the daemon offset for the next older page", () => {
    expect(resolveOlderThreadPageMessageOffset({
      loadedMessageStart: 70,
      messageCount: 120,
      currentMessageCount: 50,
    })).toBe(50);
    expect(resolveOlderThreadPageMessageOffset({
      loadedMessageStart: null,
      loadedMessageEnd: 3008,
      messageCount: 3008,
      currentMessageCount: 100,
    })).toBe(100);
    expect(resolveOlderThreadPageMessageOffset({
      loadedMessageStart: 0,
      messageCount: 100,
      currentMessageCount: 100,
    })).toBe(null);
  });
});

describe("threadHasOlderHistory", () => {
  it("detects older rows from loaded start or total message count", () => {
    expect(threadHasOlderHistory({ loadedMessageStart: 70 })).toBe(true);
    expect(threadHasOlderHistory({ loadedMessageStart: 0, messageCount: 886, loadedMessageEnd: 886 }, 100)).toBe(true);
    expect(threadHasOlderHistory({ loadedMessageStart: 0, messageCount: 886, loadedMessageEnd: 886 }, 886)).toBe(false);
    expect(threadHasOlderHistory({ loadedMessageStart: 0, messageCount: 100, loadedMessageEnd: 100 }, 100)).toBe(false);
    expect(threadHasOlderHistory({ loadedMessageStart: null, messageCount: 886 }, 100)).toBe(true);
    expect(threadHasOlderHistory({ loadedMessageStart: null }, 0)).toBe(false);
    expect(threadHasOlderHistory(null)).toBe(false);
  });
});

describe("fillThreadHistoryIfUnscrollable", () => {
  afterEach(() => {
    vi.useRealTimers();
    resetThreadHistoryScrollStateForTest();
  });

  it("pages older history when the latest window cannot be scrolled", async () => {
    vi.useFakeTimers();
    const scroller = makeScroller(0, 400, 400);
    const loadOlder = vi.fn(async () => true);

    fillThreadHistoryIfUnscrollable({
      scroller,
      loadOlder,
      hasOlderHistory: true,
    });
    expect(loadOlder).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(THREAD_HISTORY_OLDER_LOAD_DEBOUNCE_MS);
    expect(loadOlder).toHaveBeenCalledTimes(1);
  });

  it("does not page when the operator can already swipe the loaded window", async () => {
    vi.useFakeTimers();
    const scroller = makeScroller(1600, 2000, 400);
    const loadOlder = vi.fn(async () => true);

    fillThreadHistoryIfUnscrollable({
      scroller,
      loadOlder,
      hasOlderHistory: true,
    });
    await vi.advanceTimersByTimeAsync(THREAD_HISTORY_OLDER_LOAD_DEBOUNCE_MS);

    expect(loadOlder).not.toHaveBeenCalled();
  });

  it("does not page when the database has no older rows", async () => {
    vi.useFakeTimers();
    const loadOlder = vi.fn(async () => true);

    fillThreadHistoryIfUnscrollable({
      scroller: makeScroller(0, 400, 400),
      loadOlder,
      hasOlderHistory: false,
    });
    await vi.advanceTimersByTimeAsync(THREAD_HISTORY_OLDER_LOAD_DEBOUNCE_MS);

    expect(loadOlder).not.toHaveBeenCalled();
  });
});

function makeScroller(scrollTop: number, scrollHeight = 2000, clientHeight = 400): HTMLElement {
  return { scrollTop, scrollHeight, clientHeight } as HTMLElement;
}
