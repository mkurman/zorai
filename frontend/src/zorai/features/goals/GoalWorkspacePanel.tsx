import { useEffect, useMemo, useState } from "react";
import { RefreshButton } from "@/zorai/shell/RefreshButton";
import { fetchAgentTasks, type AgentQueueTask } from "@/lib/agentTaskQueue";
import { fetchThreadWorkContext } from "@/lib/agentWorkContext";
import { getDataDir, listPersistedDir } from "@/lib/persistence";
import { useThreadFilePreview } from "../threads/ThreadFilePreviewContext";
import {
  controlGoalRun,
  formatGoalRunStatus,
  summarizeGoalRunStep,
  type GoalRun,
  type GoalRunControlAction,
} from "@/lib/goalRuns";
import {
  buildGoalWorkspaceModel,
  goalFilesFromWorkContext,
  goalFileThreadIds,
  mergeGoalWorkspaceFiles,
  type GoalProjectionFile,
  type GoalWorkspaceAction,
  type GoalWorkspaceMode,
  type GoalWorkspaceRow,
  type GoalWorkspaceSection,
} from "./goalWorkspaceModel";

const GOAL_TASK_POLL_MS = 5_000;

function latestTaskLogId(task: AgentQueueTask): string {
  const logs = task.logs ?? [];
  return logs.length > 0 ? logs[logs.length - 1]?.id ?? "" : "";
}

function taskRenderFingerprint(tasks: AgentQueueTask[]): string {
  return tasks.map((task) => [
    task.id,
    task.status,
    task.progress,
    task.thread_id ?? "",
    task.blocked_reason ?? "",
    task.awaiting_approval_id ?? "",
    latestTaskLogId(task),
  ].join(":"))
    .sort()
    .join("|");
}

