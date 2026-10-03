import { FormEvent, MouseEvent, useCallback, useEffect, useMemo, useState } from "react";
import { RefreshButton } from "@/zorai/shell/RefreshButton";
import { useAgentChatPanelRuntime } from "@/components/agent-chat-panel/runtime/context";
import { useAgentStore } from "@/lib/agentStore";
import {
  actorFromText,
  actorLabel,
  chooseWorkspaceWithTasks,
  createWorkspaceTask,
  deleteWorkspaceTask,
  getWorkspaceSettings,
  latestNoticeSummaries,
  listWorkspaceNotices,
  listWorkspaceSettings,
  listWorkspaceTasks,
  moveWorkspaceTask,
  nextWorkspaceStatus,
  pauseWorkspaceTask,
  projectWorkspaceColumns,
  runWorkspaceTask,
  setWorkspaceOperator,
  stopWorkspaceTask,
  taskRunBlocked,
  updateWorkspaceTask,
  workspaceTasksNeedAutoRefresh,
  type WorkspaceActor,
  type WorkspaceNotice,
  type WorkspaceOperator,
  type WorkspacePriority,
  type WorkspaceSettings,
  type WorkspaceTask,
  type WorkspaceTaskStatus,
  type WorkspaceTaskType,
} from "@/lib/workspaceBoard";
import { navigateZorai } from "../../shell/zoraiNavigationEvents";
import { openThreadTarget } from "../threads/openThreadTarget";
import { CollapsibleSummary } from "../collapsibleSummary";
import { WorkspaceActorPickerControl } from "./WorkspaceActorPickerControl";
import { WorkspaceCreatePanel } from "./WorkspaceCreatePanel";

const WORKSPACE_SELECT_EVENT = "zorai-workspace-select";
const WORKSPACE_REFRESH_EVENT = "zorai-workspace-refresh";
const DEFAULT_WORKSPACE_ID = "main";

type TaskForm = {
  title: string;
  taskType: WorkspaceTaskType;
  description: string;
  definitionOfDone: string;
  priority: WorkspacePriority;
  assignee: string;
  reviewer: string;
};

type TaskOverlay = {
  taskId: string;
  mode: "details" | "edit";
};

const emptyForm: TaskForm = {
  title: "",
  taskType: "thread",
  description: "",
  definitionOfDone: "",
  priority: "normal",
  assignee: "",
  reviewer: "",
};

export function WorkspacesRail() {
  const [settings, setSettings] = useState<WorkspaceSettings[]>([]);
  const [selected, setSelected] = useState(DEFAULT_WORKSPACE_ID);
  const [createOpen, setCreateOpen] = useState(false);

  useEffect(() => {
    let cancelled = false;
    listWorkspaceSettings().then((items) => {
      if (!cancelled) setSettings(ensureMainWorkspace(items));
    });
    return () => {
      cancelled = true;
    };
  }, []);

  const selectWorkspace = (workspaceId: string) => {
    setSelected(workspaceId);
    window.dispatchEvent(new CustomEvent(WORKSPACE_SELECT_EVENT, { detail: { workspaceId } }));
  };

  const handleCreated = (workspace: WorkspaceSettings) => {
    setSettings((current) => {
      const items = ensureMainWorkspace(current);
      return items.some((entry) => entry.workspace_id === workspace.workspace_id)
        ? items.map((entry) => entry.workspace_id === workspace.workspace_id ? workspace : entry)
        : [...items, workspace];
    });
    selectWorkspace(workspace.workspace_id);
    setCreateOpen(false);
    void listWorkspaceSettings().then((items) => {
      setSettings((current) => {
        const next = ensureMainWorkspace(items);
        if (next.some((entry) => entry.workspace_id === workspace.workspace_id)) return next;
        const created = current.find((entry) => entry.workspace_id === workspace.workspace_id);
        return created ? [...next, created] : next;
      });
    });
  };

  return (
    <div className="zorai-rail-stack">
      {createOpen ? (
        <WorkspaceCreatePanel onCreated={handleCreated} onCancel={() => setCreateOpen(false)} />
      ) : (
        <button type="button" className="zorai-primary-button" onClick={() => setCreateOpen(true)}>
          + New workspace
        </button>
      )}
      <div className="zorai-section-label">Workspaces</div>
      {ensureMainWorkspace(settings).map((workspace) => (
        <button
          type="button"
          key={workspace.workspace_id}
          className={["zorai-thread-item", selected === workspace.workspace_id ? "zorai-thread-item--active" : ""].filter(Boolean).join(" ")}
          onClick={() => selectWorkspace(workspace.workspace_id)}
        >
          <span className="zorai-thread-title">{workspace.workspace_id}</span>
          <span className="zorai-thread-meta">
            operator: {workspace.operator} {workspace.workspace_root ? `/ ${workspace.workspace_root}` : ""}
          </span>
        </button>
      ))}
    </div>
  );
}

