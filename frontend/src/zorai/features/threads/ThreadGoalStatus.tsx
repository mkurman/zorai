import { useState, type CSSProperties } from "react";
import { controlGoalRun, formatGoalRunStatus, goalRunStatusColor, isGoalRunTerminal, summarizeGoalRunStep, type GoalRun } from "@/lib/goalRuns";
import { navigateZorai } from "@/zorai/shell/zoraiNavigationEvents";
import { pushToast } from "@/lib/toastStore";
import { goalControlOptions, type GoalControlOption } from "./threadGoalModel";

export function ThreadGoalStatusDot({ goal }: { goal: GoalRun }) {
  return <span className="zorai-thread-goal__dot" style={{ "--goal-status-color": goalRunStatusColor(goal.status) } as CSSProperties} aria-hidden="true" />;
}

export function ThreadGoalStatus({ goal }: { goal: GoalRun }) {
  const [busy, setBusy] = useState<GoalControlOption["action"] | null>(null);
  const [errorExpanded, setErrorExpanded] = useState(false);
  const step = goal.status === "paused" || isGoalRunTerminal(goal) ? null : summarizeGoalRunStep(goal);
  const error = goal.last_error || goal.error;

  const run = async (option: GoalControlOption) => {
    if (option.action === "cancel" && !window.confirm(`Cancel the goal "${goal.title}"? Its workers are stopped and it cannot be resumed.`)) return;
    setBusy(option.action);
    try {
      if (!await controlGoalRun(goal.id, option.action)) pushToast(`Could not ${option.label.toLowerCase()} the goal.`, "error");
    } finally {
      setBusy(null);
    }
  };

  return (
    <div className="zorai-thread-goal">
      <div className="zorai-thread-goal__head">
        <ThreadGoalStatusDot goal={goal} />
        <strong title={goal.goal}>{goal.title}</strong>
        <span className="zorai-thread-goal__status">{formatGoalRunStatus(goal.status)}</span>
      </div>
      {goal.status === "paused" ? (
        <p className="zorai-thread-goal__note">Paused: the agent still answers your messages, but it won't continue on its own, including after subagents report back, until you resume the goal.</p>
      ) : null}
      {step ? <p className="zorai-thread-goal__step">{step}</p> : null}
      {error ? (
        <div className="zorai-thread-goal__error-block">
          <p className={["zorai-thread-goal__error", errorExpanded ? "" : "is-clamped"].filter(Boolean).join(" ")}>{error}</p>
          {error.length > 180 ? (
            <button type="button" className="zorai-thread-goal__more" onClick={() => setErrorExpanded((value) => !value)}>
              {errorExpanded ? "Show less" : "Show full reason"}
            </button>
          ) : null}
        </div>
      ) : null}
      <div className="zorai-thread-goal__actions">
        {goalControlOptions(goal).map((option) => (
          <button
            key={option.action}
            type="button"
            className={["zorai-thread-goal__action", option.danger ? "is-danger" : ""].filter(Boolean).join(" ")}
            title={option.title}
            disabled={busy !== null}
            onClick={() => void run(option)}
          >{busy === option.action ? `${option.label}…` : option.label}</button>
        ))}
        <button type="button" className="zorai-thread-goal__action" onClick={() => navigateZorai({ view: "goals", goalRunId: goal.id })}>Open in Goals</button>
      </div>
    </div>
  );
}
