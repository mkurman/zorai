import { describe, expect, it, vi } from "vitest";
import { confirmWorkspaceDiscard, optimisticBulkGitStatus, runWorkspaceGitBulkMutation, runWorkspaceGitMutation } from "./workspaceGitActions";

describe("workspace Git action coordination", () => {
  it("awaits a file or hunk mutation before one coherent refresh", async () => {
    const events: string[] = [];
    const result = await runWorkspaceGitMutation(async () => { events.push("mutate"); return "status"; }, async () => { events.push("refresh"); });

    expect(result).toBe("status");
    expect(events).toEqual(["mutate", "refresh"]);
  });

  it("awaits all bulk mutations in order before refreshing once", async () => {
    const events: string[] = [];
    await runWorkspaceGitBulkMutation(["a", "b", "c"], async (path) => { events.push(`mutate:${path}`); }, async () => { events.push("refresh"); });

    expect(events).toEqual(["mutate:a", "mutate:b", "mutate:c", "refresh"]);
  });

  it("still refreshes when a mutation fails", async () => {
    const refresh = vi.fn(async () => undefined);
    await expect(runWorkspaceGitMutation(async () => { throw new Error("failed"); }, refresh)).rejects.toThrow("failed");
    expect(refresh).toHaveBeenCalledOnce();
  });

  it("attempts every bulk mutation, refreshes, and reports a partial failure", async () => {
    const events: string[] = [];
    await expect(runWorkspaceGitBulkMutation(["a", "b", "c"], async (path) => {
      events.push(`mutate:${path}`);
      if (path === "b") throw new Error("b failed");
    }, async () => { events.push("refresh"); })).rejects.toThrow("b failed");

    expect(events).toEqual(["mutate:a", "mutate:b", "mutate:c", "refresh"]);
  });

  it("requires explicit discard confirmation", () => {
    const confirm = vi.fn(() => false);
    expect(confirmWorkspaceDiscard("Discard changes?", confirm)).toBe(false);
    expect(confirm).toHaveBeenCalledWith("Discard changes?");
  });

  it("guards rapid bulk actions to strictly sequential mutations with a refresh only after the last item", async () => {
    vi.useFakeTimers();
    try {
      const delayMs = 25;
      const inFlight: string[] = [];
      const peakConcurrency: number[] = [];
      const refresh = vi.fn(async () => undefined);
      const paths = ["a.ts", "b.ts", "c.ts", "d.ts"];

      const bulk = runWorkspaceGitBulkMutation(paths, async (path) => {
        inFlight.push(path);
        peakConcurrency.push(inFlight.length);
        await new Promise((resolve) => setTimeout(resolve, delayMs));
        inFlight.pop();
      }, refresh);

      expect(inFlight).toEqual(["a.ts"]);
      expect(refresh).not.toHaveBeenCalled();

      for (let step = 1; step < paths.length; step += 1) {
        await vi.advanceTimersByTimeAsync(delayMs);
        expect(inFlight).toEqual([paths[step]]);
        expect(refresh).not.toHaveBeenCalled();
      }

      await vi.advanceTimersByTimeAsync(delayMs);
      await bulk;

      expect(peakConcurrency).toEqual([1, 1, 1, 1]);
      expect(refresh).toHaveBeenCalledTimes(1);
    } finally {
      vi.useRealTimers();
    }
  });
});

describe("optimistic bulk staging", () => {
  const entry = (path: string, indexStatus: string, worktreeStatus: string) => ({ path, indexStatus, worktreeStatus }) as ZoraiWorkspaceGitStatus;

  it("moves every picked file to staged right away so + All feels instant", () => {
    const before = [entry("new.ts", "?", "?"), entry("edit.ts", " ", "M"), entry("both.ts", "M", "M"), entry("gone.ts", " ", "D"), entry("other.ts", " ", "M")];
    expect(optimisticBulkGitStatus(before, "stage", ["new.ts", "edit.ts", "both.ts", "gone.ts"])).toEqual([
      entry("new.ts", "A", " "),
      entry("edit.ts", "M", " "),
      entry("both.ts", "M", " "),
      entry("gone.ts", "D", " "),
      entry("other.ts", " ", "M"),
    ]);
  });

  it("returns unstaged files to the state git would show, keeping new files untracked", () => {
    const before = [entry("new.ts", "A", " "), entry("edit.ts", "M", " "), entry("both.ts", "M", "M")];
    expect(optimisticBulkGitStatus(before, "unstage", ["new.ts", "edit.ts", "both.ts"])).toEqual([
      entry("new.ts", "?", "?"),
      entry("edit.ts", " ", "M"),
      entry("both.ts", " ", "M"),
    ]);
  });
});
