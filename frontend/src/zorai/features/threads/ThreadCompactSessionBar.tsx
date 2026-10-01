import { useCallback, useEffect, useMemo, useState } from "react";
import { useAgentChatPanelRuntime } from "@/components/agent-chat-panel/runtime/context";
import { formatRunStatus, isRunActive, runStatusColor, type AgentRun } from "@/lib/agentRuns";
import {
  fetchGitDiff,
  fetchThreadWorkContext,
  type WorkContextEntry,
} from "@/lib/agentWorkContext";
import { getBridge } from "@/lib/bridge";
import { useWorkspaceContextStore } from "@/lib/workspaceContextStore";
import { useWorkspaceEditorRequestStore } from "@/lib/workspaceEditorRequestStore";
import {
  compactSessionHasContent,
  countDiffStats,
  flattenSpawnedRuns,
  gitStatusMatchesPath,
  isRelativeWorkspacePath,
  isUntrackedGitStatus,
  rejectUsesOperationSnapshot,
  sameFilesystemPath,
  sumLineStats,
  todoProgress,
  untrackedContentStats,
  workContextFileName,
  workContextRelativePath,
  type CompactSessionSection,
  type FileLineStats,
} from "./compactSessionModel";
import { useThreadFilePreview } from "./ThreadFilePreviewContext";

const MAX_TRACKED_FILES = 40;

