import { describe, expect, it, vi } from "vitest";
import { useAgentStore, type AgentThread } from "@/lib/agentStore";
import { LAST_SESSION_STORAGE_KEY, readSavedSession, restoredSplitFor, savedSessionFor, whenThreadsLoaded } from "./sessionRestore";
import { splitLayoutTree, splitPaneIds } from "./threadSplitModel";

const thread = (id: string, daemonThreadId: string | null) => ({ id, daemonThreadId }) as AgentThread;

describe("session restore", () => {
  it("saves daemon thread ids, because local ids are regenerated on every launch", () => {
    const threads = [thread("local-1", "thread-a"), thread("local-2", null), thread("local-3", "thread-c")];
    expect(savedSessionFor(threads, ["local-1", "local-2", "thread-c", "local-3"], "local-3")).toEqual({
      tabs: ["thread-a", "thread-c"],
      active: "thread-c",
    });
    expect(savedSessionFor(threads, [], "local-2").active).toBeNull();
  });

  it("ignores missing or corrupt saved state instead of breaking startup", () => {
    const storage = (value: string | null) => ({ getItem: () => value, setItem: () => {} });
    expect(readSavedSession(storage(null))).toEqual({ tabs: [], active: null });
    expect(readSavedSession(storage("{oops"))).toEqual({ tabs: [], active: null });
    expect(readSavedSession(storage(JSON.stringify({ tabs: ["thread-a", 4, ""], active: "thread-a" })))).toEqual({ tabs: ["thread-a"], active: "thread-a" });
    expect(LAST_SESSION_STORAGE_KEY).toBe("zorai-last-session");
  });

  it("brings back the split layout across launches, dropping threads that no longer exist", () => {
    const before = [thread("local-1", "thread-a"), thread("local-2", "thread-b"), thread("local-3", "thread-c")];
    const saved = savedSessionFor(before, [], "local-1", splitLayoutTree("main-bottom", ["local-1", "local-2", "local-3"]));
    expect(splitPaneIds(saved.split ?? null)).toEqual(["thread-a", "thread-b", "thread-c"]);
    const storage = { getItem: () => JSON.stringify(saved), setItem: () => {} };
    const restored = readSavedSession(storage).split;
    const afterRelaunch = [thread("new-1", "thread-a"), thread("new-3", "thread-c")];
    expect(restoredSplitFor(afterRelaunch, restored)).toEqual({
      kind: "split", direction: "column", children: [{ kind: "pane", id: "new-1" }, { kind: "pane", id: "new-3" }],
    });
    expect(restoredSplitFor([thread("new-1", "thread-a")], restored)).toBeNull();
    expect(readSavedSession({ getItem: () => JSON.stringify({ tabs: [], active: null, split: { kind: "split", children: "x" } }), setItem: () => {} }).split).toBeUndefined();
  });

  it("waits for the thread list before restoring, so a launch without a daemon connection does not throw the saved layout away", () => {
    useAgentStore.setState({ threads: [] });
    const restore = vi.fn();
    whenThreadsLoaded(restore, 60_000);
    expect(restore).not.toHaveBeenCalled();
    useAgentStore.setState({ threads: [thread("local-1", "thread-a")] });
    expect(restore).toHaveBeenCalledTimes(1);
    useAgentStore.setState({ threads: [thread("local-1", "thread-a"), thread("local-2", "thread-b")] });
    expect(restore).toHaveBeenCalledTimes(1);
  });
});
