import type { Dispatch, SetStateAction } from "react";
import type { GoalRun } from "../../../lib/goalRuns";
import type { AgentRun } from "../../../lib/agentRuns";
import { RefreshButton } from "@/zorai/shell/RefreshButton";
import { ActionButton, EmptyPanel, SectionTitle } from "../shared";
import { GoalRunCard, GoalRunDetail } from "./GoalRunSection";
import { inputBlockStyle, inputRowStyle, sectionLabelStyle } from "./styles";

interface GoalRunPanelProps {
  goalRunsSupported: boolean;
  newGoalPrompt: string;
  setNewGoalPrompt: Dispatch<SetStateAction<string>>;
  newGoalTitle: string;
  setNewGoalTitle: Dispatch<SetStateAction<string>>;
  newGoalSessionId: string;
  setNewGoalSessionId: Dispatch<SetStateAction<string>>;
  goalStartError: string | null;
  onAddGoalRun: () => void;
  onRefreshGoalRuns: () => void;
  activeGoalRuns: GoalRun[];
  historicalGoalRuns: GoalRun[];
  completedGoalRuns: GoalRun[];
  selectedGoalRun: GoalRun | null;
  selectedGoalRunAgentRuns: AgentRun[];
  selectedGoalRunId: string | null;
  goalActionId: string | null;
  onSelectGoalRun: (goalRunId: string) => void;
  onChangeGoalRunState: (
    goalRunId: string,
    action: "pause" | "resume" | "cancel" | "accept" | "soft_reject" | "hard_reject",
    stepIndex?: number,
    explanation?: string,
  ) => void;
  historyFailureQuery: string;
  setHistoryFailureQuery: Dispatch<SetStateAction<string>>;
  historyMinReplans: number;
  setHistoryMinReplans: Dispatch<SetStateAction<number>>;
  historyMinChildTasks: number;
  setHistoryMinChildTasks: Dispatch<SetStateAction<number>>;
  historyMinApprovals: number;
  setHistoryMinApprovals: Dispatch<SetStateAction<number>>;
  historyMinDurationMinutes: number;
  setHistoryMinDurationMinutes: Dispatch<SetStateAction<number>>;
  totalGoalRunCount: number;
}