export function WorkspacesView() {
  const runtime = useAgentChatPanelRuntime();
  const [workspaceId, setWorkspaceId] = useState(DEFAULT_WORKSPACE_ID);
  const [operator, setOperator] = useState<WorkspaceOperator>("user");
  const [tasks, setTasks] = useState<WorkspaceTask[]>([]);
  const [notices, setNotices] = useState<WorkspaceNotice[]>([]);
  const [includeDeleted, setIncludeDeleted] = useState(false);
  const [expanded, setExpanded] = useState<Set<string>>(() => new Set());
  const [selectedTaskId, setSelectedTaskId] = useState<string | null>(null);
  const [taskOverlay, setTaskOverlay] = useState<TaskOverlay | null>(null);
  const [editForm, setEditForm] = useState<TaskForm>(emptyForm);
  const [formOpen, setFormOpen] = useState(false);
  const [form, setForm] = useState<TaskForm>(emptyForm);
  const [statusLine, setStatusLine] = useState("Workspace board ready.");
  const autoRefreshIntervalSecs = useAgentStore((state) => state.agentSettings.auto_refresh_interval_secs);

  const columns = useMemo(() => projectWorkspaceColumns(tasks, includeDeleted), [tasks, includeDeleted]);
  const noticeSummaries = useMemo(() => latestNoticeSummaries(notices), [notices]);
  const selectedTask = useMemo(() => tasks.find((task) => task.id === selectedTaskId) ?? null, [tasks, selectedTaskId]);
  const overlayTask = useMemo(() => tasks.find((task) => task.id === taskOverlay?.taskId) ?? null, [taskOverlay?.taskId, tasks]);

  const refresh = useCallback(async (nextWorkspaceId = workspaceId) => {
    const settingsList = ensureMainWorkspace(await listWorkspaceSettings());
    const taskEntries = await Promise.all(settingsList.map(async (workspace) => [
      workspace.workspace_id,
      await listWorkspaceTasks(workspace.workspace_id, includeDeleted),
    ] as const));
    const tasksByWorkspace = Object.fromEntries(taskEntries);
    const selectedWorkspaceId = chooseWorkspaceWithTasks({
      currentWorkspaceId: nextWorkspaceId,
      defaultWorkspaceId: DEFAULT_WORKSPACE_ID,
      workspaces: settingsList,
      tasksByWorkspace,
    });
    if (selectedWorkspaceId !== nextWorkspaceId) {
      setWorkspaceId(selectedWorkspaceId);
    }
    const taskList = tasksByWorkspace[selectedWorkspaceId] ?? [];
    const [settings, noticeList] = await Promise.all([
      getWorkspaceSettings(selectedWorkspaceId),
      listWorkspaceNotices(selectedWorkspaceId),
    ]);
    setOperator(settings?.operator ?? settingsList.find((workspace) => workspace.workspace_id === selectedWorkspaceId)?.operator ?? "user");
    setTasks(taskList);
    setNotices(noticeList);
    setStatusLine(`Loaded ${taskList.length} workspace tasks.`);
  }, [includeDeleted, workspaceId]);

  useEffect(() => {
    void refresh(workspaceId);
  }, [refresh, workspaceId]);

  useEffect(() => {
    const intervalSecs = Math.max(0, Math.trunc(Number(autoRefreshIntervalSecs) || 0));
    if (intervalSecs <= 0 || !workspaceTasksNeedAutoRefresh(tasks)) return;
    const timer = window.setInterval(() => void refresh(workspaceId), intervalSecs * 1000);
    return () => window.clearInterval(timer);
  }, [autoRefreshIntervalSecs, refresh, tasks, workspaceId]);

  useEffect(() => {
    const onSelect = (event: Event) => {
      const workspace = (event as CustomEvent<{ workspaceId?: string }>).detail?.workspaceId;
      if (workspace) setWorkspaceId(workspace);
    };
    window.addEventListener(WORKSPACE_SELECT_EVENT, onSelect);
    return () => window.removeEventListener(WORKSPACE_SELECT_EVENT, onSelect);
  }, []);

  useEffect(() => {
    const onRefresh = (event: Event) => {
      const refreshWorkspaceId = (event as CustomEvent<{ workspaceId?: string | null }>).detail?.workspaceId;
      if (!refreshWorkspaceId || refreshWorkspaceId === workspaceId) {
        void refresh(workspaceId);
      }
    };
    window.addEventListener(WORKSPACE_REFRESH_EVENT, onRefresh);
    return () => window.removeEventListener(WORKSPACE_REFRESH_EVENT, onRefresh);
  }, [refresh, workspaceId]);

  const toggleOperator = async () => {
    const next = operator === "user" ? "svarog" : "user";
    const settings = await setWorkspaceOperator(workspaceId, next);
    setOperator(settings?.operator ?? next);
    setStatusLine(`Switching workspace operator to ${next}.`);
  };

  const submitTask = async (event: FormEvent) => {
    event.preventDefault();
    if (!form.title.trim()) return;
    const task = await createWorkspaceTask({
      workspace_id: workspaceId,
      title: form.title.trim(),
      task_type: form.taskType,
      description: form.description.trim(),
      definition_of_done: form.definitionOfDone.trim() || null,
      priority: form.priority,
      assignee: actorFromText(form.assignee),
      reviewer: actorFromText(form.reviewer),
    });
    if (task) setTasks((items) => upsertTask(items, task));
    setForm(emptyForm);
    setFormOpen(false);
    setStatusLine("Created workspace task.");
    if (task) void refresh(workspaceId);
  };

  const actOnTask = async (task: WorkspaceTask, action: string) => {
    let updated: WorkspaceTask | null = null;
    if (action === "run") {
      if (taskRunBlocked(task)) {
        setStatusLine("Assign workspace task before running.");
        return;
      }
      updated = await runWorkspaceTask(task.id);
    } else if (action === "pause") updated = await pauseWorkspaceTask(task.id);
    else if (action === "stop") updated = await stopWorkspaceTask(task.id);
    else if (action === "move") updated = await moveWorkspaceTask(task.id, nextWorkspaceStatus(task.status), appendSortOrder(tasks, nextWorkspaceStatus(task.status)));
    else if (action === "review") updated = await moveWorkspaceTask(task.id, "in_review");
    else if (action === "assign") updated = await updateWorkspaceTask(task.id, { assignee: defaultActor(task.assignee) });
    else if (action === "reviewer") updated = await updateWorkspaceTask(task.id, { reviewer: defaultReviewer(task.reviewer) });
    else if (action === "delete") {
      const ok = await deleteWorkspaceTask(task.id);
      if (ok) setTasks((items) => items.filter((item) => item.id !== task.id));
      setStatusLine(ok ? "Deleted workspace task." : "Workspace task delete failed.");
      if (ok) await refresh(workspaceId);
      return;
    }
    if (updated) setTasks((items) => upsertTask(items, updated));
    setStatusLine(workspaceActionStatus(action));
    if (updated) await refresh(workspaceId);
  };

  const openTaskOverlay = (task: WorkspaceTask, mode: TaskOverlay["mode"]) => {
    setSelectedTaskId(task.id);
    if (mode === "edit") setEditForm(formFromTask(task));
    setTaskOverlay({ taskId: task.id, mode });
  };

  const saveTaskEdit = async (event: FormEvent) => {
    event.preventDefault();
    if (!overlayTask || !editForm.title.trim()) return;
    const updated = await updateWorkspaceTask(overlayTask.id, {
      title: editForm.title.trim(),
      description: editForm.description.trim(),
      definition_of_done: editForm.definitionOfDone.trim() || null,
      priority: editForm.priority,
      assignee: actorFromText(editForm.assignee),
      reviewer: actorFromText(editForm.reviewer),
    });
    if (updated) setTasks((items) => upsertTask(items, updated));
    setTaskOverlay(null);
    setStatusLine(updated ? "Updated workspace task." : "Workspace task update failed.");
    if (updated) void refresh(workspaceId);
  };

  const openTaskRuntime = async (task: WorkspaceTask) => {
    if (task.task_type === "goal") {
      const goalRunId = latestRuntimeGoalRunId(task);
      if (goalRunId) {
        navigateZorai({
          view: "goals",
          goalRunId,
          returnTarget: { view: "workspaces", label: "Return to workspace" },
        });
        return;
      }
    }

    const threadId = latestRuntimeThreadId(task);
    if (threadId) {
      const opened = await openThreadTarget(runtime, threadId);
      if (!opened) {
        setStatusLine(`Thread ${threadId} is not loaded yet.`);
        return;
      }
      navigateZorai({
        view: "threads",
        returnTarget: { view: "workspaces", label: "Return to workspace" },
      });
      return;
    }

    const goalRunId = latestRuntimeGoalRunId(task);
    if (goalRunId) {
      navigateZorai({
        view: "goals",
        goalRunId,
        returnTarget: { view: "workspaces", label: "Return to workspace" },
      });
      return;
    }

    setStatusLine("Workspace task has no linked thread or goal runtime yet.");
  };

  return (
    <section className="zorai-feature-surface zorai-workspace-board-surface">
      <div className="zorai-view-header">
        <div>
          <div className="zorai-kicker">Workspace</div>
          <h1>Workspace Board</h1>
          <p>Task orchestration from the TUI workspace board: columns, operators, notices, assignment, review, and runtime actions.</p>
        </div>
        <div className="zorai-card-actions">
          <button type="button" className="zorai-primary-button" onClick={() => setFormOpen((value) => !value)}>New task</button>
          <RefreshButton onClick={() => void refresh()} />
          <button type="button" className="zorai-ghost-button" onClick={toggleOperator}>Toggle operator: {operator}</button>
        </div>
      </div>

      <div className="zorai-workspace-toolbar">
        <span>Workspace {workspaceId}</span>
        <label>
          <input type="checkbox" checked={includeDeleted} onChange={(event) => setIncludeDeleted(event.target.checked)} />
          Show deleted
        </label>
        <span role="status" aria-live="polite">{statusLine}</span>
      </div>

      {formOpen ? <TaskCreateForm form={form} setForm={setForm} onSubmit={submitTask} /> : null}

      <div className="zorai-workspace-board">
        {columns.map((column) => (
          <section key={column.status} className="zorai-workspace-column">
            <div className="zorai-workspace-column__title">
              <span>{column.title}</span>
              <strong>{column.tasks.length}</strong>
            </div>
            {column.tasks.length === 0 ? <div className="zorai-empty-state">No tasks.</div> : null}
            {column.tasks.map((task) => (
              <TaskCard
                key={task.id}
                task={task}
                status={column.status}
                notice={noticeSummaries[task.id]}
                expanded={expanded.has(task.id)}
                selected={selectedTaskId === task.id}
                onSelect={() => setSelectedTaskId(task.id)}
                onToggle={() => setExpanded((items) => toggleSet(items, task.id))}
                onAction={(action) => void actOnTask(task, action)}
                onDetails={() => openTaskOverlay(task, "details")}
                onEdit={() => openTaskOverlay(task, "edit")}
                onOpenRuntime={() => void openTaskRuntime(task)}
              />
            ))}
          </section>
        ))}
      </div>

      {selectedTask ? <span className="zorai-workspace-selection">Selected: {selectedTask.title}</span> : null}
      {taskOverlay && overlayTask ? (
        <TaskModal
          task={overlayTask}
          mode={taskOverlay.mode}
          notices={notices.filter((notice) => notice.task_id === overlayTask.id)}
          editForm={editForm}
          setEditForm={setEditForm}
          onSave={saveTaskEdit}
          onClose={() => setTaskOverlay(null)}
          onEdit={() => {
            setEditForm(formFromTask(overlayTask));
            setTaskOverlay({ taskId: overlayTask.id, mode: "edit" });
          }}
          onOpenRuntime={() => void openTaskRuntime(overlayTask)}
        />
      ) : null}
    </section>
  );
}