export function ThreadCompactSessionBar() {
  const runtime = useAgentChatPanelRuntime();
  const daemonThreadId = runtime.activeThread?.daemonThreadId ?? null;
  const localThreadId = runtime.activeThreadId;
  const workspaceRoot = useWorkspaceContextStore((state) => (
    localThreadId ? state.byThreadId[localThreadId]?.root ?? null : null
  ));
  const requestFileView = useWorkspaceEditorRequestStore((state) => state.requestFileView);
  const { openThreadFilePreview } = useThreadFilePreview();
  const [entries, setEntries] = useState<WorkContextEntry[]>([]);
  const [statsByPath, setStatsByPath] = useState<Record<string, FileLineStats>>({});
  const [busyPath, setBusyPath] = useState<string | null>(null);
  const [openSection, setOpenSection] = useState<CompactSessionSection | null>(null);
  const spawned = useMemo(
    () => flattenSpawnedRuns(runtime.spawnedAgentTree),
    [runtime.spawnedAgentTree],
  );
  const todos = runtime.todos;
  const progress = todoProgress(todos);

  const refresh = useCallback(async () => {
    if (!daemonThreadId) {
      setEntries([]);
      return;
    }
    const next = await fetchThreadWorkContext(daemonThreadId);
    setEntries(next.entries.slice(0, MAX_TRACKED_FILES));
  }, [daemonThreadId]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    const bridge = getBridge();
    if (!daemonThreadId || !bridge?.onAgentEvent) return;
    return bridge.onAgentEvent((event: { type?: string; thread_id?: string }) => {
      if (event?.type !== "work_context_update" || event.thread_id !== daemonThreadId) return;
      void refresh();
    });
  }, [daemonThreadId, refresh]);

  useEffect(() => {
    let cancelled = false;
    const loadStats = async () => {
      const next: Record<string, FileLineStats> = {};
      for (const entry of entries) {
        const root = entry.repoRoot ?? workspaceRoot;
        if (!root) continue;
        next[entry.path] = await loadFileLineStats(root, workContextRelativePath(entry.path, root));
      }
      if (!cancelled) setStatsByPath(next);
    };
    void loadStats();
    return () => {
      cancelled = true;
    };
  }, [entries, workspaceRoot]);

  if (!compactSessionHasContent(entries, todos, spawned)) {
    return null;
  }

  const toggle = (section: CompactSessionSection) => {
    setOpenSection((current) => (current === section ? null : section));
  };

  const openFile = (entry: WorkContextEntry) => {
    const gitRoot = entry.repoRoot ?? workspaceRoot;
    const workspacePath = workspaceRoot
      ? workContextRelativePath(entry.path, workspaceRoot)
      : gitRoot
        ? workContextRelativePath(entry.path, gitRoot)
        : entry.path;
    if (
      localThreadId
      && workspaceRoot
      && isRelativeWorkspacePath(workspacePath)
      && (!gitRoot || sameFilesystemPath(gitRoot, workspaceRoot))
    ) {
      requestFileView(localThreadId, workspacePath, "diff");
      return;
    }
    openThreadFilePreview(entry);
  };

  const keepFile = async (entry: WorkContextEntry) => {
    const root = entry.repoRoot ?? workspaceRoot;
    const relative = root ? workContextRelativePath(entry.path, root) : entry.path;
    const bridge = getBridge();
    setBusyPath(entry.path);
    try {
      if (root && bridge?.workspaceGitStage) {
        await bridge.workspaceGitStage(root, relative);
      }
    } finally {
      setBusyPath(null);
    }
  };

  const rejectFile = async (entry: WorkContextEntry, prompt = true) => {
    if (prompt && !window.confirm(`Reject changes in ${workContextFileName(entry.path)}?`)) return;
    const root = entry.repoRoot ?? workspaceRoot;
    const relative = root ? workContextRelativePath(entry.path, root) : entry.path;
    const bridge = getBridge();
    setBusyPath(entry.path);
    try {
      if (rejectUsesOperationSnapshot(entry) && entry.operationId && bridge?.agentRevertFileOperation) {
        await bridge.agentRevertFileOperation(entry.operationId);
      } else if (root && bridge?.workspaceGitDiscard) {
        await bridge.workspaceGitDiscard(root, relative);
      }
      await refresh();
    } finally {
      setBusyPath(null);
    }
  };

  const rejectAll = async () => {
    if (!window.confirm(`Reject all ${entries.length} file changes?`)) return;
    for (const entry of entries) {
      await rejectFile(entry, false);
    }
  };

  const keepAll = async () => {
    for (const entry of entries) {
      await keepFile(entry);
    }
  };

  return (
    <div className="zorai-compact-session">
      {entries.length > 0 ? (
        <section>
          <div className="zorai-compact-session__row">
            <button type="button" className="zorai-compact-session__toggle" aria-expanded={openSection === "files"} onClick={() => toggle("files")}>
              <span aria-hidden="true">{openSection === "files" ? "▾" : "▸"}</span>
              {entries.length} {entries.length === 1 ? "File" : "Files"}
            </button>
            <div className="zorai-compact-session__actions">
              <button type="button" className="zorai-compact-session__keep" disabled={Boolean(busyPath)} onClick={() => void keepAll()}>
                Keep All
              </button>
              <button type="button" className="zorai-compact-session__reject" disabled={Boolean(busyPath)} onClick={() => void rejectAll()}>
                Reject All
              </button>
              <button type="button" className="zorai-compact-session__review" onClick={() => setOpenSection("files")}>
                Review
              </button>
            </div>
          </div>
          {openSection === "files" ? (
            <ul className="zorai-compact-session__list">
              {entries.map((entry) => {
                const stats = statsByPath[entry.path];
                return (
                  <li key={`${entry.path}:${entry.updatedAt}`} className="zorai-compact-session__file">
                    <button type="button" className="zorai-compact-session__name" title={entry.path} onClick={() => openFile(entry)}>
                      {workContextFileName(entry.path)}
                    </button>
                    <span className="zorai-compact-session__stats">
                      {stats ? (
                        <>
                          <b className="zorai-compact-session__plus">+{stats.additions}</b>
                          <b className="zorai-compact-session__minus">−{stats.deletions}</b>
                        </>
                      ) : (
                        <span>{entry.changeKind ?? "changed"}</span>
                      )}
                    </span>
                    <span className="zorai-compact-session__file-actions">
                      <button type="button" disabled={busyPath === entry.path} onClick={() => void keepFile(entry)}>Keep</button>
                      <button type="button" disabled={busyPath === entry.path} onClick={() => void rejectFile(entry)}>Reject</button>
                    </span>
                  </li>
                );
              })}
            </ul>
          ) : null}
        </section>
      ) : null}

      {todos.length > 0 ? (
        <section>
          <button type="button" className="zorai-compact-session__row zorai-compact-session__toggle" aria-expanded={openSection === "todos"} onClick={() => toggle("todos")}>
            <span aria-hidden="true">{openSection === "todos" ? "▾" : "▸"}</span>
            {progress.done}/{progress.total} Todos
          </button>
          {openSection === "todos" ? (
            <ul className="zorai-compact-session__list">
              {todos.map((todo) => (
                <li key={todo.id} className="zorai-compact-session__todo">
                  <span className={`zorai-compact-session__check zorai-compact-session__check--${todo.status}`} aria-hidden="true" />
                  <span>{todo.content}</span>
                </li>
              ))}
            </ul>
          ) : null}
        </section>
      ) : null}

      {spawned.length > 0 ? (
        <section>
          <button type="button" className="zorai-compact-session__row zorai-compact-session__toggle" aria-expanded={openSection === "spawned"} onClick={() => toggle("spawned")}>
            <span aria-hidden="true">{openSection === "spawned" ? "▾" : "▸"}</span>
            {spawned.length} {spawned.length === 1 ? "Agent" : "Agents"}
          </button>
          {openSection === "spawned" ? (
            <ul className="zorai-compact-session__list">
              {spawned.map((item) => (
                <SpawnedRow
                  key={item.id}
                  run={item}
                  canOpen={runtime.canOpenSpawnedThread(item)}
                  onOpen={() => void runtime.openSpawnedThread(item)}
                />
              ))}
            </ul>
          ) : null}
        </section>
      ) : null}
    </div>
  );
}

