import { describe, expect, it } from "vitest";
import type { AgentThread } from "@/lib/agentStore";
import { LAST_SESSION_STORAGE_KEY, readSavedSession, savedSessionFor } from "./sessionRestore";

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
});
