import { useEffect, useMemo, useRef, useState } from "react";
import { RefreshButton } from "@/zorai/shell/RefreshButton";
import { fetchAgentTasksFiltered, type AgentQueueTask } from "@/lib/agentTaskQueue";
import { fetchThreadWorkContext } from "@/lib/agentWorkContext";
import { getDataDir, listPersistedDir } from "@/lib/persistence";
import { useThreadFilePreview } from "../threads/ThreadFilePreviewContext";
import {
  controlGoalRun,
  type GoalRun,
  type GoalRunControlAction,
} from "@/lib/goalRuns";
import { normalizeGoalText } from "./goalPresentation";
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
  const projectionFilesRef = useRef(projectionFiles);
  projectionFilesRef.current = projectionFiles;
  const goalTasksRef = useRef(goalTasks);
  goalTasksRef.current = goalTasks;
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
      if (goalFileFingerprint(projectionFilesRef.current) !== goalFileFingerprint(files)) setProjectionFiles(files);
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
      const tasks = await fetchAgentTasksFiltered({ goalRunId: run.id });
      if (!cancelled && taskRenderFingerprint(goalTasksRef.current) !== taskRenderFingerprint(tasks)) {
        setGoalTasks(tasks);
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

  const summaryLinks = model.summaryRows.filter((row) => row.id !== "goal-prompt-body");
  const promptBodyRow = model.summaryRows.find((row) => row.id === "goal-prompt-body");
  const hasDetailSections = model.detailSections.length > 0;

  return (
    <div className="zorai-goal-workspace-shell" aria-label="Goal workspace">
          <div className="zorai-goal-controlbar">
            <div className="zorai-goal-controlbar__links" aria-label="Goal links">
              {summaryLinks.map((row) => {
                const clickable = row.id === "goal-prompt" || Boolean(row.targetThreadId) || Boolean(row.targetFilePath);
                return (
                  <button
                    key={row.id}
                    type="button"
                    className={["zorai-goal-link-chip", row.id === "goal-prompt" && promptExpanded ? "is-active" : ""].filter(Boolean).join(" ")}
                    disabled={!clickable}
                    onClick={() => handleSummaryRowClick(row)}
                  >
                    <span>{row.text}</span>
                    {row.meta ? <small>{row.id === "goal-prompt" ? (promptExpanded ? "Hide" : "Show") : "↗"}</small> : null}
                  </button>
                );
              })}
            </div>
              <div className="zorai-goal-controlbar__actions" aria-label={model.footerTitle}>
                {model.footerActions.map((action) => (
                  action.id === "refresh" ? (
                    <RefreshButton key={action.id} disabled={!action.enabled} onClick={() => void runFooterAction(action)} />
                  ) : (
                    <button
                      key={action.id}
                      type="button"
                      className={[
                        "zorai-ghost-button",
                        action.id === "cancel" || action.id === "hard_reject" ? "zorai-ghost-button--danger" : "",
                        action.id === "accept" || (action.id === "toggle" && action.label === "Resume") ? "zorai-ghost-button--primary-soft" : "",
                      ].filter(Boolean).join(" ")}
                      onClick={() => void runFooterAction(action)}
                      disabled={!action.enabled}
                    >
                      {action.label}
                    </button>
                  )
                ))}
              </div>
          </div>

          {promptExpanded && promptBodyRow ? (
            <section className="zorai-goal-prompt-drawer" aria-label="Goal prompt">
              <p>{normalizeGoalText(promptBodyRow.text)}</p>
            </section>
          ) : null}

          {run.status === "awaiting_review" ? (
            <section className="zorai-panel zorai-goal-review-banner" aria-label="Supervisor review">
              <div className="zorai-section-label">Worker report</div>
              <p className="zorai-goal-review-report">{normalizeGoalText(run.pending_review_report || "The worker asked for supervisor review.")}</p>
            </section>
          ) : null}

          <div className="zorai-goal-workspace-main">
              <nav className="zorai-goal-tabs" role="tablist" aria-label="Goal views">
                {model.tabs.map((tab) => (
                  <button
                    type="button"
                    role="tab"
                    aria-selected={tab.active}
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
                  <section className={["zorai-panel", "zorai-goal-pane", "zorai-goal-workspace-pane", hasDetailSections ? "zorai-goal-pane--split" : ""].filter(Boolean).join(" ")}>
                    <div className="zorai-goal-pane__column">
                      <div className="zorai-section-label">{model.centerTitle}</div>
                      <div className="zorai-goal-pane__body">
                        <RowList
                          rows={model.centerRows}
                          onRowClick={(row, index) => {
                            setSelectedCenterIndex(index);
                            handleTargetRow(row);
                          }}
                        />
                      </div>
                    </div>
                    {hasDetailSections ? (
                      <div className="zorai-goal-pane__detail">
                        <SectionList sections={model.detailSections} onRowClick={handleTargetRow} />
                      </div>
                    ) : null}
                  </section>
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
                    if (row.id === "todos-label") {
                      return (
                        <div key={`${row.id}-${index}`} className="zorai-goal-subhead">
                          {row.text}
                        </div>
                      );
                    }
                    const normalizedText = normalizeGoalText(row.text);
                    const expanded = expandedTextRows.has(row.id);
                    const longText = normalizedText.length > 420;
                    const text = longText && !expanded
          ? `${normalizedText.slice(0, 420).trimEnd()}…`
                    : normalizedText;
        const showProgress = typeof row.progress === "number" && row.progress > 0 && row.progress < 100;
                  return (
                  <button
                    key={`${row.id}-${index}`}
                    type="button"
                    className={[
                      "zorai-goal-item",
                      `zorai-row-tone--${row.tone ?? "normal"}`,
                      row.selected ? "zorai-goal-item--selected" : "",
                    ].filter(Boolean).join(" ")}
                    style={row.depth ? { paddingLeft: `${12 + row.depth * 14}px` } : undefined}
                    onClick={() => onRowClick?.(row, index)}
                  >
                    <span
                      className={[
                        "zorai-goal-indicator",
                        `zorai-goal-indicator--${row.tone ?? "normal"}`,
                        row.working ? "zorai-goal-indicator--working" : "",
                      ].filter(Boolean).join(" ")}
                      aria-label={row.indicatorLabel}
                    />
                    <span className="zorai-goal-item__body">
                        <span className="zorai-goal-item__headline">
                          <span
                            className={[
                              "zorai-goal-item__text",
                              expanded ? "zorai-goal-item__text--expanded" : "",
                            ].filter(Boolean).join(" ")}
                          >
                            {text}
                          </span>
                          {row.meta ? <span className="zorai-goal-item__meta">{row.meta}</span> : null}
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
                        {showProgress ? (
                          <span className="zorai-goal-progress" aria-label={`${row.indicatorLabel ?? "Progress"} ${row.progress}%`}>
                            <span className="zorai-goal-progress__bar" style={{ width: `${Math.max(0, Math.min(100, row.progress ?? 0))}%` }} />
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