function TaskCreateForm({ form, setForm, onSubmit }: { form: TaskForm; setForm: (form: TaskForm) => void; onSubmit: (event: FormEvent) => void }) {
  return (
    <form className="zorai-workspace-form" onSubmit={onSubmit}>
      <input value={form.title} onChange={(event) => setForm({ ...form, title: event.target.value })} placeholder="Task title" />
      <select value={form.taskType} onChange={(event) => setForm({ ...form, taskType: event.target.value as WorkspaceTaskType })}>
        <option value="thread">Thread</option>
        <option value="goal">Goal</option>
      </select>
      <select value={form.priority} onChange={(event) => setForm({ ...form, priority: event.target.value as WorkspacePriority })}>
        <option value="low">Low</option>
        <option value="normal">Normal</option>
        <option value="high">High</option>
        <option value="urgent">Urgent</option>
      </select>
      <WorkspaceActorPickerControl mode="assignee" value={form.assignee} onChange={(assignee) => setForm({ ...form, assignee })} />
      <WorkspaceActorPickerControl mode="reviewer" value={form.reviewer} onChange={(reviewer) => setForm({ ...form, reviewer })} />
      <textarea value={form.description} onChange={(event) => setForm({ ...form, description: event.target.value })} placeholder="Description" />
      <textarea value={form.definitionOfDone} onChange={(event) => setForm({ ...form, definitionOfDone: event.target.value })} placeholder="Definition of done" />
      <button type="submit" className="zorai-primary-button">Create task</button>
    </form>
  );
}

