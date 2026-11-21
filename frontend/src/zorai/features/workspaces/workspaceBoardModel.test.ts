import { describe, expect, it } from "vitest";
import type { WorkspaceTask } from "@/lib/workspaceBoard";
import {
  filterWorkspaceTasks,
  formatWorkspacePath,
  priorityTone,
  shortActorName,
  statusTone,
  workspaceAssigneeOptions,
  workspaceColumnTemplate,
} from "./workspaceBoardModel";

function fakeTask(id: string, status: WorkspaceTask["status"]): WorkspaceTask {
  return {
    id,
    workspace_id: "main",
    title: id,
    task_type: "thread",
    description: "",
    definition_of_done: null,
    status,
    priority: "normal",
    reporter: "user",
    assignee: null,
    reviewer: null,
    sort_order: 1,
    thread_id: null,
    goal_run_id: null,
    runtime_history: [],
    created_at: 1,
    updated_at: 1,
    deleted_at: null,
  };
}

describe("workspaceBoardModel", () => {
  it("normalizes double leading slashes and shortens home paths", () => {
    expect(formatWorkspacePath("//home/alice/.zorai/workspaces/main")).toBe("~/.zorai/workspaces/main");
    expect(formatWorkspacePath("/home/alice/project")).toBe("~/project");
    expect(formatWorkspacePath(null)).toBe("");
  });

  it("strips redundant agent/subagent prefixes for compact card chips", () => {
    expect(shortActorName({ agent: "svarog" })).toBe("svarog");
    expect(shortActorName({ subagent: "weles" })).toBe("weles");
    expect(shortActorName("user")).toBe("user");
    expect(shortActorName(null)).toBe("");
  });

  it("gives populated columns wider tracks than empty or collapsed columns", () => {
    const columns = [
      { status: "todo" as const, tasks: [] },
      { status: "in_progress" as const, tasks: [] },
      { status: "in_review" as const, tasks: [] },
      { status: "done" as const, tasks: [fakeTask("t1", "done")] },
    ];
    const template = workspaceColumnTemplate(columns, new Set(["todo"]));
    expect(template).toBe("minmax(52px, 52px) minmax(136px, 0.58fr) minmax(136px, 0.58fr) minmax(280px, 1.9fr)");
    expect(priorityTone("urgent")).toBe("danger");
    expect(statusTone("done")).toBe("success");
  });

  it("filters tasks by query, priority, type, and assignee", () => {
    const tasks: WorkspaceTask[] = [
      { ...fakeTask("t1", "done"), title: "Remote approval wedge", priority: "urgent", task_type: "thread", assignee: { agent: "svarog" } },
      { ...fakeTask("t2", "todo"), title: "Goal verification", priority: "high", task_type: "goal", assignee: null },
    ];
    expect(workspaceAssigneeOptions(tasks)).toEqual(["svarog"]);
    expect(filterWorkspaceTasks(tasks, { query: "approval", priority: "all", taskType: "all", assignee: "all" }).map((t) => t.id)).toEqual(["t1"]);
    expect(filterWorkspaceTasks(tasks, { query: "", priority: "high", taskType: "goal", assignee: "__unassigned__" }).map((t) => t.id)).toEqual(["t2"]);
    expect(filterWorkspaceTasks(tasks, { query: "", priority: "all", taskType: "all", assignee: "svarog" }).map((t) => t.id)).toEqual(["t1"]);
  });
});
