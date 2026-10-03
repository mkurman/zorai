import { describe, expect, it } from "vitest";
import type { GoalRun } from "@/lib/goalRuns";
import {
  buildGoalWorkspaceModel,
  goalFilesFromWorkContext,
  mergeGoalWorkspaceFiles,
} from "./goalWorkspaceModel";

const baseRun: GoalRun = {
  id: "goal-1",
  title: "Ship release",
  goal: "Pin the contract and cut the landing ledger",
  status: "running",
  created_at: 1,
  thread_id: "thread-main",
  supervision_thread_id: "thread-owner",
  root_thread_id: "thread-root",
  active_thread_id: "thread-main",
  execution_thread_ids: ["thread-main"],
  current_step_index: 0,
  replan_count: 0,
  child_task_count: 1,
  child_task_ids: ["task-a"],
  events: [
    {
      id: "event-1",
      timestamp: 1,
      phase: "todo",
      message: "goal todo updated",
      details: "Read harness/types.rs",
      step_index: 0,
      todo_snapshot: [
        { id: "todo-1", content: "Read harness/types.rs", status: "completed", position: 0, step_index: 0 },
        { id: "todo-2", content: "Land the ledger", status: "in_progress", position: 1, step_index: 0 },
      ],
    },
  ],
};

describe("goalWorkspaceModel", () => {
  it("exposes worker progress instead of planner steps", () => {
    const model = buildGoalWorkspaceModel(baseRun, { mode: "work" });

    expect(model.summaryTitle).toBe("Goal Mission Control");
    expect(model.tabs.map((tab) => tab.label)).toEqual([
      "Work",
      "Review",
      "Activity",
      "Threads",
      "Files",
    ]);
    expect(model.summaryRows.map((row) => row.text)).toEqual(["Goal prompt", "Worker", "Owner"]);
    expect(model.summaryRows.find((row) => row.id === "worker-thread")?.targetThreadId).toBe("thread-main");
    expect(model.centerTitle).toBe("Worker progress");
    expect(model.centerRows.map((row) => row.text)).toContain("Land the ledger");
    expect(model.centerRows.map((row) => row.text)).not.toContain("1. Pin the contract and cut the landing ledger");
    expect(model.footerTitle).toBe("Goal actions");
    expect(model.footerActions.map((action) => action.label)).toEqual(["Pause", "Stop", "Refresh"]);
  });

  it("surfaces the pending supervisor report on the review tab", () => {
    const model = buildGoalWorkspaceModel({
      ...baseRun,
      status: "awaiting_review",
      pending_review_report: "Ledger landed and tests passed.",
      events: [
        ...(baseRun.events ?? []),
        {
          id: "event-review",
          timestamp: 2,
          phase: "review",
          message: "worker requested supervisor review",
          todo_snapshot: [],
        },
      ],
    }, { mode: "review" });

    expect(model.centerRows[0]).toMatchObject({
      id: "pending-report",
      text: "Ledger landed and tests passed.",
    });
    expect(model.centerRows.map((row) => row.text)).toContain("worker requested supervisor review");
    expect(model.footerActions.map((action) => action.label)).toEqual([
      "Pause",
      "Stop",
      "Accept",
      "Soft reject",
      "Hard reject",
      "Refresh",
    ]);
  });

  it("keeps worker todos as the progress surface even when live worker tasks exist", () => {
    const model = buildGoalWorkspaceModel(baseRun, {
      mode: "work",
      tasks: [{
        id: "task-live",
        title: "Goal worker",
        description: "",
        status: "in_progress",
        priority: "high",
        progress: 42,
        created_at: 2,
        source: "goal_run",
        goal_run_id: "goal-1",
        thread_id: "thread-main",
        logs: [{ id: "log-1", timestamp: 3, level: "info", phase: "execution", message: "editing goal UI", attempt: 0 }],
      }],
    });

    expect(model.centerRows.find((row) => row.id === "task-task-live")).toMatchObject({
      text: "Goal worker",
      targetThreadId: "thread-main",
      meta: "Working 42% · editing goal UI",
    });
    expect(model.centerRows.find((row) => row.id === "todo-todo-2")?.text).toBe("Land the ledger");
  });

  it("maps a selected activity row back to its owning event", () => {
    const run: GoalRun = {
      ...baseRun,
      events: [
        ...(baseRun.events ?? []),
        {
          id: "event-with-hyphens",
          timestamp: 2,
          phase: "execution",
          message: "newest event",
          details: "newest event details",
          step_index: 0,
          todo_snapshot: [],
        },
      ],
    };
    const model = buildGoalWorkspaceModel(run, { mode: "activity", selectedCenterIndex: 1 });

    expect(model.centerRows[1]).toMatchObject({
      id: "event-with-hyphens-details",
      eventId: "event-with-hyphens",
      selected: true,
    });
    expect(model.detailSections.find((section) => section.title === "Selected activity")?.rows[0]).toMatchObject({
      id: "event-with-hyphens",
      text: "newest event",
    });
  });

  it("labels linked threads as worker and owner instead of planner roles", () => {
    const threads = buildGoalWorkspaceModel(baseRun, { mode: "threads" });
    expect(threads.centerRows.find((row) => row.targetThreadId === "thread-main")?.text).toBe("Worker");
    expect(threads.centerRows.find((row) => row.targetThreadId === "thread-owner")?.text).toBe("Owner");
  });

  it("marks projection files as actionable targets", () => {
    const files = buildGoalWorkspaceModel(baseRun, {
      mode: "files",
      projectionFiles: [
        {
          relativePath: "notes.md",
          absolutePath: "/home/example/.zorai/goals/goal-1/notes.md",
          sizeBytes: 42,
        },
      ],
    });
    expect(files.centerRows[0]).toMatchObject({
      text: "notes.md",
      targetFilePath: "/home/example/.zorai/goals/goal-1/notes.md",
    });
    expect(files.detailSections[0].rows.map((row) => row.text)).toContain("Size 42 bytes");
  });

  it("lists created and edited work-context files when the projection directory is empty", () => {
    const touched = goalFilesFromWorkContext("goal-1", [
      {
        path: "/repo/src/new-report.md",
        source: "create_file",
        goalRunId: "goal-1",
        updatedAt: 2,
      },
      {
        path: "src/existing.ts",
        repoRoot: "/repo",
        source: "apply_file_patch",
        goalRunId: null,
        updatedAt: 3,
      },
      {
        path: "/repo/src/existing.ts",
        source: "replace_in_file",
        updatedAt: 1,
      },
      {
        path: "/other/secret.txt",
        source: "create_file",
        goalRunId: "goal-2",
        updatedAt: 4,
      },
      {
        path: "   ",
        source: "create_file",
        updatedAt: 5,
      },
    ]);
    const files = buildGoalWorkspaceModel(baseRun, {
      mode: "files",
      projectionFiles: mergeGoalWorkspaceFiles([], touched),
    });

    expect(files.centerRows.map((row) => row.text)).toEqual([
      "/repo/src/new-report.md",
      "src/existing.ts",
    ]);
    expect(files.centerRows.map((row) => row.meta)).toEqual(["Created", "Edited"]);
    expect(files.centerRows.map((row) => row.targetFilePath)).toEqual([
      "/repo/src/new-report.md",
      "/repo/src/existing.ts",
    ]);
    expect(files.centerRows.map((row) => row.text)).not.toContain("/other/secret.txt");
  });
});
