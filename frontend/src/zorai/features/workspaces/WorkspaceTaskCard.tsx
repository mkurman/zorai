import type { MouseEvent } from "react";
import {
  actorLabel,
  taskRunBlocked,
  type WorkspaceTask,
} from "@/lib/workspaceBoard";
import { priorityTone, shortActorName } from "./workspaceBoardModel";

export function WorkspaceTaskCard({
  task,
  notice,
  expanded,
  selected,
  onSelect,
  onToggle,
  onAction,
  onDetails,
  onEdit,
  onOpenRuntime,
}: {
  task: WorkspaceTask;
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
  const assignee = shortActorName(task.assignee);
  const reviewer = shortActorName(task.reviewer);
  const pTone = priorityTone(task.priority);

  return (
    <article
      className={["zorai-workspace-task", selected ? "zorai-workspace-task--active" : ""].filter(Boolean).join(" ")}
      onClick={onSelect}
      onDoubleClick={onDetails}
    >
      <div className="zorai-workspace-task__head">
        <strong className="zorai-workspace-task__title" title={task.title}>{task.title}</strong>
        <div className="zorai-workspace-task__badges">
          {task.priority !== "normal" ? (
            <span className={`zorai-status-pill zorai-status-pill--${pTone}`}>{task.priority}</span>
          ) : null}
          <span className="zorai-workspace-task__kind">{task.task_type}</span>
        </div>
      </div>

      {task.description.trim() ? (
        <p className="zorai-workspace-task__summary">{task.description.trim()}</p>
      ) : null}

      {notice ? (
        <div className="zorai-workspace-task__notice" title={notice}>{notice}</div>
      ) : null}

      <div className="zorai-workspace-task__footer">
        <div className="zorai-workspace-task__people">
          {assignee ? (
            <span className="zorai-workspace-person" title={`Assignee: ${actorLabel(task.assignee)}`}>
              {assignee}
            </span>
          ) : (
            <span className="zorai-workspace-person zorai-workspace-person--empty">unassigned</span>
          )}
          {reviewer ? (
            <>
              <span className="zorai-workspace-person__arrow" aria-hidden="true">→</span>
              <span className="zorai-workspace-person zorai-workspace-person--reviewer" title={`Reviewer: ${actorLabel(task.reviewer)}`}>
                {reviewer}
              </span>
            </>
          ) : null}
        </div>
        <div className="zorai-workspace-task__quick">
          <button type="button" className="zorai-task-chip-btn zorai-task-chip-btn--accent" onClick={stopClick(onOpenRuntime)} title="Open linked runtime">
            Runtime
          </button>
          <button type="button" className="zorai-task-chip-btn" onClick={stopClick(onDetails)}>
            Details
          </button>
          <button
            type="button"
            className={["zorai-task-chip-btn", expanded ? "is-active" : ""].filter(Boolean).join(" ")}
            aria-expanded={expanded}
            aria-label={expanded ? "Hide actions" : "More task actions"}
            title={expanded ? "Hide actions" : "More actions"}
            onClick={stopClick(onToggle)}
          >
            ···
          </button>
        </div>
      </div>

      {expanded ? (
        <div className="zorai-workspace-actions" onClick={(event) => event.stopPropagation()}>
          <button type="button" className="zorai-task-chip-btn" onClick={stopClick(onEdit)}>Edit</button>
          <button type="button" className="zorai-task-chip-btn" disabled={taskRunBlocked(task)} onClick={stopClick(() => onAction("run"))}>
            {taskRunBlocked(task) ? "Blocked" : "Run"}
          </button>
          {(["pause", "stop", "move", "review", "assign", "reviewer"] as const).map((action) => (
            <button type="button" className="zorai-task-chip-btn" key={action} onClick={stopClick(() => onAction(action))}>
              {actionLabel(action)}
            </button>
          ))}
          <button type="button" className="zorai-task-chip-btn zorai-task-chip-btn--danger" onClick={stopClick(() => onAction("delete"))}>
            Delete
          </button>
        </div>
      ) : null}
    </article>
  );
}

function actionLabel(action: string): string {
  return action === "move" ? "Move" : action === "reviewer" ? "Reviewer" : `${action.charAt(0).toUpperCase()}${action.slice(1)}`;
}

function stopClick(callback: () => void) {
  return (event: MouseEvent) => {
    event.stopPropagation();
    callback();
  };
}
