import { describe, expect, it } from "vitest";
import { rememberOpenSessionId, sessionCloseFollowUp } from "./openSessionTabs";

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
});