function TaskCard({ task, status, notice, expanded, selected, onSelect, onToggle, onAction, onDetails, onEdit, onOpenRuntime }: {
  task: WorkspaceTask;
  status: WorkspaceTaskStatus;
  notice?: string;
  expanded: boolean;
  selected: boolean;
  onSelect: () => void;
  onToggle: () => void;
  onAction: (action: string) => void;
  onDetails: () => void;
  onEdit: () => void;
  onOpenRuntime: () => void;
}) {
  return (
    <article className={["zorai-workspace-task", selected ? "zorai-workspace-task--active" : ""].filter(Boolean).join(" ")} onClick={onSelect}>
      <div className="zorai-run-card__header">
        <div>
          <strong>{task.title}</strong>
          <span>{task.task_type} / {task.priority} / {shortId(task.id)}</span>
        </div>
        <span className="zorai-status-pill">{status.replace("_", " ")}</span>
      </div>
      {task.description.trim() ? <CollapsibleSummary text={task.description} /> : null}
      {notice ? <CollapsibleSummary className="zorai-workspace-notice" text={notice} /> : null}
      <div className="zorai-workspace-task__meta">
        <span>assignee: {actorLabel(task.assignee)}</span>
        <span>reviewer: {actorLabel(task.reviewer)}</span>
      </div>
      <div className="zorai-card-actions">
        <button type="button" className="zorai-ghost-button" onClick={stopClick(onOpenRuntime)}>Open runtime</button>
        <button type="button" className="zorai-ghost-button" onClick={stopClick(onDetails)}>Details</button>
        <button type="button" className="zorai-ghost-button" onClick={stopClick(onEdit)}>Edit</button>
        <button type="button" className="zorai-ghost-button" onClick={stopClick(onToggle)}>{expanded ? "Hide actions" : "Actions"}</button>
      </div>
      {expanded ? (
        <div className="zorai-workspace-actions">
          <button type="button" className="zorai-ghost-button" disabled={taskRunBlocked(task)} onClick={stopClick(() => onAction("run"))}>{taskRunBlocked(task) ? "Blocked" : "Run"}</button>
          {["pause", "stop", "move", "review", "assign", "reviewer", "delete"].map((action) => (
            <button type="button" className="zorai-ghost-button" key={action} onClick={stopClick(() => onAction(action))}>{actionLabel(action)}</button>
          ))}
        </div>
      ) : null}
    </article>
  );
}

