import { iconButtonStyle } from "../shared";
import {
  formatGoalRunDuration,
  formatGoalRunStatus,
  goalRunChildTaskCount,
  goalRunStatusColor,
  isGoalRunActive,
  latestGoalRunTodoSnapshot,
  summarizeGoalRunStep,
  type GoalRun,
  type GoalRunEvent,
  type TodoItem,
} from "../../../lib/goalRuns";
import type { AgentRun } from "../../../lib/agentRuns";
import { formatTaskTimestamp } from "../../../lib/agentTaskQueue";
import { detailBodyStyle, detailLabelStyle } from "./styles";

function todoStatusColor(status: TodoItem["status"]): string {
  switch (status) {
    case "in_progress":
      return "var(--accent)";
    case "completed":
      return "var(--success)";
    case "blocked":
      return "var(--warning)";
    default:
      return "var(--text-muted)";
  }
}

function TodoSnapshotList({ items }: { items: TodoItem[] }) {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-1)", marginTop: 4 }}>
      {items.slice().sort((a, b) => a.position - b.position).map((item) => (
        <div
          key={item.id}
          style={{
            display: "flex",
            alignItems: "center",
            gap: "var(--space-2)",
            padding: "6px 8px",
            borderRadius: "var(--radius-sm)",
            background: "color-mix(in srgb, var(--text-primary) 3%, transparent)",
          }}
        >
          <span
            style={{
              width: 8,
              height: 8,
              borderRadius: "50%",
              background: todoStatusColor(item.status),
              flexShrink: 0,
            }}
          />
          <span style={{ fontSize: "var(--text-sm)", color: "var(--text-primary)", flex: 1 }}>
            {item.content}
          </span>
          <span style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)", textTransform: "capitalize" }}>
            {item.status.replace(/_/g, " ")}
          </span>
        </div>
      ))}
    </div>
  );
}

function DetailCard({ label, value }: { label: string; value: string }) {
  return (
    <div style={{ padding: "var(--space-2)", borderRadius: "var(--radius-sm)", background: "var(--bg-tertiary)" }}>
      <div style={detailLabelStyle}>{label}</div>
      <div style={{ fontSize: "var(--text-sm)", color: "var(--text-primary)", marginTop: 2, wordBreak: "break-word" }}>{value}</div>
    </div>
  );
}

function formatInteger(value?: number | null): string {
  return typeof value === "number" && Number.isFinite(value)
    ? value.toLocaleString("en-US")
    : "0";
}

function formatCost(value?: number | null): string {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    return "-";
  }
  return value >= 1 ? `$${value.toFixed(2)}` : `$${value.toFixed(4)}`;
}

function formatModelDuration(durationMs?: number | null): string {
  if (typeof durationMs !== "number" || !Number.isFinite(durationMs) || durationMs <= 0) {
    return "-";
  }

  const totalSeconds = Math.max(1, Math.round(durationMs / 1000));
  if (totalSeconds < 120) {
    return `${totalSeconds}s`;
  }
  return formatGoalRunDuration(durationMs);
}

function hasGoalUsage(goalRun: GoalRun): boolean {
  return Boolean(
    (goalRun.total_prompt_tokens ?? 0) > 0 ||
    (goalRun.total_completion_tokens ?? 0) > 0 ||
    typeof goalRun.estimated_cost_usd === "number" ||
    (goalRun.model_usage?.length ?? 0) > 0,
  );
}

interface GoalAgentRow {
  id: string;
  label: string;
  name: string;
  detail: string;
}

function profileDetail(provider: string, model: string, reasoningEffort?: string | null): string {
  return [provider && model ? `${provider}/${model}` : provider || model, reasoningEffort]
    .filter(Boolean)
    .join(" · ");
}

