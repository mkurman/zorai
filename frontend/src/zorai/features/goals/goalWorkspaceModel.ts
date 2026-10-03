import type { AgentQueueTask } from "@/lib/agentTaskQueue";
import {
  formatGoalRunStatus,
  isGoalRunActive,
  latestGoalRunTodoSnapshot,
  type GoalRun,
  type GoalRunEvent,
  type TodoStatus,
} from "@/lib/goalRuns";

export type GoalWorkspaceMode = "work" | "review" | "activity" | "threads" | "files";

export type GoalWorkspaceTone = "normal" | "muted" | "active" | "success" | "warning" | "danger" | "accent";

export interface GoalWorkspaceRow {
  id: string;
  text: string;
  tone?: GoalWorkspaceTone;
  depth?: number;
  selected?: boolean;
  eventId?: string;
  targetThreadId?: string;
  targetFilePath?: string;
  progress?: number;
  working?: boolean;
  indicatorLabel?: string;
  meta?: string;
}

export interface GoalWorkspaceAction {
  id: "toggle" | "cancel" | "accept" | "soft_reject" | "hard_reject" | "refresh";
  label: string;
  enabled: boolean;
}

export interface GoalWorkspaceSection {
  title: string;
  rows: GoalWorkspaceRow[];
}

export interface GoalWorkspaceModel {
  summaryTitle: string;
  tabs: Array<{ id: GoalWorkspaceMode; label: string; active: boolean }>;
  summaryRows: GoalWorkspaceRow[];
  centerTitle: string;
  centerRows: GoalWorkspaceRow[];
  detailSections: GoalWorkspaceSection[];
  footerTitle: string;
  statusLabel: string;
  footerActions: GoalWorkspaceAction[];
}

export interface GoalWorkspaceOptions {
  mode?: GoalWorkspaceMode;
  promptExpanded?: boolean;
  selectedCenterIndex?: number;
  projectionFiles?: GoalProjectionFile[];
  tasks?: AgentQueueTask[];
}

export interface GoalProjectionFile {
  relativePath: string;
  absolutePath: string;
  sizeBytes?: number | null;
  source?: string | null;
}

export interface GoalWorkContextFile {
  path: string;
  repoRoot?: string | null;
  source?: string | null;
  goalRunId?: string | null;
  updatedAt?: number;
}

const modeTabs: Array<{ id: GoalWorkspaceMode; label: string; center: string }> = [
  { id: "work", label: "Work", center: "Worker progress" },
  { id: "review", label: "Review", center: "Supervisor review" },
  { id: "activity", label: "Activity", center: "Run activity" },
  { id: "threads", label: "Threads", center: "Threads" },
  { id: "files", label: "Files", center: "Files" },
];

export function buildGoalWorkspaceModel(run: GoalRun, options: GoalWorkspaceOptions = {}): GoalWorkspaceModel {
  const mode = options.mode ?? "work";
  const modeMeta = modeTabs.find((tab) => tab.id === mode) ?? modeTabs[0];
  const tasks = options.tasks ?? [];
  const selectedIndex = options.selectedCenterIndex ?? 0;
  const projectionFiles = options.projectionFiles ?? [];
  const centerRows = buildCenterRows(run, mode, selectedIndex, projectionFiles, tasks);

  return {
    summaryTitle: "Goal Mission Control",
    tabs: modeTabs.map((tab) => ({ id: tab.id, label: tab.label, active: tab.id === mode })),
    summaryRows: buildSummaryRows(run, Boolean(options.promptExpanded)),
    centerTitle: modeMeta.center,
    centerRows,
    detailSections: buildDetailSections(run, mode, selectedIndex, projectionFiles, centerRows),
    footerTitle: "Goal actions",
    statusLabel: statusLabel(run),
    footerActions: buildFooterActions(run),
  };
}

