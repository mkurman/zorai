export type WorkspaceRootState = {
  entries: ZoraiWorkspaceEntry[];
  statuses: ZoraiWorkspaceGitStatus[];
  overview: ZoraiWorkspaceGitOverview | null;
  worktrees: ZoraiGitWorktree[];
  history: Array<{ hash: string; shortHash: string; author: string; date: string; subject: string }>;
  conflicts: Array<{ path: string }>;
};

const rootStateCache = new Map<string, WorkspaceRootState>();

export function cachedWorkspaceRootState(root: string | null | undefined): WorkspaceRootState | null {
  return root ? rootStateCache.get(root) ?? null : null;
}

export function rememberWorkspaceRootState(root: string, patch: Partial<WorkspaceRootState>): void {
  const current = rootStateCache.get(root) ?? { entries: [], statuses: [], overview: null, worktrees: [], history: [], conflicts: [] };
  rootStateCache.set(root, { ...current, ...patch });
}

export type WorkspaceRefreshBridge = Pick<ZoraiBridge,
  "workspaceGitStatus" |
  "workspaceGitOverview" |
  "workspaceGitListWorktrees" |
  "workspaceGitHistory" |
  "workspaceGitConflicts"
> & Required<Pick<ZoraiBridge, "workspaceListDirectory">>;

export async function loadWorkspaceRootState(bridge: WorkspaceRefreshBridge, root: string): Promise<WorkspaceRootState & { gitError: string | null }> {
  let gitError: string | null = null;
  const optional = <T,>(load: Promise<T> | undefined, fallback: T): Promise<T> => (load ?? Promise.resolve(fallback)).catch((reason) => {
    gitError ??= reason?.message ?? String(reason);
    return fallback;
  });
  const [entries, statuses, overview, worktrees, history, conflicts] = await Promise.all([
    bridge.workspaceListDirectory(root, ""),
    optional(bridge.workspaceGitStatus?.(root), []),
    optional(bridge.workspaceGitOverview?.(root), null),
    optional(bridge.workspaceGitListWorktrees?.(root), []),
    optional(bridge.workspaceGitHistory?.(root, { limit: 50 }), []),
    optional(bridge.workspaceGitConflicts?.(root), []),
  ]);
  return { entries, statuses, overview, worktrees, history, conflicts, gitError };
}

export async function runWorkspacePathMutation<T>(mutate: () => Promise<T>, refreshRoot: () => Promise<void>): Promise<T> {
  const result = await mutate();
  await refreshRoot();
  return result;
}

export type WorkspaceGitRefreshBridge = Pick<ZoraiBridge, "workspaceGitStatus" | "workspaceGitOverview">;

export type WorkspaceGitState = {
  statuses: ZoraiWorkspaceGitStatus[];
  overview: ZoraiWorkspaceGitOverview | null;
};

/**
 * Targeted git-only refresh: 2 IPC calls instead of the 6-call full root
 * reload. Use after stage/unstage/commit/checkout mutations so the change
 * list updates in one round-trip without re-walking the file tree.
 */
export async function loadWorkspaceGitState(bridge: WorkspaceGitRefreshBridge, root: string): Promise<WorkspaceGitState> {
  const [statuses, overview] = await Promise.all([
    bridge.workspaceGitStatus?.(root) ?? Promise.resolve([]),
    bridge.workspaceGitOverview?.(root) ?? Promise.resolve(null),
  ]);
  return { statuses, overview };
}

export type WorkspaceGraphCommit = {
  hash: string;
  shortHash: string;
  author: string;
  date: string;
  subject: string;
  parents: string[];
  refs: string[];
};

export async function loadWorkspaceGraphHistory(
  bridge: Pick<ZoraiBridge, "workspaceGitHistory">,
  root: string,
  limit = 200,
): Promise<WorkspaceGraphCommit[]> {
  if (!bridge.workspaceGitHistory) return [];
  const commits = await bridge.workspaceGitHistory(root, { limit, graph: true });
  return commits.map((commit) => ({
    ...commit,
    parents: commit.parents ?? [],
    refs: commit.refs ?? [],
  }));
}
