import { describe, expect, it } from "vitest";
import { buildReviewPrompt, extractSpawnedSubagentId, resolveReviewReport, waitForSubagentRun } from "./codeReview";

describe("waiting for the review subagent", () => {
  const spawnReply = "Spawned subagent task_42 with runtime daemon.\nReserved thread: thread_child\nDo not busy-wait on child status.";

  it("reads the subagent id from the spawn acknowledgement instead of treating it as the review", () => {
    expect(extractSpawnedSubagentId(spawnReply)).toBe("task_42");
    expect(extractSpawnedSubagentId("NO_ISSUES_FOUND")).toBeNull();
  });

  it("waits until the run finishes and exposes its thread while it is still running", async () => {
    const snapshots = [
      [{ id: "task_42", task_id: "task_42", status: "in_progress", thread_id: "thread_child" }],
      [{ id: "task_42", task_id: "task_42", status: "completed", thread_id: "thread_child", result: "BUG 1: off by one" }],
    ];
    const threads: string[] = [];
    const run = await waitForSubagentRun(async () => snapshots.shift() as never, "task_42", {
      onThread: (threadId) => threads.push(threadId),
      sleep: async () => undefined,
    });
    expect(run.result).toBe("BUG 1: off by one");
    expect(threads).toEqual(["thread_child"]);
  });

  it("gives up with a pointer to the thread instead of spinning forever", async () => {
    await expect(waitForSubagentRun(async () => [], "task_42", { intervalMs: 10, timeoutMs: 20, sleep: async () => undefined }))
      .rejects.toThrow("did not finish in time");
  });
});

describe("buildReviewPrompt", () => {
  it("names the commits instead of pasting their files and diffs, so the prompt stays small and the reviewer reads git itself", () => {
    const prompt = buildReviewPrompt({ root: "/repo", branch: "main", commits: [{ shortHash: "abc123", subject: "Fix parser" }, { shortHash: "def456", subject: "Add cache" }] });
    expect(prompt).toContain("Review the last 2 commits on branch main in /repo:");
    expect(prompt).toContain("- abc123 Fix parser");
    expect(prompt).not.toContain("diff");
    expect(prompt).toContain("NO_ISSUES_FOUND");
  });
});

describe("resolveReviewReport", () => {
  it("falls back to the agent's reply when the outcome summary only paraphrases the bugs", () => {
    const summary = "Completed review. Eight offline-verified findings are supplied in the final BUG-block report.";
    const reply = "BUG 1: Exam signatures collapse\nclinical_gym/schemas/action.py:56\n\nBUG 2: Commit control consumed";
    const report = resolveReviewReport([summary, reply]);
    expect(report?.findings.map((finding) => finding.title)).toEqual(["Exam signatures collapse", "Commit control consumed"]);
  });

  it("never reports a clean review when nothing contains findings or the no-issues marker", () => {
    expect(resolveReviewReport(["Review done, see notes.", "Working on it", null])).toBeNull();
  });

  it("accepts an explicit no-issues verdict", () => {
    expect(resolveReviewReport(["NO_ISSUES_FOUND"])).toEqual({ findings: [], raw: "NO_ISSUES_FOUND" });
  });
});
