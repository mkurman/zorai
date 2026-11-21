import { create } from "zustand";
import { shortenHomePath } from "@/lib/workspaceStore";
import {
  actorLabel,
  type WorkspaceActor,
  type WorkspaceNotice,
  type WorkspacePriority,
  type WorkspaceTask,
  type WorkspaceTaskStatus,
  type WorkspaceTaskType,
} from "@/lib/workspaceBoard";

export type WorkspaceBoardViewMode = "board" | "list";
export type WorkspacePriorityTone = "danger" | "warning" | "neutral" | "muted";
export type WorkspaceTaskFilter = {
  query: string;
  priority: WorkspacePriority | "all";
  taskType: WorkspaceTaskType | "all";
  assignee: string;
};

export type WorkspaceSelectionState = {
  workspaceId: string;
  task: WorkspaceTask | null;
  notices: WorkspaceNotice[];
  onOpenRuntime: ((task: WorkspaceTask) => void) | null;
  onEdit: ((task: WorkspaceTask) => void) | null;
  onAction: ((task: WorkspaceTask, action: string) => void) | null;
  setSelection: (payload: {
    workspaceId: string;
    task: WorkspaceTask | null;
    notices: WorkspaceNotice[];
    onOpenRuntime: ((task: WorkspaceTask) => void) | null;
    onEdit: ((task: WorkspaceTask) => void) | null;
    onAction: ((task: WorkspaceTask, action: string) => void) | null;
  }) => void;
};

export const useWorkspaceSelectionStore = create<WorkspaceSelectionState>((set) => ({
  workspaceId: "main",
  task: null,
  notices: [],
  onOpenRuntime: null,
  onEdit: null,
  onAction: null,
  setSelection: (payload) => set(payload),
}));

export function formatWorkspacePath(root: string | null | undefined): string {
  if (!root?.trim()) return "";
  const cleaned = root.trim().replace(/^\/{2,}/, "/");
  return shortenHomePath(cleaned);
}

export function shortActorName(actor: WorkspaceActor): string {
  const label = actorLabel(actor);
  if (!label || label === "unassigned" || label === "none") return "";
  return label.replace(/^(?:agent|subagent):/i, "");
}

export function priorityTone(priority: WorkspacePriority): WorkspacePriorityTone {
  switch (priority) {
    case "urgent":
      return "danger";
    case "high":
      return "warning";
    case "low":
      return "muted";
    default:
      return "neutral";
  }
}

export function statusTone(status: WorkspaceTaskStatus): "muted" | "info" | "warning" | "success" {
  switch (status) {
    case "in_progress":
      return "info";
    case "in_review":
      return "warning";
    case "done":
      return "success";
    default:
      return "muted";
  }
}

export function workspaceAssigneeOptions(tasks: ReadonlyArray<WorkspaceTask>): string[] {
  const seen = new Set<string>();
  for (const task of tasks) {
    const name = shortActorName(task.assignee);
    if (name) seen.add(name);
  }
  return [...seen].sort((a, b) => a.localeCompare(b));
}

export function filterWorkspaceTasks(
  tasks: ReadonlyArray<WorkspaceTask>,
  filter: WorkspaceTaskFilter,
): WorkspaceTask[] {
  const needle = filter.query.trim().toLowerCase();
  return tasks.filter((task) => {
    if (filter.priority !== "all" && task.priority !== filter.priority) return false;
    if (filter.taskType !== "all" && task.task_type !== filter.taskType) return false;
    if (filter.assignee !== "all") {
      const name = shortActorName(task.assignee);
      if (filter.assignee === "__unassigned__") {
        if (name) return false;
      } else if (name !== filter.assignee) {
        return false;
      }
    }
    if (!needle) return true;
    const haystack = [
      task.title,
      task.description,
      task.definition_of_done ?? "",
      task.id,
      task.task_type,
      task.priority,
      actorLabel(task.assignee),
      actorLabel(task.reviewer),
    ]
      .join(" ")
      .toLowerCase();
    return haystack.includes(needle);
  });
}

export function workspaceColumnTemplate(
  columns: ReadonlyArray<{ status: WorkspaceTaskStatus; tasks: ReadonlyArray<WorkspaceTask> }>,
  collapsedEmpty: ReadonlySet<WorkspaceTaskStatus>,
): string {
  const populatedCount = columns.filter((col) => col.tasks.length > 0).length;
  if (populatedCount === 0 || populatedCount === columns.length) {
    return `repeat(${columns.length}, minmax(220px, 1fr))`;
  }
  return columns
    .map((col) => {
      if (col.tasks.length > 0) return "minmax(280px, 1.9fr)";
      if (collapsedEmpty.has(col.status)) return "minmax(52px, 52px)";
      return "minmax(136px, 0.58fr)";
    })
    .join(" ");
}