function buildSummaryRows(run: GoalRun, promptExpanded: boolean): GoalWorkspaceRow[] {
  const rows: GoalWorkspaceRow[] = [
    {
      id: "goal-prompt",
      text: "Goal prompt",
      meta: promptExpanded ? "Hide" : "Show",
      tone: "accent",
    },
  ];
  if (promptExpanded) {
    rows.push({ id: "goal-prompt-body", text: run.goal || "No goal prompt available.", tone: "muted", depth: 1 });
  }
  const worker = workerThread(run);
  rows.push(worker
    ? { id: "worker-thread", text: "Worker", meta: "Open thread", tone: "active", targetThreadId: worker }
    : { id: "worker-thread-empty", text: "No worker thread yet.", tone: "muted" });
  if (run.supervision_thread_id && run.supervision_thread_id !== run.thread_id) {
    rows.push({
      id: "owner-thread",
      text: "Owner",
      meta: "Open thread",
      tone: "accent",
      targetThreadId: run.supervision_thread_id,
    });
  }
  return rows;
}

function buildCenterRows(
  run: GoalRun,
  mode: GoalWorkspaceMode,
  selectedIndex: number,
  projectionFiles: GoalProjectionFile[],
  tasks: AgentQueueTask[],
): GoalWorkspaceRow[] {
  switch (mode) {
    case "work":
      return workRows(run, selectedIndex, tasks);
    case "review":
      return reviewRows(run, selectedIndex);
    case "activity":
      return timelineRows(run, selectedIndex);
    case "threads":
      return threadRows(run, selectedIndex, tasks);
    case "files":
      return fileRows(run, selectedIndex, projectionFiles);
  }
}

function buildDetailSections(
  run: GoalRun,
  mode: GoalWorkspaceMode,
  selectedIndex: number,
  projectionFiles: GoalProjectionFile[],
  centerRows: GoalWorkspaceRow[],
): GoalWorkspaceSection[] {
  if (mode === "files") return fileDetails(run, selectedIndex, projectionFiles);
  if (mode === "activity") return activityDetails(run, selectedIndex, centerRows);
  return [];
}

function workRows(run: GoalRun, selectedIndex: number, tasks: AgentQueueTask[]): GoalWorkspaceRow[] {
  const rows: GoalWorkspaceRow[] = [];
  const workerId = workerThread(run);
  const workerTasks = tasks.filter((task) => (
    task.source === "goal_run" || (workerId != null && task.thread_id === workerId)
  ));
  for (const task of workerTasks) {
    rows.push({
      id: `task-${task.id}`,
      text: task.title || "Goal worker",
      meta: taskActivityLabel(task),
      tone: taskTone(task),
      targetThreadId: task.thread_id ?? undefined,
      working: task.status === "in_progress",
      indicatorLabel: task.status === "in_progress" ? "Working" : task.status.replace(/_/g, " "),
      progress: task.progress,
    });
  }
  const todos = latestGoalRunTodoSnapshot(run);
  if (todos.length > 0 && workerTasks.length > 0) {
    rows.push({ id: "todos-label", text: "Worker todos", tone: "accent" });
  }
  for (const todo of todos) {
    rows.push({
      id: `todo-${todo.id}`,
      text: todo.content,
      meta: todoStatusLabel(todo.status),
      tone: todo.status === "completed" ? "success" : todo.status === "blocked" ? "danger" : todo.status === "in_progress" ? "active" : "muted",
      depth: workerTasks.length > 0 ? 1 : 0,
      indicatorLabel: todoStatusLabel(todo.status),
      progress: todo.status === "completed" ? 100 : todo.status === "in_progress" ? 50 : 0,
    });
  }
  if (run.last_error || run.error) {
    rows.push({ id: "work-error", text: run.last_error || run.error || "Error", tone: "danger" });
  }
  return markSelected(rows.length ? rows : [{ id: "empty", text: "Worker has not recorded progress yet.", tone: "muted" }], selectedIndex);
}

