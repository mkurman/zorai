import { actorLabel, taskRunBlocked } from "@/lib/workspaceBoard";
import { priorityTone, statusTone, useWorkspaceSelectionStore } from "./workspaceBoardModel";
import { ContextRow } from "@/zorai/features/threads/ThreadContextRow";

export function WorkspacesContext() {
  const { workspaceId, task, notices, onOpenRuntime, onEdit, onAction } = useWorkspaceSelectionStore();

  if (!task) {
    return (
      <div className="zorai-thread-context-stack">
        <div className="zorai-context-group">
          <div className="zorai-context-group__title">Workspace · {workspaceId}</div>
          <p className="zorai-context-hint">Select a card on the board or list to inspect its description, definition of done, activity and runtime controls.</p>
        </div>
      </div>
    );
  }

  return (
    <div className="zorai-thread-context-stack">
      <div className="zorai-context-group">
        <div className="zorai-context-group__title">Task · {task.id.slice(0, 12)}</div>
        <div className="zorai-context-heading">
          <strong>{task.title}</strong>
          <div className="zorai-context-pills">
            <span className={`zorai-status-pill zorai-status-pill--${statusTone(task.status)}`}>{task.status.replace("_", " ")}</span>
            <span className={`zorai-status-pill zorai-status-pill--${priorityTone(task.priority)}`}>{task.priority}</span>
            <span className="zorai-status-pill">{task.task_type}</span>
          </div>
        </div>
        <div className="zorai-context-inline-actions">
          {onOpenRuntime ? <button type="button" className="zorai-ghost-button zorai-ghost-button--compact" onClick={() => onOpenRuntime(task)}>Open runtime</button> : null}
          {onEdit ? <button type="button" className="zorai-ghost-button zorai-ghost-button--compact" onClick={() => onEdit(task)}>Edit</button> : null}
          {onAction && !taskRunBlocked(task) ? <button type="button" className="zorai-ghost-button zorai-ghost-button--compact" onClick={() => onAction(task, "run")}>Run</button> : null}
        </div>
      </div>

      <div className="zorai-context-group">
        <div className="zorai-context-group__title">People</div>
        <ContextRow icon="person" label="Assignee" value={actorLabel(task.assignee)} />
        <ContextRow icon="person" label="Reviewer" value={actorLabel(task.reviewer)} />
        <ContextRow icon="person" label="Reporter" value={actorLabel(task.reporter)} />
      </div>

      <div className="zorai-context-group">
        <div className="zorai-context-group__title">Details</div>
        {task.description.trim() ? (
          <ContextRow key={`description:${task.id}`} icon="text" label="Description" defaultOpen>
            <p className="zorai-context-prose">{task.description}</p>
          </ContextRow>
        ) : null}
        {task.definition_of_done ? (
          <ContextRow key={`done:${task.id}`} icon="done" label="Definition of done">
            <p className="zorai-context-prose">{task.definition_of_done}</p>
          </ContextRow>
        ) : null}
        <ContextRow key={`activity:${task.id}`} icon="activity" label="Activity" value={notices.length}>
          {notices.length === 0 ? (
            <p>No notices recorded yet.</p>
          ) : (
            <ul className="zorai-context-notice-list">
              {notices.slice(0, 8).map((notice, index) => (
                <li key={`${notice.notice_type}-${index}`}>
                  <strong>{notice.notice_type}</strong>
                  <span>{notice.message}</span>
                </li>
              ))}
            </ul>
          )}
        </ContextRow>
      </div>
    </div>
  );
}
