import type { CodeToolbarMenuItem } from "./CodeToolbarMenu";

export type WorkspaceExplorerMenuActions = {
  isAttached: (path: string) => boolean;
  open: (path: string) => void;
  toggleAttached: (path: string) => void;
  create: (operation: "file" | "directory", initialPath: string) => void;
  copy: (text: string) => void;
  reveal: (absolutePath: string) => void;
  openExternal: (absolutePath: string) => void;
  openTerminal: (absoluteDirectory: string) => void;
  rename: (path: string) => void;
  remove: (path: string, isDirectory: boolean) => void;
  refresh: () => void;
};

export const EXPLORER_RENAME_KEY = "F2";
export const EXPLORER_DELETE_KEY = "Delete";

export function workspaceExplorerMenuItems(root: string, entry: ZoraiWorkspaceEntry | null, actions: WorkspaceExplorerMenuActions): CodeToolbarMenuItem[] {
  const absolute = (path: string) => path ? `${root.replace(/[\\/]$/, "")}/${path}` : root;
  const path = entry?.path ?? "";
  const isFile = Boolean(entry && !entry.isDirectory);
  const directory = isFile ? path.split(/[\\/]/).slice(0, -1).join("/") : path;
  const items: CodeToolbarMenuItem[] = [];
  if (isFile) {
    items.push({ key: "open", label: "Open", onSelect: () => actions.open(path) });
  } else {
    const prefix = directory ? `${directory}/` : "";
    items.push(
      { key: "new-file", label: "New File…", onSelect: () => actions.create("file", prefix) },
      { key: "new-folder", label: "New Folder…", onSelect: () => actions.create("directory", prefix) },
    );
  }
  items.push({ key: "reveal", label: "Open Containing Folder", separatorBefore: !isFile, onSelect: () => actions.reveal(absolute(path)) });
  if (isFile) items.push({ key: "open-external", label: "Open With Default App", onSelect: () => actions.openExternal(absolute(path)) });
  items.push({ key: "terminal", label: "Open in Integrated Terminal", onSelect: () => actions.openTerminal(absolute(directory)) });
  if (isFile) {
    items.push({ key: "attach", label: actions.isAttached(path) ? "Remove File from Agent Context" : "Add File to Agent Context", separatorBefore: true, onSelect: () => actions.toggleAttached(path) });
  }
  items.push(
    { key: "copy-path", label: "Copy Path", separatorBefore: true, onSelect: () => actions.copy(absolute(path)) },
    { key: "copy-relative", label: "Copy Relative Path", disabled: !entry, onSelect: () => actions.copy(path) },
  );
  if (entry) {
    items.push(
      { key: "rename", label: "Rename…", shortcut: EXPLORER_RENAME_KEY, separatorBefore: true, onSelect: () => actions.rename(path) },
      { key: "delete", label: "Delete", shortcut: EXPLORER_DELETE_KEY, danger: true, onSelect: () => actions.remove(path, entry.isDirectory) },
    );
  } else {
    items.push({ key: "refresh", label: "Refresh Explorer", separatorBefore: true, onSelect: actions.refresh });
  }
  return items;
}
