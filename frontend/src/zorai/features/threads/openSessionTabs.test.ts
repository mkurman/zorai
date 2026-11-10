import { describe, expect, it } from "vitest";
import { moveOpenSessionId, neighborSessionId, rememberOpenSessionId, sessionCloseFollowUp } from "./openSessionTabs";

describe("open session tabs", () => {
  it("appends a new session and leaves the existing order in place when one is selected again", () => {
    const opened = rememberOpenSessionId(["a", "b"], "c");
    expect(opened).toEqual(["a", "b", "c"]);
    expect(rememberOpenSessionId(opened, "b")).toBe(opened);
    expect(rememberOpenSessionId(["a", "b", "c"], "b")).toEqual(["a", "b", "c"]);
  });

  it("clears the thread when the last open tab closes so the view is empty", () => {
    expect(sessionCloseFollowUp({
      closedId: "thread-a",
      activeThreadId: "thread-a",
      activeDaemonThreadId: "daemon-a",
      nextOpenId: null,
      openCount: 0,
    })).toEqual({ kind: "clear" });
  });

  it("keeps a hidden active thread on screen when the last visible Code tab closes", () => {
    expect(sessionCloseFollowUp({
      closedId: "code-thread",
      activeThreadId: "threads-session",
      activeDaemonThreadId: "daemon-threads-session",
      nextOpenId: null,
      openCount: 0,
    })).toEqual({ kind: "stay" });
  });

  it("follows the neighbor when an active tab closes and another session stays open", () => {
    expect(sessionCloseFollowUp({
      closedId: "thread-a",
      activeThreadId: "thread-a",
      activeDaemonThreadId: null,
      nextOpenId: "thread-b",
      openCount: 1,
    })).toEqual({ kind: "follow", id: "thread-b" });
  });

  it("leaves the visible thread alone when a background tab closes", () => {
    expect(sessionCloseFollowUp({
      closedId: "thread-b",
      activeThreadId: "thread-a",
      activeDaemonThreadId: null,
      nextOpenId: "thread-a",
      openCount: 1,
    })).toEqual({ kind: "stay" });
  });

  it("moves a dragged tab before or after the tab it is dropped on", () => {
    expect(moveOpenSessionId(["a", "b", "c", "d"], "a", "c", false)).toEqual(["b", "a", "c", "d"]);
    expect(moveOpenSessionId(["a", "b", "c", "d"], "a", "c", true)).toEqual(["b", "c", "a", "d"]);
    expect(moveOpenSessionId(["a", "b", "c", "d"], "d", "a", false)).toEqual(["d", "a", "b", "c"]);
  });

  it("keeps the same array when a drop would not change the order so the store does not re-render", () => {
    const ids = ["a", "b", "c"];
    expect(moveOpenSessionId(ids, "a", "b", false)).toBe(ids);
    expect(moveOpenSessionId(ids, "b", "b", true)).toBe(ids);
    expect(moveOpenSessionId(ids, "x", "b", true)).toBe(ids);
  });
});

describe("filtered session tabs", () => {
  it("moves to a neighbouring visible tab, so closing a Code project tab never jumps to a hidden Threads session", () => {
    const visible = ["code-a", "code-b", "code-c"];
    expect(neighborSessionId(visible, "code-b")).toBe("code-c");
    expect(neighborSessionId(visible, "code-c")).toBe("code-b");
    expect(neighborSessionId(["code-a"], "code-a")).toBeNull();
    expect(neighborSessionId(visible, "threads-only")).toBeNull();
  });
});
