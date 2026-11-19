import { beforeEach, describe, expect, it, vi } from "vitest";
import { TINT_COLORS, localTintsToUpload, resetDaemonColorsAvailabilityForTest, syncTintsWithDaemon, threadTint, threadTintKey, tintColor, tintMenuItem, tintStyle, tintsFromDaemon, useTintStore } from "./tintStore";

describe("tints", () => {
  beforeEach(() => useTintStore.setState({ threads: {}, roots: {} }));

  it("keys a thread by its daemon id so the colour survives the local id changing after a reload", () => {
    expect(threadTintKey({ id: "local-1", daemonThreadId: "thread_abc" })).toBe("thread_abc");
    expect(threadTintKey({ id: "local-1", daemonThreadId: null })).toBe("local-1");
  });

  it("keeps a colour picked before the thread reached the daemon once the daemon id arrives", () => {
    expect(threadTint({ "local-1": "teal" }, { id: "local-1", daemonThreadId: "thread_abc" })).toBe("teal");
    expect(threadTint({ "local-1": "teal", thread_abc: "red" }, { id: "local-1", daemonThreadId: "thread_abc" })).toBe("red");
  });

  it("sets and clears thread and folder tints independently", () => {
    const { setThreadTint, setRootTint } = useTintStore.getState();
    setThreadTint("thread_abc", "teal");
    setRootTint("/repo", "pink");
    expect(useTintStore.getState()).toMatchObject({ threads: { thread_abc: "teal" }, roots: { "/repo": "pink" } });
    setThreadTint("thread_abc", null);
    expect(useTintStore.getState().threads).toEqual({});
    expect(useTintStore.getState().roots).toEqual({ "/repo": "pink" });
  });

  it("offers None plus every colour with a swatch, ticks the current one, and reports the pick", () => {
    const picks: Array<string | null> = [];
    const item = tintMenuItem("blue", (tint) => picks.push(tint));
    expect(item.children?.map((child) => child.key)).toEqual(["tint-none", ...Object.keys(TINT_COLORS).map((name) => `tint-${name}`)]);
    expect(item.children?.find((child) => child.checked)?.key).toBe("tint-blue");
    expect(item.children?.find((child) => child.key === "tint-green")?.swatch).toBe(TINT_COLORS.green);
    item.children?.find((child) => child.key === "tint-none")?.onSelect?.();
    item.children?.find((child) => child.key === "tint-red")?.onSelect?.();
    expect(picks).toEqual([null, "red"]);
  });

  it("ignores colour names a future or hand-edited store no longer knows", () => {
    expect(tintColor("chartreuse" as never)).toBeNull();
    expect(tintStyle(undefined)).toBeUndefined();
    expect(tintStyle("purple")).toEqual({ "--zorai-tint": TINT_COLORS.purple });
  });
});

describe("colours stored by the daemon", () => {
  it("drops unknown colour names the daemon returns instead of painting with them", () => {
    expect(tintsFromDaemon({ thread: { t1: "teal", t2: "chartreuse" }, root: { "/repo": "blue" } })).toEqual({
      threads: { t1: "teal" },
      roots: { "/repo": "blue" },
    });
  });

  it("copies colours picked before daemon storage up once, so they are not lost", () => {
    const local = { threads: { t1: "red" as const }, roots: { "/repo": "green" as const }, daemonSynced: false };
    expect(localTintsToUpload(local, { thread: {}, root: {} })).toEqual([
      { kind: "thread", key: "t1", tint: "red" },
      { kind: "root", key: "/repo", tint: "green" },
    ]);
  });

  it("never re-uploads stale local colours over what the daemon already holds or after the first sync", () => {
    const local = { threads: { t1: "red" as const }, roots: {}, daemonSynced: false };
    expect(localTintsToUpload(local, { thread: { t9: "blue" }, root: {} })).toEqual([]);
    expect(localTintsToUpload({ ...local, daemonSynced: true }, { thread: {}, root: {} })).toEqual([]);
  });

  it("asks the daemon to save a colour change so it follows the thread to other windows", () => {
    const agentSetUiColor = vi.fn(() => Promise.resolve({ thread: { t1: "purple" }, root: {} }));
    (globalThis as { window?: unknown }).window = { zorai: { agentSetUiColor } };
    useTintStore.getState().setThreadTint("t1", "purple");
    expect(agentSetUiColor).toHaveBeenCalledWith("thread", "t1", "purple");
    delete (globalThis as { window?: unknown }).window;
  });
});

describe("daemon without colour storage", () => {
  it("stops asking after the first failure so an older daemon does not keep dropping the connection", async () => {
    resetDaemonColorsAvailabilityForTest();
    const agentListUiColors = vi.fn(() => Promise.resolve({ error: "agent bridge exited" }));
    const agentSetUiColor = vi.fn(() => Promise.resolve({}));
    (globalThis as { window?: unknown }).window = { zorai: { agentListUiColors, agentSetUiColor } };
    await syncTintsWithDaemon();
    await syncTintsWithDaemon();
    useTintStore.getState().setThreadTint("t1", "red");
    expect(agentListUiColors).toHaveBeenCalledTimes(1);
    expect(agentSetUiColor).not.toHaveBeenCalled();
    expect(useTintStore.getState().threads.t1).toBe("red");
    delete (globalThis as { window?: unknown }).window;
    resetDaemonColorsAvailabilityForTest();
  });
});