function buildGoalAgentRows(goalRun: GoalRun, agentRuns: AgentRun[]): GoalAgentRow[] {
  const rows: GoalAgentRow[] = [];
  const seen = new Set<string>();

  const addRow = (row: GoalAgentRow) => {
    const key = `${row.label}\n${row.name}\n${row.detail}`;
    if (seen.has(key)) {
      return;
    }
    seen.add(key);
    rows.push(row);
  };

  if (goalRun.planner_owner_profile) {
    addRow({
      id: `${goalRun.id}-planner`,
      label: "Planner",
      name: goalRun.planner_owner_profile.agent_label,
      detail: profileDetail(
        goalRun.planner_owner_profile.provider,
        goalRun.planner_owner_profile.model,
        goalRun.planner_owner_profile.reasoning_effort,
      ),
    });
  }

  if (goalRun.current_step_owner_profile) {
    addRow({
      id: `${goalRun.id}-current`,
      label: "Current",
      name: goalRun.current_step_owner_profile.agent_label,
      detail: profileDetail(
        goalRun.current_step_owner_profile.provider,
        goalRun.current_step_owner_profile.model,
        goalRun.current_step_owner_profile.reasoning_effort,
      ),
    });
  }

  const assignments = (goalRun.runtime_assignment_list?.length
    ? goalRun.runtime_assignment_list
    : goalRun.launch_assignment_snapshot) ?? [];
  for (const assignment of assignments) {
    addRow({
      id: `${goalRun.id}-role-${assignment.role_id}`,
      label: "Role",
      name: assignment.role_id,
      detail: [
        assignment.inherit_from_main
          ? "inherits main"
          : profileDetail(assignment.provider, assignment.model, assignment.reasoning_effort),
        assignment.enabled ? null : "disabled",
      ].filter(Boolean).join(" · "),
    });
  }

  for (const run of agentRuns) {
    addRow({
      id: `${goalRun.id}-run-${run.id}`,
      label: run.kind === "subagent" || run.parent_run_id || run.parent_task_id || run.parent_thread_id ? "Subagent" : "Task",
      name: run.title,
      detail: [run.kind, run.status, run.goal_step_title].filter(Boolean).join(" · "),
    });
  }

  return rows;
}

function GoalRunTimelineEvent({ event }: { event: GoalRunEvent }) {
  return (
    <div
      style={{
        padding: "var(--space-2)",
        borderRadius: "var(--radius-sm)",
        background: "var(--bg-tertiary)",
        display: "flex",
        flexDirection: "column",
        gap: "var(--space-1)",
      }}
    >
      <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: "var(--space-2)" }}>
        <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)", textTransform: "capitalize" }}>
          {event.phase.replace(/_/g, " ")}
          {typeof event.step_index === "number" ? ` · step ${event.step_index + 1}` : ""}
        </div>
        <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)" }}>
          {formatTaskTimestamp(event.timestamp)}
        </div>
      </div>
      <div style={{ fontSize: "var(--text-sm)", color: "var(--text-primary)" }}>
        {event.message}
      </div>
      {event.details && (
        <div style={{ fontSize: "var(--text-xs)", color: "var(--text-secondary)", whiteSpace: "pre-wrap" }}>
          {event.details}
        </div>
      )}
      {event.todo_snapshot.length > 0 && <TodoSnapshotList items={event.todo_snapshot} />}
    </div>
  );
}