function TaskModal({
  task,
  mode,
  notices,
  editForm,
  setEditForm,
  onSave,
  onClose,
  onEdit,
  onOpenRuntime,
}: {
  task: WorkspaceTask;
  mode: "details" | "edit";
  notices: Array<{ notice_type: string; message: string }>;
  editForm: TaskForm;
  setEditForm: (form: TaskForm) => void;
  onSave: (event: FormEvent) => void;
  onClose: () => void;
  onEdit: () => void;
  onOpenRuntime: () => void;
}) {
  return (
    <div className="zorai-workspace-modal-overlay" role="presentation">
      <section className="zorai-workspace-modal" role="dialog" aria-modal="true" aria-labelledby="zorai-workspace-modal-title">
        <header className="zorai-workspace-modal__header">
          <div>
            <div className="zorai-section-label">{mode === "edit" ? "Edit Task" : "Task Details"}</div>
            <h2 id="zorai-workspace-modal-title">{task.title}</h2>
          </div>
          <div className="zorai-card-actions">
            <button type="button" className="zorai-ghost-button" onClick={onOpenRuntime}>Open runtime</button>
            {mode === "details" ? <button type="button" className="zorai-ghost-button" onClick={onEdit}>Edit</button> : null}
            <button type="button" className="zorai-ghost-button" onClick={onClose}>Close</button>
          </div>
        </header>
        {mode === "edit" ? (
          <TaskEditForm form={editForm} setForm={setEditForm} onSubmit={onSave} onCancel={onClose} />
        ) : (
          <div className="zorai-workspace-detail">
            <div className="zorai-workspace-detail-grid">
              <Info label="Type" value={task.task_type} />
              <Info label="Status" value={task.status} />
              <Info label="Priority" value={task.priority} />
              <Info label="Reporter" value={actorLabel(task.reporter)} />
              <Info label="Assignee" value={actorLabel(task.assignee)} />
              <Info label="Reviewer" value={actorLabel(task.reviewer)} />
              <Info label="Thread" value={task.thread_id ?? "none"} />
              <Info label="Goal" value={task.goal_run_id ?? "none"} />
            </div>
            <p>{task.description || "No description."}</p>
            <p>Definition of done: {task.definition_of_done ?? "Not provided"}</p>
            <div className="zorai-goal-mode-list">
              {notices.length === 0 ? <div>No notices.</div> : notices.slice(0, 5).map((notice, index) => <div key={`${notice.notice_type}-${index}`}>{notice.notice_type}: {notice.message}</div>)}
            </div>
          </div>
        )}
      </section>
    </div>
  );
}