function SpawnedRow({
  run,
  canOpen,
  onOpen,
}: {
  run: AgentRun;
  canOpen: boolean;
  onOpen: () => void;
}) {
  return (
    <li className={isRunActive(run) ? "zorai-compact-session__spawned zorai-compact-session__spawned--live" : "zorai-compact-session__spawned"}>
      <button type="button" className="zorai-compact-session__spawned-main" disabled={!canOpen} onClick={onOpen}>
        <strong>{run.description.trim() || run.title}</strong>
        <span style={{ color: runStatusColor(run.status) }}>{formatRunStatus(run)}</span>
      </button>
    </li>
  );
}

async function loadFileLineStats(root: string, relativePath: string): Promise<FileLineStats> {
  const empty: FileLineStats = { additions: 0, deletions: 0 };
  const bridge = getBridge();
  if (bridge?.workspaceGitHunks) {
    try {
      const [unstaged, staged] = await Promise.all([
        bridge.workspaceGitHunks(root, relativePath, { staged: false }),
        bridge.workspaceGitHunks(root, relativePath, { staged: true }),
      ]);
      const fromHunks = sumLineStats([...(unstaged ?? []), ...(staged ?? [])]);
      if (fromHunks.additions || fromHunks.deletions) return fromHunks;
    } catch {
      /* fall through to HEAD / untracked diffs */
    }
  }
  if (bridge?.workspaceGitDiff) {
    try {
      const patch = await bridge.workspaceGitDiff(root, relativePath, { againstHead: true, includeUntracked: true });
      const fromPatch = countDiffStats(patch ?? "");
      if (fromPatch.additions || fromPatch.deletions) return fromPatch;
    } catch {
      /* fall through */
    }
  }
  const fromDaemon = countDiffStats(await fetchGitDiff(root, relativePath));
  if (fromDaemon.additions || fromDaemon.deletions) return fromDaemon;
  if (bridge?.gitDiff) {
    try {
      const patch = await bridge.gitDiff(root, relativePath);
      const fromGit = countDiffStats(typeof patch === "string" ? patch : "");
      if (fromGit.additions || fromGit.deletions) return fromGit;
    } catch {
      /* fall through */
    }
  }
  if (bridge?.workspaceGitStatus && bridge.workspaceReadFile) {
    try {
      const statuses = await bridge.workspaceGitStatus(root);
      const match = statuses.find((entry) => gitStatusMatchesPath(entry.path, relativePath));
      if (match && isUntrackedGitStatus({ indexStatus: match.indexStatus, worktreeStatus: match.worktreeStatus })) {
        const file = await bridge.workspaceReadFile(root, relativePath);
        return untrackedContentStats(file.content);
      }
    } catch {
      /* ignore */
    }
  }
  return empty;
}