export function GoalWorkspacePanel({
  run,
  onRefresh,
  onMessage,
  onOpenThread,
}: {
  run: GoalRun | null;
  onRefresh: () => Promise<void>;
  onMessage: (message: string) => void;
  onOpenThread?: (threadId: string) => void | Promise<void>;
}) {
  const [mode, setMode] = useState<GoalWorkspaceMode>("work");
  const [selectedCenterIndex, setSelectedCenterIndex] = useState(0);
  const [promptExpanded, setPromptExpanded] = useState(false);
  const [projectionFiles, setProjectionFiles] = useState<GoalProjectionFile[]>([]);
  const [goalTasks, setGoalTasks] = useState<AgentQueueTask[]>([]);
  const { openThreadFilePreview } = useThreadFilePreview();
  const fileThreadKey = useMemo(
    () => (run ? goalFileThreadIds(run, goalTasks).join("\n") : ""),
    [goalTasks, run],
  );

  useEffect(() => {
    setMode(run?.status === "awaiting_review" ? "review" : "work");
    setSelectedCenterIndex(0);
    setPromptExpanded(false);
  }, [run?.id]);

  useEffect(() => {
    let cancelled = false;
    if (!run?.id || mode !== "files") {
      setProjectionFiles((current) => (current.length === 0 ? current : []));
      return () => {
        cancelled = true;
      };
    }

    const refreshFiles = async () => {
      const threadIds = fileThreadKey ? fileThreadKey.split("\n") : [];
      const [projection, contexts] = await Promise.all([
        loadGoalProjectionFiles(run.id),
        Promise.all(threadIds.map((threadId) => fetchThreadWorkContext(threadId))),
      ]);
      if (cancelled) return;
      const touched = goalFilesFromWorkContext(run.id, contexts.flatMap((context) => context.entries));
      const files = mergeGoalWorkspaceFiles(projection, touched);
      setProjectionFiles((current) => (
        goalFileFingerprint(current) === goalFileFingerprint(files) ? current : files
      ));
    };

    void refreshFiles();
    const timer = window.setInterval(() => void refreshFiles(), GOAL_TASK_POLL_MS);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [fileThreadKey, mode, run?.id]);

  useEffect(() => {
    let cancelled = false;
    if (!run?.id) {
      setGoalTasks((current) => (current.length === 0 ? current : []));
      return () => {
        cancelled = true;
      };
    }

    const refreshTasks = async () => {
      if (typeof document !== "undefined" && document.visibilityState === "hidden") return;
      const tasks = (await fetchAgentTasks()).filter((task) => task.goal_run_id === run.id);
      if (!cancelled) {
        setGoalTasks((current) => (
          taskRenderFingerprint(current) === taskRenderFingerprint(tasks) ? current : tasks
        ));
      }
    };
    void refreshTasks();
    const timer = window.setInterval(() => void refreshTasks(), GOAL_TASK_POLL_MS);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [run?.id]);

  const model = useMemo(() => run ? buildGoalWorkspaceModel(run, {
    mode,
    selectedCenterIndex,
    promptExpanded,
    projectionFiles,
    tasks: goalTasks,
  }) : null, [goalTasks, mode, projectionFiles, promptExpanded, run, selectedCenterIndex]);

  const control = async (action: GoalRunControlAction, explanation?: string) => {
    if (!run || !model) return;
    if (action === "cancel" && !window.confirm("Stop this goal run?")) return;
    if (action === "hard_reject" && !window.confirm("Hard reject this goal?")) return;
    let reason = explanation ?? "";
    if (action === "soft_reject" || action === "hard_reject") {
      if (!reason) {
        reason = window.prompt(action === "soft_reject" ? "Why should the worker keep going?" : "Why is this goal rejected?") ?? "";
      }
      if (!reason.trim()) return;
    }
    const ok = await controlGoalRun(run.id, action, null, reason || null);
    onMessage(ok ? `Goal ${action.replace(/_/g, " ")} requested.` : "Goal action failed.");
    await onRefresh();
  };

  const runFooterAction = async (action: GoalWorkspaceAction) => {
    if (action.id === "refresh") {
      await onRefresh();
      return;
    }
    if (action.id === "toggle") {
      await control(run?.status === "paused" ? "resume" : "pause");
      return;
    }
    if (action.id === "cancel") await control("cancel");
    if (action.id === "accept") await control("accept");
    if (action.id === "soft_reject") await control("soft_reject");
    if (action.id === "hard_reject") await control("hard_reject");
  };

  if (!run || !model) {
    return (
      <div className="zorai-goal-workspace-shell">
        <div className="zorai-panel zorai-empty-state">Select a goal run to inspect the worker and review dialogue.</div>
      </div>
    );
  }

  const handleSummaryRowClick = (row: GoalWorkspaceRow) => {
    if (handleTargetRow(row)) return;
    if (row.id === "goal-prompt") {
      setPromptExpanded((current) => !current);
    }
  };

  const handleTargetRow = (row: GoalWorkspaceRow) => {
    if (row.targetThreadId) {
      void onOpenThread?.(row.targetThreadId);
      return true;
    }
    if (row.targetFilePath) {
      openThreadFilePreview({
        path: row.targetFilePath,
        kind: "artifact",
        source: "goal",
        goalRunId: run?.id ?? null,
        isText: true,
        updatedAt: Date.now(),
      });
      return true;
    }
    return false;
  };

  return (
    <div className="zorai-goal-workspace-shell" aria-label="Goal workspace">
      <section className="zorai-panel zorai-goal-toolbar">
        <div>
          <div className="zorai-section-label">{model.footerTitle}</div>
          <strong>{model.statusLabel}</strong>
        </div>
        <div className="zorai-card-actions">
          {model.footerActions.map((action) => (
            action.id === "refresh" ? (
              <RefreshButton key={action.id} disabled={!action.enabled} onClick={() => void runFooterAction(action)} />
            ) : (
              <button
                key={action.id}
                type="button"
                className={action.id === "toggle" && action.label === "Resume" ? "zorai-primary-button" : "zorai-ghost-button"}
                onClick={() => void runFooterAction(action)}
                disabled={!action.enabled}
              >
                {action.label}
              </button>
            )
          ))}
        </div>
      </section>

      {run.status === "awaiting_review" ? (
        <section className="zorai-panel zorai-goal-review-banner" aria-label="Supervisor review">
          <div className="zorai-section-label">Worker report</div>
          <p className="zorai-goal-review-report">{run.pending_review_report || "The worker asked for supervisor review."}</p>
        </section>
      ) : null}

      <section className="zorai-panel zorai-goal-summary-pane">
        <div className="zorai-section-label">Goal</div>
        <RowList rows={model.summaryRows} onRowClick={handleSummaryRowClick} />
      </section>

      <div className="zorai-goal-workspace-main">
        <nav className="zorai-goal-tabs" aria-label="Goal views">
          {model.tabs.map((tab) => (
            <button
              type="button"
              key={tab.id}
              className={["zorai-goal-tab", tab.active ? "zorai-goal-tab--active" : ""].filter(Boolean).join(" ")}
              onClick={() => {
                setMode(tab.id);
                setSelectedCenterIndex(0);
              }}
            >
              {tab.label}
            </button>
          ))}
        </nav>
        <section className="zorai-panel zorai-goal-pane zorai-goal-workspace-pane">
          <div className="zorai-section-label">{model.centerTitle}</div>
          <div className="zorai-goal-pane__body">
            <RowList
              rows={model.centerRows}
              onRowClick={(row, index) => {
                setSelectedCenterIndex(index);
                handleTargetRow(row);
              }}
            />
            <SectionList sections={model.detailSections} onRowClick={handleTargetRow} />
          </div>
        </section>
        <div className="zorai-goal-workspace-status">
          <span className="zorai-status-pill">{formatGoalRunStatus(run.status)}</span>
          <span>{summarizeGoalRunStep(run)}</span>
        </div>
      </div>

    </div>
  );
}

function RowList({
  rows,
  onRowClick,
}: {
  rows: GoalWorkspaceRow[];
  onRowClick?: (row: GoalWorkspaceRow, index: number) => void;
}) {
  const [expandedTextRows, setExpandedTextRows] = useState<Set<string>>(() => new Set());
  return (
    <div className="zorai-goal-item-list">
      {rows.map((row, index) => {
        const expanded = expandedTextRows.has(row.id);
        const longText = row.text.length > 420;
        const text = longText && !expanded
          ? `${row.text.slice(0, 420).trimEnd()}…`
          : row.text;
        return (
        <button
          key={`${row.id}-${index}`}
          type="button"
          className={[
            "zorai-goal-item",
            `zorai-row-tone--${row.tone ?? "normal"}`,
            row.selected ? "zorai-goal-item--selected" : "",
          ].filter(Boolean).join(" ")}
          style={{ paddingLeft: `${10 + (row.depth ?? 0) * 16}px` }}
          onClick={() => onRowClick?.(row, index)}
        >
          <span
            className={[
              "zorai-goal-indicator",
              row.working ? "zorai-goal-indicator--working" : "",
            ].filter(Boolean).join(" ")}
            aria-label={row.indicatorLabel}
          />
          <span className="zorai-goal-item__body">
            <span
              className={[
                "zorai-goal-item__text",
                expanded ? "zorai-goal-item__text--expanded" : "",
              ].filter(Boolean).join(" ")}
            >
              {text}
            </span>
            {longText ? (
              <span
                className="zorai-goal-item__text-toggle"
                role="button"
                tabIndex={0}
                onClick={(event) => {
                  event.stopPropagation();
                  setExpandedTextRows((current) => {
                    const next = new Set(current);
                    if (next.has(row.id)) next.delete(row.id);
                    else next.add(row.id);
                    return next;
                  });
                }}
                onKeyDown={(event) => {
                  if (event.key === "Enter" || event.key === " ") event.currentTarget.click();
                }}
              >
                {expanded ? "Show less" : "Show full text"}
              </span>
            ) : null}
            {row.meta ? <span className="zorai-goal-item__meta">{row.meta}</span> : null}
            {typeof row.progress === "number" ? (
              <span className="zorai-goal-progress" aria-label={`${row.indicatorLabel ?? "Progress"} ${row.progress}%`}>
                <span className="zorai-goal-progress__bar" style={{ width: `${Math.max(0, Math.min(100, row.progress))}%` }} />
              </span>
            ) : null}
          </span>
        </button>
        );
      })}
    </div>
  );
}

function SectionList({
  sections,
  onRowClick,
}: {
  sections: GoalWorkspaceSection[];
  onRowClick?: (row: GoalWorkspaceRow, index: number) => void;
}) {
  if (sections.length === 0) return null;
  return (
    <div className="zorai-goal-detail-sections">
      {sections.map((section) => (
        <section key={section.title} className="zorai-goal-detail-section">
          <h3>{section.title}</h3>
          <RowList rows={section.rows} onRowClick={onRowClick} />
        </section>
      ))}
    </div>
  );
}

function goalFileFingerprint(files: GoalProjectionFile[]): string {
  return files.map((file) => `${file.absolutePath}:${file.source ?? ""}:${file.sizeBytes ?? ""}`).join("|");
}

async function loadGoalProjectionFiles(goalRunId: string): Promise<GoalProjectionFile[]> {
  const dataDir = await getDataDir();
  if (!dataDir) return [];
  const root = `goals/${goalRunId}`;
  const files: GoalProjectionFile[] = [];
  const visit = async (relativeDir: string) => {
    const entries = await listPersistedDir(relativeDir);
    for (const entry of entries) {
      if (entry.isDirectory) {
        await visit(entry.path);
      } else {
        files.push({
          relativePath: entry.path.startsWith(`${root}/`) ? entry.path.slice(root.length + 1) : entry.path,
          absolutePath: `${dataDir.replace(/\/$/, "")}/${entry.path}`,
          sizeBytes: null,
        });
      }
    }
  };
  await visit(root);
  return files.sort((a, b) => a.relativePath.localeCompare(b.relativePath));
}
