export type WorkspaceFileIndexBridge = Pick<ZoraiBridge, "workspaceListDirectory">;

export const WORKSPACE_FILE_INDEX_MAX_FILES = 8000;
export const WORKSPACE_FILE_INDEX_MAX_DIRS = 1200;

/**
 * Bounded breadth-first file index for Quick Open.
 * Respects the Electron-side IGNORED_DIRECTORY_NAMES (the bridge already
 * omits .git/node_modules/target/dist/build/.venv etc.) so we can walk
 * without extra filtering. Stops when `maxFiles` or `maxDirs` is reached.
 * Returns POSIX-style `entry.path` values (relative to root) for files only.
 */
export async function collectWorkspaceFiles(
  bridge: WorkspaceFileIndexBridge,
  root: string,
  options: { maxFiles?: number; maxDirs?: number; signal?: AbortSignal } = {},
): Promise<string[]> {
  if (!bridge.workspaceListDirectory || !root) return [];
  const maxFiles = Math.max(1, Math.min(options.maxFiles ?? WORKSPACE_FILE_INDEX_MAX_FILES, 20000));
  const maxDirs = Math.max(1, Math.min(options.maxDirs ?? WORKSPACE_FILE_INDEX_MAX_DIRS, 5000));
  const files: string[] = [];
  const pending: string[] = [""];
  let visitedDirs = 0;

  while (pending.length > 0 && files.length < maxFiles && visitedDirs < maxDirs) {
    if (options.signal?.aborted) break;
    // Breadth-first: shift preserves top-level priority for Quick Open ranking.
    const relativeDir = pending.shift() as string;
    visitedDirs += 1;
    let entries: ZoraiWorkspaceEntry[];
    try {
      entries = await bridge.workspaceListDirectory(root, relativeDir);
    } catch {
      continue;
    }
    for (const entry of entries) {
      if (entry.isDirectory) {
        if (pending.length + visitedDirs < maxDirs) pending.push(entry.path);
      } else if (!entry.isDirectory) {
        files.push(entry.path);
        if (files.length >= maxFiles) break;
      }
    }
  }

  return files;
}

export type WorkspaceFileIndex = {
  root: string;
  files: string[];
  at: number;
};

/**
 * Per-root file-index cache. TTL + invalidate on demand (e.g. file watcher).
 */
const indexByRoot = new Map<string, WorkspaceFileIndex>();
const inflightByRoot = new Map<string, Promise<string[]>>();

export function getCachedWorkspaceFiles(root: string): string[] | null {
  const entry = indexByRoot.get(root);
  return entry ? entry.files : null;
}

export function invalidateWorkspaceFileIndex(root: string): void {
  indexByRoot.delete(root);
}

export function cachedWorkspaceFileIndexAge(root: string): number | null {
  const entry = indexByRoot.get(root);
  return entry ? Date.now() - entry.at : null;
}

export async function getWorkspaceFiles(
  bridge: WorkspaceFileIndexBridge,
  root: string,
  options: { force?: boolean } = {},
): Promise<string[]> {
  if (!root || !bridge.workspaceListDirectory) return [];
  if (!options.force) {
    const cached = indexByRoot.get(root);
    if (cached) return cached.files;
  }
  const inflight = inflightByRoot.get(root);
  if (inflight && !options.force) return inflight;

  const promise = collectWorkspaceFiles(bridge, root).then((files) => {
    indexByRoot.set(root, { root, files, at: Date.now() });
    return files;
  }).finally(() => {
    if (inflightByRoot.get(root) === promise) inflightByRoot.delete(root);
  });

  inflightByRoot.set(root, promise);
  return promise;
}
