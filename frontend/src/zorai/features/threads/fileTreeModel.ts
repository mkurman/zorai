export type FileTreeNode<T> =
  | { kind: "folder"; key: string; name: string; children: FileTreeNode<T>[] }
  | { kind: "file"; key: string; name: string; item: T };

type MutableFolder<T> = { folders: Map<string, MutableFolder<T>>; files: Array<{ name: string; key: string; item: T }> };

export function buildFileTree<T>(items: readonly T[], pathOf: (item: T) => string, keyOf: (item: T) => string): FileTreeNode<T>[] {
  const root: MutableFolder<T> = { folders: new Map(), files: [] };
  for (const item of items) {
    const path = pathOf(item).replace(/\\/g, "/");
    const segments = path.split("/").filter(Boolean);
    const name = segments.pop() ?? path;
    if (path.startsWith("/") && segments.length > 0) segments[0] = `/${segments[0]}`;
    let folder = root;
    for (const segment of segments) {
      let next = folder.folders.get(segment);
      if (!next) {
        next = { folders: new Map(), files: [] };
        folder.folders.set(segment, next);
      }
      folder = next;
    }
    folder.files.push({ name, key: keyOf(item), item });
  }
  return toNodes(root, "");
}

function toNodes<T>(folder: MutableFolder<T>, parentKey: string): FileTreeNode<T>[] {
  const folders = [...folder.folders.entries()]
    .sort(([left], [right]) => left.localeCompare(right))
    .map(([name, child]): FileTreeNode<T> => {
      let label = name;
      let current = child;
      while (current.files.length === 0 && current.folders.size === 1) {
        const [nextName, next] = [...current.folders.entries()][0];
        label = `${label}/${nextName}`;
        current = next;
      }
      const key = `${parentKey}/${label}`;
      return { kind: "folder", key, name: label, children: toNodes(current, key) };
    });
  const files = [...folder.files]
    .sort((left, right) => left.name.localeCompare(right.name))
    .map((file): FileTreeNode<T> => ({ kind: "file", key: file.key, name: file.name, item: file.item }));
  return [...folders, ...files];
}