function reviewRows(run: GoalRun, selectedIndex: number): GoalWorkspaceRow[] {
  const rows: GoalWorkspaceRow[] = [];
  if (run.status === "awaiting_review") {
    rows.push({
      id: "pending-report",
      text: run.pending_review_report || "The worker asked for supervisor review.",
      meta: "Pending report",
      tone: "warning",
    });
  } else {
    rows.push({
      id: "review-idle",
      text: "No supervisor review is pending. Completeness is decided only by Accept.",
      tone: "muted",
    });
  }
  for (const event of (run.events ?? []).filter(isReviewEvent).slice(-12).reverse()) {
    rows.push({
      id: event.id,
      eventId: event.id,
      text: event.message || "review event",
      meta: event.phase,
      tone: eventTone(event),
    });
    if (event.details) {
      rows.push({ id: `${event.id}-details`, eventId: event.id, text: event.details, tone: "muted", depth: 1 });
    }
  }
  if (run.last_error) {
    rows.push({ id: "last-error", text: run.last_error, meta: "Last error", tone: "danger" });
  }
  return markSelected(rows, selectedIndex);
}

function activityDetails(run: GoalRun, selectedIndex: number, centerRows: GoalWorkspaceRow[]): GoalWorkspaceSection[] {
  const selected = centerRows[selectedIndex];
  const selectedEvent = selected?.eventId
    ? (run.events ?? []).find((event) => event.id === selected.eventId)
    : undefined;
  if (!selectedEvent) return [];
  const rows: GoalWorkspaceRow[] = [{ id: selectedEvent.id, text: selectedEvent.message, tone: "active" }];
  if (selectedEvent.details) rows.push({ id: `${selectedEvent.id}-details`, text: selectedEvent.details, tone: "muted" });
  for (const todo of selectedEvent.todo_snapshot ?? []) {
    rows.push({ id: `${selectedEvent.id}-${todo.id}`, text: todo.content, meta: todoStatusLabel(todo.status), tone: "muted" });
  }
  return [{ title: "Selected activity", rows }];
}

const MAX_TIMELINE_EVENTS = 40;
const MAX_EVENT_TODOS = 12;

function timelineRows(run: GoalRun, selectedIndex: number): GoalWorkspaceRow[] {
  const events = (run.events ?? []).slice(-MAX_TIMELINE_EVENTS).reverse();
  if (events.length === 0) return [{ id: "empty", text: "Waiting for run events.", tone: "muted" }];
  return events.flatMap((event) => eventRows(event)).map((row, index) => ({
    ...row,
    selected: index === selectedIndex,
  }));
}

function eventRows(event: GoalRunEvent): GoalWorkspaceRow[] {
  const rows: GoalWorkspaceRow[] = [{ id: event.id, eventId: event.id, text: event.message || "event", tone: eventTone(event) }];
  if (event.details) rows.push({ id: `${event.id}-details`, eventId: event.id, text: event.details, tone: "muted", depth: 1 });
  for (const todo of (event.todo_snapshot ?? []).slice(0, MAX_EVENT_TODOS)) {
    rows.push({ id: `${event.id}-${todo.id}`, eventId: event.id, text: todo.content, meta: todoStatusLabel(todo.status), tone: "muted", depth: 1 });
  }
  return rows;
}

function fileRows(run: GoalRun, selectedIndex: number, projectionFiles: GoalProjectionFile[]): GoalWorkspaceRow[] {
  if (projectionFiles.length > 0) {
    return projectionFiles.map((file, index) => ({
      id: `file-${file.absolutePath}`,
      text: file.relativePath,
      meta: file.source ? fileActivityLabel(file.source) : undefined,
      selected: index === selectedIndex,
      targetFilePath: file.absolutePath,
      tone: index === selectedIndex ? "accent" : "normal",
    }));
  }
  const entries: GoalWorkspaceRow[] = [
    ...(run.generated_skill_path ? [{
      id: "generated-skill",
      text: `Generated skill: ${run.generated_skill_path}`,
      targetFilePath: run.generated_skill_path,
    }] : []),
    ...((run.memory_updates ?? []).map((entry, index) => ({ id: `memory-${index}`, text: `Memory update: ${entry}`, tone: "muted" as const }))),
  ];
  return entries.length ? markSelected(entries, selectedIndex) : [{ id: "empty", text: "No goal files yet.", tone: "muted" }];
}