function TaskEditForm({
  form,
  setForm,
  onSubmit,
  onCancel,
}: {
  form: TaskForm;
  setForm: (form: TaskForm) => void;
  onSubmit: (event: FormEvent) => void;
  onCancel: () => void;
}) {
  return (
    <form className="zorai-workspace-edit-form" onSubmit={onSubmit}>
      <label>
        <span>Title</span>
        <input value={form.title} onChange={(event) => setForm({ ...form, title: event.target.value })} />
      </label>
      <label>
        <span>Description</span>
        <textarea value={form.description} onChange={(event) => setForm({ ...form, description: event.target.value })} />
      </label>
      <label>
        <span>Definition of done</span>
        <textarea value={form.definitionOfDone} onChange={(event) => setForm({ ...form, definitionOfDone: event.target.value })} />
      </label>
      <div className="zorai-workspace-edit-form__grid">
        <label>
          <span>Priority</span>
          <select value={form.priority} onChange={(event) => setForm({ ...form, priority: event.target.value as WorkspacePriority })}>
            <option value="low">Low</option>
            <option value="normal">Normal</option>
            <option value="high">High</option>
            <option value="urgent">Urgent</option>
          </select>
        </label>
        <label>
          <span>Assignee</span>
          <WorkspaceActorPickerControl mode="assignee" value={form.assignee} onChange={(assignee) => setForm({ ...form, assignee })} />
        </label>
        <label>
          <span>Reviewer</span>
          <WorkspaceActorPickerControl mode="reviewer" value={form.reviewer} onChange={(reviewer) => setForm({ ...form, reviewer })} />
        </label>
      </div>
      <div className="zorai-card-actions">
        <button type="submit" className="zorai-primary-button" disabled={!form.title.trim()}>Save task</button>
        <button type="button" className="zorai-ghost-button" onClick={onCancel}>Cancel</button>
      </div>
    </form>
  );
}

