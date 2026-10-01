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
