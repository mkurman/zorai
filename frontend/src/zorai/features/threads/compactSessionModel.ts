import type { AgentTodoItem } from "@/lib/agentStore";
import type { AgentRun } from "@/lib/agentRuns";
import type { WorkContextEntry } from "@/lib/agentWorkContext";
import type { SpawnedAgentTree, SpawnedAgentTreeNode } from "@/lib/spawnedAgentTree";

export type CompactSessionSection = "files" | "todos" | "spawned";

export type FileLineStats = {
  additions: number;
  deletions: number;
};

export function workContextFileName(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] || path;
}

export function workContextRelativePath(path: string, root: string | null | undefined): string {
  if (!root) return path;
  const normalizedRoot = root.replace(/[/\\]+$/, "");
  if (path === normalizedRoot) return "";
  const prefix = `${normalizedRoot}/`;
  const winPrefix = `${normalizedRoot}\\`;
  if (path.startsWith(prefix)) return path.slice(prefix.length);
  if (path.startsWith(winPrefix)) return path.slice(winPrefix.length);
  return path;
}

export function sumLineStats(items: Array<{ additions?: number; deletions?: number }>): FileLineStats {
  return items.reduce<FileLineStats>(
    (total, item) => ({
      additions: total.additions + Math.max(0, item.additions ?? 0),
      deletions: total.deletions + Math.max(0, item.deletions ?? 0),
    }),
    { additions: 0, deletions: 0 },
  );
}

export function countDiffStats(diff: string): FileLineStats {
  let additions = 0;
  let deletions = 0;
  for (const line of diff.split("\n")) {
    if (line.startsWith("+++") || line.startsWith("---") || line.startsWith("@@") || line.startsWith("diff ")) {
      continue;
    }
    if (line.startsWith("+")) additions += 1;
    else if (line.startsWith("-")) deletions += 1;
  }
  return { additions, deletions };
}

export function todoProgress(todos: AgentTodoItem[]): { done: number; total: number } {
  return {
    done: todos.filter((todo) => todo.status === "completed").length,
    total: todos.length,
  };
}

export function flattenSpawnedRuns(tree: SpawnedAgentTree<AgentRun> | null): AgentRun[] {
  if (!tree) return [];
  const runs: AgentRun[] = [];
  const seen = new Set<string>();
  const visit = (node: SpawnedAgentTreeNode<AgentRun>) => {
    if (!seen.has(node.item.id)) {
      seen.add(node.item.id);
      runs.push(node.item);
    }
    node.children.forEach(visit);
  };
  if (tree.anchor) {
    tree.anchor.children.forEach(visit);
  }
  tree.roots.forEach(visit);
  return runs;
}

export function compactSessionHasContent(
  files: WorkContextEntry[],
  todos: AgentTodoItem[],
  spawned: AgentRun[],
): boolean {
  return files.length > 0 || todos.length > 0 || spawned.length > 0;
}

export function pendingWorkContextEntries(
  entries: WorkContextEntry[],
  statusesByRoot: Record<string, Array<{ path: string; worktreeStatus: string }> | undefined>,
  workspaceRoot: string | null | undefined,
): WorkContextEntry[] {
  return entries.filter((entry) => {
    const root = entry.repoRoot ?? workspaceRoot;
    if (!root) return false;
    const relative = workContextRelativePath(entry.path, root);
    if (!isRelativeWorkspacePath(relative)) return false;
    const match = statusesByRoot[root]?.find((status) => gitStatusMatchesPath(status.path, relative));
    return Boolean(match?.worktreeStatus.trim());
  });
}

export function rejectUsesOperationSnapshot(entry: Pick<WorkContextEntry, "operationId">): boolean {
  return Boolean(entry.operationId);
}

function normalizeFsPath(value: string): string {
  return value.replace(/\\/g, "/").replace(/\/+$/, "");
}

export function sameFilesystemPath(left: string | null | undefined, right: string | null | undefined): boolean {
  if (!left || !right) return false;
  return normalizeFsPath(left) === normalizeFsPath(right);
}

export function isRelativeWorkspacePath(path: string): boolean {
  const trimmed = path.trim();
  if (!trimmed) return false;
  if (trimmed.startsWith("/") || trimmed.startsWith("\\")) return false;
  return !/^[A-Za-z]:[\\/]/.test(trimmed);
}

export function gitStatusMatchesPath(statusPath: string, relativePath: string): boolean {
  const left = normalizeFsPath(statusPath);
  const right = normalizeFsPath(relativePath);
  return left === right || left.endsWith(`/${right}`) || right.endsWith(`/${left}`);
}

export function isUntrackedGitStatus(
  entry: { indexStatus?: string; worktreeStatus?: string; previousPath?: string | null } | undefined,
): boolean {
  if (!entry) return false;
  return entry.worktreeStatus === "?" || entry.indexStatus === "?";
}

export function untrackedContentStats(content: string): FileLineStats {
  if (!content) return { additions: 0, deletions: 0 };
  const lines = content.split(/\r?\n/);
  const count = lines[lines.length - 1] === "" ? Math.max(0, lines.length - 1) : lines.length;
  return { additions: count, deletions: 0 };
}