export function GoalRunCard({
  goalRun,
  selected,
  busy,
  onSelect,
  onPause,
  onResume,
  onCancel,
}: {
  goalRun: GoalRun;
  selected: boolean;
  busy: boolean;
  onSelect: () => void;
  onPause?: () => void;
  onResume?: () => void;
  onCancel?: () => void;
}) {
  const statusColor = goalRunStatusColor(goalRun.status);
  const active = isGoalRunActive(goalRun);

  return (
    <div
      role="button"
      tabIndex={0}
      onClick={onSelect}
      onKeyDown={(event) => {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          onSelect();
        }
      }}
      style={{
        width: "100%",
        textAlign: "left",
        padding: "var(--space-3)",
        borderRadius: "var(--radius-md)",
        border: selected ? `1px solid ${statusColor}` : "1px solid var(--border)",
        background: selected ? "var(--bg-tertiary)" : "var(--bg-secondary)",
        marginBottom: "var(--space-2)",
        cursor: "pointer",
      }}
    >
      <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: "var(--space-2)" }}>
        <div style={{ flex: 1, minWidth: 0 }}>
          <div style={{ display: "flex", alignItems: "center", gap: "var(--space-2)", flexWrap: "wrap" }}>
            <div style={{ fontSize: "var(--text-sm)", fontWeight: 600, color: "var(--text-primary)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
              {goalRun.title}
            </div>
            <span style={{ padding: "2px 8px", borderRadius: 999, fontSize: "var(--text-xs)", border: "1px solid var(--glass-border)", color: statusColor }}>
              {formatGoalRunStatus(goalRun.status)}
            </span>
          </div>
          <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)", marginTop: 4 }}>
            {summarizeGoalRunStep(goalRun)}
            <span style={{ marginLeft: "var(--space-2)" }}>{formatTaskTimestamp(goalRun.created_at)}</span>
            <span style={{ marginLeft: "var(--space-2)" }}>{goalRunChildTaskCount(goalRun)} child task{goalRunChildTaskCount(goalRun) === 1 ? "" : "s"}</span>
            <span style={{ marginLeft: "var(--space-2)" }}>{goalRun.replan_count} replan{goalRun.replan_count === 1 ? "" : "s"}</span>
            <span style={{ marginLeft: "var(--space-2)" }}>{goalRun.approval_count ?? 0} approval{(goalRun.approval_count ?? 0) === 1 ? "" : "s"}</span>
            <span style={{ marginLeft: "var(--space-2)" }}>{formatGoalRunDuration(goalRun.duration_ms)}</span>
          </div>
        </div>
        {active && (
          <div style={{ display: "flex", gap: "var(--space-1)", flexShrink: 0 }}>
            {onPause && (
              <button type="button" onClick={(event) => { event.stopPropagation(); onPause(); }} style={{ ...iconButtonStyle, fontSize: "var(--text-sm)" }} disabled={busy}>
                Pause
              </button>
            )}
            {onResume && (
              <button type="button" onClick={(event) => { event.stopPropagation(); onResume(); }} style={{ ...iconButtonStyle, fontSize: "var(--text-sm)" }} disabled={busy}>
                Resume
              </button>
            )}
            {onCancel && (
              <button type="button" onClick={(event) => { event.stopPropagation(); onCancel(); }} style={{ ...iconButtonStyle, fontSize: "var(--text-sm)" }} disabled={busy}>
                Cancel
              </button>
            )}
          </div>
        )}
      </div>
      {goalRun.plan_summary && (
        <div style={{ fontSize: "var(--text-xs)", color: "var(--text-secondary)", marginTop: "var(--space-2)" }}>
          {goalRun.plan_summary}
        </div>
      )}
      {(goalRun.failure_cause || goalRun.last_error || goalRun.error) && (
        <div style={{ fontSize: "var(--text-xs)", color: "var(--danger)", marginTop: "var(--space-2)" }}>
          {goalRun.failure_cause ?? goalRun.last_error ?? goalRun.error}
        </div>
      )}
    </div>
  );
}