function Info({ label, value }: { label: string; value: string }) {
  return <div className="zorai-goal-mode-info"><div className="zorai-section-label">{label}</div><p>{value}</p></div>;
}

function ensureMainWorkspace(settings: WorkspaceSettings[]): WorkspaceSettings[] {
  if (settings.some((workspace) => workspace.workspace_id === DEFAULT_WORKSPACE_ID)) return settings;
  return [{ workspace_id: DEFAULT_WORKSPACE_ID, workspace_root: null, operator: "user", repo_monitor_enabled: false, repo_monitor_include_dirs: [], repo_monitor_exclude_dirs: [], created_at: 0, updated_at: 0 }, ...settings];
}

function upsertTask(tasks: WorkspaceTask[], task: WorkspaceTask): WorkspaceTask[] {
  return tasks.some((item) => item.id === task.id) ? tasks.map((item) => item.id === task.id ? task : item) : [...tasks, task];
}

function formFromTask(task: WorkspaceTask): TaskForm {
  return {
    title: task.title,
    taskType: task.task_type,
    description: task.description,
    definitionOfDone: task.definition_of_done ?? "",
    priority: task.priority,
    assignee: actorLabel(task.assignee),
    reviewer: actorLabel(task.reviewer),
  };
}

function latestRuntimeThreadId(task: WorkspaceTask): string | null {
  return task.thread_id ?? [...task.runtime_history].reverse().find((entry) => entry.thread_id)?.thread_id ?? null;
}

function latestRuntimeGoalRunId(task: WorkspaceTask): string | null {
  return task.goal_run_id ?? [...task.runtime_history].reverse().find((entry) => entry.goal_run_id)?.goal_run_id ?? null;
}

function appendSortOrder(tasks: WorkspaceTask[], status: WorkspaceTaskStatus): number {
  return Math.max(0, ...tasks.filter((task) => task.status === status && task.deleted_at == null).map((task) => task.sort_order)) + 1;
}

function toggleSet(items: Set<string>, id: string): Set<string> {
  const next = new Set(items);
  if (next.has(id)) next.delete(id);
  else next.add(id);
  return next;
}

function defaultActor(current: WorkspaceActor): WorkspaceActor {
  return current ?? { agent: "svarog" };
}

function defaultReviewer(current: WorkspaceActor): WorkspaceActor {
  return current ?? "user";
}

function shortId(id: string): string {
  return id.length > 12 ? id.slice(0, 12) : id;
}

function actionLabel(action: string): string {
  return action === "move" ? "Move" : action === "reviewer" ? "Reviewer" : `${action.charAt(0).toUpperCase()}${action.slice(1)}`;
}

function workspaceActionStatus(action: string): string {
  if (action === "review") return "Sending workspace task to review.";
  return `${actionLabel(action)} workspace task.`;
}

function stopClick(callback: () => void) {
  return (event: MouseEvent) => {
    event.stopPropagation();
    callback();
  };
}
