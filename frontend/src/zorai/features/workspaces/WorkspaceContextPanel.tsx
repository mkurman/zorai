import { actorLabel, taskRunBlocked } from "@/lib/workspaceBoard";
import { priorityTone, statusTone, useWorkspaceSelectionStore } from "./workspaceBoardModel";

export function WorkspacesContext() {
  const { workspaceId, task, notices, onOpenRuntime, onEdit, onAction } = useWorkspaceSelectionStore();

  if (!task) {
    return (
      <div className="zorai-context-summary">
        <div className="zorai-section-label">Workspace · {workspaceId}</div>
        <div className="zorai-context-block">
          <strong>Task Inspector</strong>
          <span>Select a card on the board or list to inspect its full description, definition of done, notices, and runtime controls.</span>
        </div>
      </div>
    );
  }

  return (
    <div className="zorai-context-summary">
      <div className="zorai-section-label">Task · {task.id.slice(0, 12)}</div>
      <div className="zorai-context-block">
        <strong>{task.title}</strong>
        <div className="zorai-context-pills">
          <span className={`zorai-status-pill zorai-status-pill--${statusTone(task.status)}`}>{task.status.replace("_", " ")}</span>
          <span className={`zorai-status-pill zorai-status-pill--${priorityTone(task.priority)}`}>{task.priority}</span>
          <span className="zorai-status-pill">{task.task_type}</span>
        </div>
      </div>

      <div className="zorai-context-actions">
        {onOpenRuntime ? (
          <button type="button" className="zorai-ghost-button zorai-ghost-button--primary-soft" onClick={() => onOpenRuntime(task)}>
            Open runtime
          </button>
        ) : null}
        {onEdit ? (
          <button type="button" className="zorai-ghost-button" onClick={() => onEdit(task)}>
            Edit
          </button>
        ) : null}
        {onAction && !taskRunBlocked(task) ? (
          <button type="button" className="zorai-ghost-button" onClick={() => onAction(task, "run")}>
            Run
          </button>
        ) : null}
      </div>

      <div className="zorai-context-kv-grid">
        <div><span>Assignee</span><strong>{actorLabel(task.assignee)}</strong></div>
        <div><span>Reviewer</span><strong>{actorLabel(task.reviewer)}</strong></div>
        <div><span>Reporter</span><strong>{actorLabel(task.reporter)}</strong></div>
      </div>

      {task.description.trim() ? (
        <div className="zorai-context-block">
          <div className="zorai-section-label">Description</div>
          <p className="zorai-context-prose">{task.description}</p>
        </div>
      ) : null}

      {task.definition_of_done ? (
        <div className="zorai-context-block">
          <div className="zorai-section-label">Definition of done</div>
          <p className="zorai-context-prose">{task.definition_of_done}</p>
        </div>
      ) : null}

      <div className="zorai-context-block">
        <div className="zorai-section-label">Activity ({notices.length})</div>
        {notices.length === 0 ? (
          <span>No notices recorded yet.</span>
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
      </div>
    </div>
  );
}
