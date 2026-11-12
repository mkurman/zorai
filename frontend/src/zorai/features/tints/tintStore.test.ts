import { beforeEach, describe, expect, it } from "vitest";
import { TINT_COLORS, threadTint, threadTintKey, tintColor, tintMenuItem, tintStyle, useTintStore } from "./tintStore";

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