export function GoalRunDetail({
  goalRun,
  agentRuns = [],
  busy,
  onReview,
}: {
  goalRun: GoalRun;
  agentRuns?: AgentRun[];
  busy: boolean;
  onReview: (action: "accept" | "soft_reject" | "hard_reject", explanation?: string) => void;
}) {
  const currentStep = typeof goalRun.current_step_index === "number" && goalRun.steps?.length
    ? goalRun.steps[goalRun.current_step_index] ?? null
    : null;
  const latestTodos = latestGoalRunTodoSnapshot(goalRun);
  const agentRows = buildGoalAgentRows(goalRun, agentRuns);

  return (
    <div
      style={{
        padding: "var(--space-3)",
        borderRadius: "var(--radius-md)",
        border: "1px solid var(--border)",
        background: "var(--bg-secondary)",
        display: "flex",
        flexDirection: "column",
        gap: "var(--space-3)",
      }}
    >
      <div>
        <div style={{ fontSize: "var(--text-sm)", color: "var(--text-primary)", fontWeight: 600 }}>{goalRun.title}</div>
        <div style={{ fontSize: "var(--text-xs)", color: goalRunStatusColor(goalRun.status), marginTop: 4 }}>
          {formatGoalRunStatus(goalRun.status)}
        </div>
      </div>

      <div style={{ fontSize: "var(--text-sm)", color: "var(--text-primary)", whiteSpace: "pre-wrap" }}>
        {goalRun.goal}
      </div>

      <div style={{ display: "grid", gridTemplateColumns: "repeat(2, minmax(0, 1fr))", gap: "var(--space-2)" }}>
        <DetailCard label="Current Step" value={currentStep?.title || goalRun.current_step_title || "-"} />
        <DetailCard label="Step Kind" value={currentStep?.kind || goalRun.current_step_kind || "-"} />
        <DetailCard label="Thread" value={goalRun.thread_id || "-"} />
        <DetailCard label="Session" value={goalRun.session_id || currentStep?.session_id || "-"} />
        <DetailCard label="Approval" value={goalRun.awaiting_approval_id || "-"} />
        <DetailCard label="Skill" value={goalRun.generated_skill_path || "-"} />
        <DetailCard label="Duration" value={formatGoalRunDuration(goalRun.duration_ms)} />
        <DetailCard label="Replans" value={String(goalRun.replan_count)} />
        <DetailCard label="Child Tasks" value={String(goalRunChildTaskCount(goalRun))} />
        <DetailCard label="Approvals" value={String(goalRun.approval_count ?? 0)} />
      </div>

      {hasGoalUsage(goalRun) && (
        <div>
          <div style={detailLabelStyle}>Usage</div>
          <div style={{ display: "grid", gridTemplateColumns: "repeat(3, minmax(0, 1fr))", gap: "var(--space-2)", marginTop: "var(--space-2)" }}>
            <DetailCard label="Prompt Tokens" value={formatInteger(goalRun.total_prompt_tokens)} />
            <DetailCard label="Completion Tokens" value={formatInteger(goalRun.total_completion_tokens)} />
            <DetailCard label="Estimated Cost" value={formatCost(goalRun.estimated_cost_usd)} />
          </div>
          {(goalRun.model_usage?.length ?? 0) > 0 && (
            <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-2)", marginTop: "var(--space-2)" }}>
              {goalRun.model_usage?.map((usage) => (
                <div key={`${usage.provider}-${usage.model}`} style={{ ...detailBodyStyle, display: "grid", gridTemplateColumns: "minmax(0, 1fr) auto", gap: "var(--space-2)", alignItems: "center" }}>
                  <div style={{ minWidth: 0 }}>
                    <div style={{ color: "var(--text-primary)", overflowWrap: "anywhere" }}>
                      {usage.provider}
                    </div>
                    <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)", overflowWrap: "anywhere" }}>
                      {usage.model}
                    </div>
                  </div>
                  <div style={{ fontSize: "var(--text-xs)", color: "var(--text-secondary)", textAlign: "right", whiteSpace: "nowrap" }}>
                    {usage.request_count} req · {formatInteger(usage.prompt_tokens)} in · {formatInteger(usage.completion_tokens)} out · {formatCost(usage.estimated_cost_usd)} · {formatModelDuration(usage.duration_ms)}
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      )}

      {agentRows.length > 0 && (
        <div>
          <div style={detailLabelStyle}>Agents</div>
          <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-2)", marginTop: "var(--space-2)" }}>
            {agentRows.map((row) => (
              <div key={row.id} style={detailBodyStyle}>
                <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)" }}>{row.label}</div>
                <div style={{ color: "var(--text-primary)", marginTop: 2 }}>{row.name}</div>
                {row.detail && (
                  <div style={{ fontSize: "var(--text-xs)", color: "var(--text-secondary)", marginTop: 2 }}>
                    {row.detail}
                  </div>
                )}
              </div>
            ))}
          </div>
        </div>
      )}

      {goalRun.plan_summary && (
        <div>
          <div style={detailLabelStyle}>Plan</div>
          <div style={detailBodyStyle}>{goalRun.plan_summary}</div>
        </div>
      )}

      {goalRun.reflection_summary && (
        <div>
          <div style={detailLabelStyle}>Reflection</div>
          <div style={detailBodyStyle}>{goalRun.reflection_summary}</div>
        </div>
      )}

      {goalRun.status === "awaiting_review" && (
        <div>
          <div style={detailLabelStyle}>Worker report</div>
          <div style={detailBodyStyle}>{goalRun.pending_review_report || "The worker asked for supervisor review."}</div>
          <div style={{ display: "flex", gap: "var(--space-2)", marginTop: "var(--space-2)", flexWrap: "wrap" }}>
            <button type="button" onClick={() => onReview("accept")} style={{ ...iconButtonStyle, fontSize: "var(--text-sm)" }} disabled={busy}>
              Accept
            </button>
            <button
              type="button"
              onClick={() => {
                const reason = window.prompt("Why should the worker keep going?");
                if (reason?.trim()) onReview("soft_reject", reason.trim());
              }}
              style={{ ...iconButtonStyle, fontSize: "var(--text-sm)" }}
              disabled={busy}
            >
              Soft reject
            </button>
            <button
              type="button"
              onClick={() => {
                const reason = window.prompt("Why is this goal rejected?");
                if (reason?.trim()) onReview("hard_reject", reason.trim());
              }}
              style={{ ...iconButtonStyle, fontSize: "var(--text-sm)" }}
              disabled={busy}
            >
              Hard reject
            </button>
          </div>
        </div>
      )}

      {goalRun.steps && goalRun.steps.length > 0 && (
        <div>
          <div style={detailLabelStyle}>Plan Steps</div>
          <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-2)" }}>
            {goalRun.steps.map((step, index) => {
              const active = goalRun.current_step_index === index;
              return (
                <div
                  key={step.id}
                  style={{
                    padding: "var(--space-2)",
                    borderRadius: "var(--radius-sm)",
                    background: active ? "var(--mission-soft)" : "var(--bg-tertiary)",
                    border: active ? "1px solid var(--mission-border)" : "1px solid transparent",
                  }}
                >
                  <div style={{ fontSize: "var(--text-xs)", color: active ? "var(--mission)" : "var(--text-muted)" }}>
                    Step {index + 1} · {step.kind}{step.status ? ` · ${step.status}` : ""}
                  </div>
                  <div style={{ fontSize: "var(--text-sm)", color: "var(--text-primary)", marginTop: 2 }}>{step.title}</div>
                  {step.success_condition && (
                    <div style={{ fontSize: "var(--text-xs)", color: "var(--text-secondary)", marginTop: 4 }}>
                      Success: {step.success_condition}
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        </div>
      )}

      {latestTodos.length > 0 && (
        <div>
          <div style={detailLabelStyle}>Current Todo</div>
          <TodoSnapshotList items={latestTodos} />
        </div>
      )}

      {goalRun.events && goalRun.events.length > 0 && (
        <div>
          <div style={detailLabelStyle}>Timeline</div>
          <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-2)" }}>
            {goalRun.events.slice().sort((a, b) => b.timestamp - a.timestamp).map((event) => (
              <GoalRunTimelineEvent key={event.id} event={event} />
            ))}
          </div>
        </div>
      )}

      {goalRun.memory_updates && goalRun.memory_updates.length > 0 && (
        <div>
          <div style={detailLabelStyle}>Memory Updates</div>
          <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-2)" }}>
            {goalRun.memory_updates.map((entry, index) => (
              <div key={`${goalRun.id}-memory-${index}`} style={detailBodyStyle}>{entry}</div>
            ))}
          </div>
        </div>
      )}

      {(goalRun.result || goalRun.failure_cause || goalRun.last_error || goalRun.error) && (
        <div>
          <div style={detailLabelStyle}>{goalRun.result ? "Outcome" : "Error"}</div>
          <div style={detailBodyStyle}>{goalRun.result || goalRun.failure_cause || goalRun.last_error || goalRun.error}</div>
        </div>
      )}
    </div>
  );
}