function fileDetails(run: GoalRun, selectedIndex: number, projectionFiles: GoalProjectionFile[]): GoalWorkspaceSection[] {
  const rows = fileRows(run, selectedIndex, projectionFiles);
  const selected = rows[selectedIndex] ?? rows[0];
  if (!selected || selected.id === "empty") return [{ title: "Selected file", rows }];
  const metadata: GoalWorkspaceRow[] = [selected];
  if (selected.targetFilePath) metadata.push({ id: `${selected.id}-path`, text: `Path ${selected.targetFilePath}`, tone: "muted" });
  const projection = projectionFiles.find((file) => file.absolutePath === selected.targetFilePath);
  if (typeof projection?.sizeBytes === "number") metadata.push({ id: `${selected.id}-size`, text: `Size ${projection.sizeBytes} bytes`, tone: "muted" });
  if (selected.targetFilePath) metadata.push({ id: `${selected.id}-open`, text: "Open preview", tone: "accent", targetFilePath: selected.targetFilePath });
  return [{ title: "Selected file", rows: metadata }];
}

function threadRows(run: GoalRun, selectedIndex: number, tasks: AgentQueueTask[]): GoalWorkspaceRow[] {
  const entries = goalThreadEntries(run, tasks);
  return markSelected(entries.length
    ? entries.map((entry) => ({ id: entry.threadId, text: entry.label, meta: "Open thread", tone: "accent" as const, targetThreadId: entry.threadId }))
    : [{ id: "empty", text: "No linked threads available.", tone: "muted" }], selectedIndex);
}

function buildFooterActions(run: GoalRun): GoalWorkspaceAction[] {
  const actions: GoalWorkspaceAction[] = [];
  if (isGoalRunActive(run)) {
    actions.push({
      id: "toggle",
      label: run.status === "paused" ? "Resume" : "Pause",
      enabled: true,
    });
    if (run.status !== "paused") {
      actions.push({ id: "cancel", label: "Stop", enabled: true });
    }
  }
  if (run.status === "awaiting_review") {
    actions.push({ id: "accept", label: "Accept", enabled: true });
    actions.push({ id: "soft_reject", label: "Soft reject", enabled: true });
    actions.push({ id: "hard_reject", label: "Hard reject", enabled: true });
  }
  actions.push({ id: "refresh", label: "Refresh", enabled: true });
  return actions;
}

export function goalFileThreadIds(run: GoalRun, tasks: AgentQueueTask[] = []): string[] {
  const ids: string[] = [];
  const push = (threadId?: string | null) => {
    if (threadId && !ids.includes(threadId)) ids.push(threadId);
  };
  push(workerThread(run));
  push(run.root_thread_id);
  push(run.active_thread_id);
  for (const threadId of run.execution_thread_ids ?? []) push(threadId);
  for (const task of tasks) push(task.thread_id);
  return ids;
}

export function goalFilesFromWorkContext(goalRunId: string, entries: GoalWorkContextFile[]): GoalProjectionFile[] {
  const seen = new Set<string>();
  const files: GoalProjectionFile[] = [];
  const ordered = [...entries].sort((left, right) => (right.updatedAt ?? 0) - (left.updatedAt ?? 0));
  for (const entry of ordered) {
    if (entry.goalRunId && entry.goalRunId !== goalRunId) continue;
    const absolutePath = absoluteWorkContextPath(entry);
    if (!absolutePath || seen.has(absolutePath)) continue;
    seen.add(absolutePath);
    files.push({
      relativePath: entry.repoRoot && !isAbsolutePath(entry.path) ? entry.path : absolutePath,
      absolutePath,
      sizeBytes: null,
      source: entry.source ?? null,
    });
  }
  return files.sort((left, right) => left.relativePath.localeCompare(right.relativePath));
}

export function mergeGoalWorkspaceFiles(
  projectionFiles: GoalProjectionFile[],
  touchedFiles: GoalProjectionFile[],
): GoalProjectionFile[] {
  const seen = new Set<string>();
  const merged: GoalProjectionFile[] = [];
  for (const file of [...touchedFiles, ...projectionFiles]) {
    if (!file.absolutePath || seen.has(file.absolutePath)) continue;
    seen.add(file.absolutePath);
    merged.push(file);
  }
  return merged.sort((left, right) => left.relativePath.localeCompare(right.relativePath));
}