export function GoalRunPanel({
  goalRunsSupported,
  newGoalPrompt,
  setNewGoalPrompt,
  newGoalTitle,
  setNewGoalTitle,
  newGoalSessionId,
  setNewGoalSessionId,
  goalStartError,
  onAddGoalRun,
  onRefreshGoalRuns,
  activeGoalRuns,
  historicalGoalRuns,
  completedGoalRuns,
  selectedGoalRun,
  selectedGoalRunAgentRuns,
  selectedGoalRunId,
  goalActionId,
  onSelectGoalRun,
  onChangeGoalRunState,
  historyFailureQuery,
  setHistoryFailureQuery,
  historyMinReplans,
  setHistoryMinReplans,
  historyMinChildTasks,
  setHistoryMinChildTasks,
  historyMinApprovals,
  setHistoryMinApprovals,
  historyMinDurationMinutes,
  setHistoryMinDurationMinutes,
  totalGoalRunCount,
}: GoalRunPanelProps) {
  return (
    <>
      <div
        style={{
          display: "flex",
          alignItems: "flex-start",
          justifyContent: "space-between",
          gap: "var(--space-3)",
          flexWrap: "wrap",
        }}
      >
        <div style={{ flex: "1 1 320px" }}>
          <SectionTitle
            title="Goal Runners"
            subtitle="Durable autonomous jobs that plan, execute, reflect, and learn over time"
          />
        </div>
        <div style={{ paddingTop: "var(--space-4)" }}>
          <RefreshButton onClick={onRefreshGoalRuns} />
        </div>
      </div>

      {goalRunsSupported ? (
        <>
          <div
            style={{
              marginBottom: "var(--space-4)",
              display: "flex",
              flexDirection: "column",
              gap: "var(--space-2)",
            }}
          >
            <textarea
              placeholder="Define a long-running goal..."
              value={newGoalPrompt}
              onChange={(event) => setNewGoalPrompt(event.target.value)}
              rows={3}
              style={inputBlockStyle}
            />
            <div
              style={{
                display: "grid",
                gridTemplateColumns: "repeat(2, minmax(0, 1fr))",
                gap: "var(--space-2)",
              }}
            >
              <input
                type="text"
                placeholder="Goal title (optional)..."
                value={newGoalTitle}
                onChange={(event) => setNewGoalTitle(event.target.value)}
                style={inputRowStyle}
              />
              <input
                type="text"
                placeholder="Target session ID (optional)..."
                value={newGoalSessionId}
                onChange={(event) => setNewGoalSessionId(event.target.value)}
                style={inputRowStyle}
              />
            </div>
            <div
              style={{
                display: "flex",
                alignItems: "center",
                justifyContent: "space-between",
                gap: "var(--space-2)",
                flexWrap: "wrap",
              }}
            >
              <div
                style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)" }}
              >
                Goal runners sit above the task queue and can spawn child tasks as
                they replan.
              </div>
              <ActionButton onClick={onAddGoalRun} disabled={!newGoalPrompt.trim()}>
                Start Goal Run
              </ActionButton>
            </div>
            {goalStartError && (
              <div style={{ fontSize: "var(--text-xs)", color: "var(--danger)" }}>
                {goalStartError}
              </div>
            )}
          </div>

          {activeGoalRuns.length > 0 && (
            <div style={{ marginBottom: "var(--space-4)" }}>
              <div style={sectionLabelStyle}>Active ({activeGoalRuns.length})</div>
              {activeGoalRuns.map((goalRun) => (
                <GoalRunCard
                  key={goalRun.id}
                  goalRun={goalRun}
                  selected={goalRun.id === selectedGoalRunId}
                  busy={goalActionId === goalRun.id}
                  onSelect={() => onSelectGoalRun(goalRun.id)}
                  onPause={
                    goalRun.status === "running"
                      ? () => onChangeGoalRunState(goalRun.id, "pause")
                      : undefined
                  }
                  onResume={
                    goalRun.status === "paused"
                      ? () => onChangeGoalRunState(goalRun.id, "resume")
                      : undefined
                  }
                  onCancel={
                    goalRun.status !== "completed" &&
                    goalRun.status !== "failed" &&
                    goalRun.status !== "cancelled"
                      ? () => onChangeGoalRunState(goalRun.id, "cancel")
                      : undefined
                  }
                />
              ))}
            </div>
          )}

          {historicalGoalRuns.length > 0 && (
            <div style={{ marginBottom: "var(--space-4)" }}>
              <div style={sectionLabelStyle}>
                History ({completedGoalRuns.length} shown of{" "}
                {historicalGoalRuns.length})
              </div>
              <div
                style={{
                  display: "grid",
                  gridTemplateColumns: "repeat(5, minmax(0, 1fr))",
                  gap: "var(--space-2)",
                  marginBottom: "var(--space-3)",
                }}
              >
                <input
                  type="text"
                  placeholder="Failure cause filter..."
                  value={historyFailureQuery}
                  onChange={(event) => setHistoryFailureQuery(event.target.value)}
                  style={inputRowStyle}
                />
                <input
                  type="number"
                  min={0}
                  placeholder="Min replans"
                  value={historyMinReplans}
                  onChange={(event) =>
                    setHistoryMinReplans(Number(event.target.value) || 0)
                  }
                  style={inputRowStyle}
                />
                <input
                  type="number"
                  min={0}
                  placeholder="Min child tasks"
                  value={historyMinChildTasks}
                  onChange={(event) =>
                    setHistoryMinChildTasks(Number(event.target.value) || 0)
                  }
                  style={inputRowStyle}
                />
                <input
                  type="number"
                  min={0}
                  placeholder="Min approvals"
                  value={historyMinApprovals}
                  onChange={(event) =>
                    setHistoryMinApprovals(Number(event.target.value) || 0)
                  }
                  style={inputRowStyle}
                />
                <input
                  type="number"
                  min={0}
                  placeholder="Min duration (min)"
                  value={historyMinDurationMinutes}
                  onChange={(event) =>
                    setHistoryMinDurationMinutes(Number(event.target.value) || 0)
                  }
                  style={inputRowStyle}
                />
              </div>
              {completedGoalRuns.length > 0 ? (
                completedGoalRuns.slice(0, 12).map((goalRun) => (
                  <GoalRunCard
                    key={goalRun.id}
                    goalRun={goalRun}
                    selected={goalRun.id === selectedGoalRunId}
                    busy={false}
                    onSelect={() => onSelectGoalRun(goalRun.id)}
                  />
                ))
              ) : (
                <EmptyPanel message="No historical goal runs match the current filters." />
              )}
            </div>
          )}

          {totalGoalRunCount === 0 && (
            <EmptyPanel message="No goal runs yet. Start a goal to let one worker work until the owner supervisor accepts." />
          )}

          {selectedGoalRun && (
            <div style={{ marginBottom: "var(--space-5)" }}>
              <SectionTitle
                title="Goal Run Detail"
                subtitle="Worker thread, review report, and supervisor verdicts"
              />
              <GoalRunDetail
                goalRun={selectedGoalRun}
                agentRuns={selectedGoalRunAgentRuns}
                busy={goalActionId === selectedGoalRun.id}
                onReview={(action, explanation) =>
                  onChangeGoalRunState(
                    selectedGoalRun.id,
                    action,
                    undefined,
                    explanation,
                  )
                }
              />
            </div>
          )}
        </>
      ) : (
        <EmptyPanel message="Goal-runner controls will appear here when the backend exposes goal-run IPC methods." />
      )}
    </>
  );
}
