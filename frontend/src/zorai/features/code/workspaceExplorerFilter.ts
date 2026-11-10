export type ExplorerFilterEntry = {
  name: string;
  path: string;
};

export function filterExplorerEntries<T extends ExplorerFilterEntry>(
  entries: readonly T[],
  query: string,
  childrenOf: (path: string) => readonly T[],
): T[] {
  const needle = query.trim().toLowerCase();
  if (!needle) return [...entries];
  const visible = (entry: T): boolean => {
    if (entry.name.toLowerCase().includes(needle) || entry.path.toLowerCase().includes(needle)) return true;
    return childrenOf(entry.path).some(visible);
  };
  return entries.filter(visible);
}

function wildcardPattern(needle: string): RegExp | null {
  if (!/[*?]/.test(needle)) return null;
  const source = needle.replace(/[.+^${}()|[\]\\]/g, "\\$&").replace(/\*/g, ".*").replace(/\?/g, ".");
  return new RegExp(`^${source}$`);
}

export function searchWorkspaceFiles(files: readonly string[], query: string, limit = 200): string[] {
  const needle = query.trim().toLowerCase();
  if (!needle) return [];
  const pattern = wildcardPattern(needle);
  const matchesPath = pattern && needle.includes("/");
  const nameMatches: string[] = [];
  const pathMatches: string[] = [];
  for (const file of files) {
    const lower = file.toLowerCase();
    const name = lower.split(/[\\/]/).pop() ?? lower;
    if (pattern) {
      if (pattern.test(matchesPath ? lower : name)) nameMatches.push(file);
      continue;
    }
    if (!lower.includes(needle)) continue;
    (name.includes(needle) ? nameMatches : pathMatches).push(file);
  }
  return [...nameMatches, ...pathMatches].slice(0, limit);
}
