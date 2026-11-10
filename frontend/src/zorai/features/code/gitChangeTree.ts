export type GitChangeTreeNode =
  | { kind: "folder"; name: string; path: string; children: GitChangeTreeNode[] }
  | { kind: "file"; name: string; path: string; entry: ZoraiWorkspaceGitStatus };

type MutableFolder = { folders: Map<string, MutableFolder>; files: ZoraiWorkspaceGitStatus[] };

export function buildGitChangeTree(entries: readonly ZoraiWorkspaceGitStatus[]): GitChangeTreeNode[] {
  const root: MutableFolder = { folders: new Map(), files: [] };
  for (const entry of entries) {
    const parts = entry.path.split(/[\\/]/).filter(Boolean);
    let folder = root;
    for (const part of parts.slice(0, -1)) {
      let next = folder.folders.get(part);
      if (!next) {
        next = { folders: new Map(), files: [] };
        folder.folders.set(part, next);
      }
      folder = next;
    }
    folder.files.push(entry);
  }
  return toNodes(root, "");
}

function toNodes(folder: MutableFolder, prefix: string): GitChangeTreeNode[] {
  const folders = [...folder.folders.entries()]
    .sort(([left], [right]) => left.localeCompare(right))
    .map(([name, child]) => {
      let label = name;
      let path = prefix ? `${prefix}/${name}` : name;
      let current = child;
      while (current.files.length === 0 && current.folders.size === 1) {
        const [nextName, nextFolder] = [...current.folders.entries()][0];
        label = `${label}/${nextName}`;
        path = `${path}/${nextName}`;
        current = nextFolder;
      }
      return { kind: "folder" as const, name: label, path, children: toNodes(current, path) };
    });
  const files = [...folder.files]
    .sort((left, right) => left.path.localeCompare(right.path))
    .map((entry) => ({ kind: "file" as const, name: entry.path.split(/[\\/]/).pop() ?? entry.path, path: entry.path, entry }));
  return [...folders, ...files];
}