function absoluteWorkContextPath(entry: GoalWorkContextFile): string {
  const path = entry.path.trim();
  if (!path) return "";
  if (!entry.repoRoot || isAbsolutePath(path)) return path;
  const root = entry.repoRoot.replace(/[/\\]+$/, "");
  const relative = path.replace(/^[/\\]+/, "");
  return relative ? `${root}/${relative}` : root;
}

function isAbsolutePath(path: string): boolean {
  return path.startsWith("/") || path.startsWith("\\") || /^[A-Za-z]:[\\/]/.test(path);
}

function fileActivityLabel(source: string): string {
  switch (source) {
    case "create_file":
      return "Created";
    case "write_file":
      return "Written";
    case "append_to_file":
      return "Appended";
    case "replace_in_file":
    case "apply_file_patch":
    case "apply_patch":
      return "Edited";
    default:
      return source.replace(/_/g, " ");
  }
}

function workerThread(run: GoalRun): string | null {
  return run.thread_id || run.active_thread_id || run.execution_thread_ids?.[0] || null;
}

function goalThreadEntries(run: GoalRun, tasks: AgentQueueTask[]): Array<{ label: string; threadId: string }> {
  const entries: Array<{ label: string; threadId: string }> = [];
  const push = (label: string, threadId?: string | null) => {
    if (threadId && !entries.some((entry) => entry.threadId === threadId)) entries.push({ label, threadId });
  };
  push("Worker", workerThread(run));
  if (run.supervision_thread_id && run.supervision_thread_id !== run.thread_id) {
    push("Owner", run.supervision_thread_id);
  }
  for (const task of tasks) {
    if (!task.thread_id || task.thread_id === run.thread_id || task.thread_id === run.supervision_thread_id) continue;
    push(task.title || "Spawned helper", task.thread_id);
  }
  return entries;
}

function statusLabel(run: GoalRun): string {
  if (run.status === "awaiting_review") return "Supervisor review";
  const worker = tasksWorkingLabel(run);
  return worker || formatGoalRunStatus(run.status);
}

function tasksWorkingLabel(run: GoalRun): string {
  const todos = latestGoalRunTodoSnapshot(run);
  const working = todos.find((todo) => todo.status === "in_progress");
  return working?.content ?? "";
}

function taskActivityLabel(task: AgentQueueTask): string {
  const status = task.status === "in_progress" ? `Working ${Math.max(0, Math.min(100, task.progress))}%` : task.status.replace(/_/g, " ");
  const activity = [...(task.logs ?? [])].reverse().find((entry) => entry.message.trim())?.message;
  return activity ? `${status} · ${activity}` : status;
}

function taskTone(task: AgentQueueTask): GoalWorkspaceTone {
  if (task.status === "failed" || task.status === "budget_exceeded") return "danger";
  if (task.status === "blocked" || task.status === "awaiting_approval" || task.status === "failed_analyzing") return "warning";
  if (task.status === "completed") return "success";
  if (task.status === "in_progress") return "active";
  return "muted";
}

function todoStatusLabel(status: TodoStatus): string {
  if (status === "in_progress") return "In progress";
  if (status === "completed") return "Done";
  if (status === "blocked") return "Blocked";
  return "Todo";
}

function isReviewEvent(event: GoalRunEvent): boolean {
  const haystack = `${event.phase} ${event.message}`.toLowerCase();
  return haystack.includes("review") || haystack.includes("accept") || haystack.includes("reject") || haystack.includes("supervisor");
}

function eventTone(event: GoalRunEvent): GoalWorkspaceTone {
  if (event.phase.includes("error") || event.message.toLowerCase().includes("failed")) return "danger";
  if (event.phase.includes("todo")) return "warning";
  if (isReviewEvent(event)) return "warning";
  return "normal";
}

function markSelected(rows: GoalWorkspaceRow[], selectedIndex: number): GoalWorkspaceRow[] {
  return rows.map((row, index) => ({ ...row, selected: index === selectedIndex }));
}
