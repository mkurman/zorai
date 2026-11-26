import { useMemo, useState, type ReactNode } from "react";
import type { AgentTodoItem } from "@/lib/agentStore";
import type { WorkContextEntry } from "@/lib/agentWorkContext";
import { shortenHomePath } from "@/lib/workspaceStore";
import { CodeFileIcon, CodeFolderChevron } from "@/zorai/features/code/CodeFileIcon";
import { workContextFileName, workContextRelativePath } from "./compactSessionModel";
import { buildFileTree, type FileTreeNode } from "./fileTreeModel";
import { useFileViewPrefsStore } from "./fileViewPrefsStore";

export type WorkFileRowExtras = { className?: string; stats?: ReactNode; actions?: ReactNode };

export function WorkFileList({ entries, workspaceRoot, onOpen, rowExtras }: {
  entries: WorkContextEntry[];
  workspaceRoot: string | null;
  onOpen: (entry: WorkContextEntry) => void;
  rowExtras?: (entry: WorkContextEntry) => WorkFileRowExtras;
}) {
  const fileViewMode = useFileViewPrefsStore((state) => state.mode);
  const setFileViewMode = useFileViewPrefsStore((state) => state.setMode);
  const [collapsedFolders, setCollapsedFolders] = useState<Set<string>>(() => new Set());
  const fileTree = useMemo(
    () => buildFileTree(
      entries,
      (entry) => {
        const root = entry.repoRoot ?? workspaceRoot;
        const relative = root ? workContextRelativePath(entry.path, root) : entry.path;
        return relative === entry.path ? shortenHomePath(entry.path) : relative;
      },
      (entry) => `${entry.path}:${entry.updatedAt}`,
    ),
    [entries, workspaceRoot],
  );

  const entryFolder = (entry: WorkContextEntry) => {
    const root = entry.repoRoot ?? workspaceRoot;
    const relative = root ? workContextRelativePath(entry.path, root) : entry.path;
    return relative.split(/[\\/]/).slice(0, -1).join("/");
  };

  const fileRow = (entry: WorkContextEntry, showFolder: boolean, depth: number) => {
    const extras = rowExtras?.(entry) ?? {};
    const folder = showFolder ? entryFolder(entry) : "";
    return (
      <li key={`${entry.path}:${entry.updatedAt}`} className={["zorai-compact-session__file", extras.className ?? ""].filter(Boolean).join(" ")} style={depth ? { paddingLeft: 6 + depth * 14 } : undefined}>
        <button type="button" className="zorai-compact-session__name" title={entry.path} onClick={() => onOpen(entry)}>
          <CodeFileIcon path={entry.path} />
          <span className="zorai-compact-session__filename">{workContextFileName(entry.path)}</span>
          {folder ? <span className="zorai-compact-session__folder">{folder}</span> : null}
        </button>
        <span className="zorai-compact-session__stats">
          {extras.stats ?? <span>{entry.changeKind ?? "changed"}</span>}
        </span>
        {extras.actions ?? null}
      </li>
    );
  };

  const renderFileTree = (nodes: FileTreeNode<WorkContextEntry>[], depth: number): ReactNode[] => nodes.flatMap((node) => {
    if (node.kind === "file") return [fileRow(node.item, false, depth)];
    const collapsed = collapsedFolders.has(node.key);
    return [
      <li key={node.key} className="zorai-compact-session__tree-folder" style={depth ? { paddingLeft: 6 + depth * 14 } : undefined}>
        <button
          type="button"
          aria-expanded={!collapsed}
          title={node.name}
          onClick={() => setCollapsedFolders((current) => {
            const next = new Set(current);
            if (next.has(node.key)) next.delete(node.key);
            else next.add(node.key);
            return next;
          })}
        >
          <CodeFolderChevron expanded={!collapsed} />
          <span>{node.name}</span>
        </button>
      </li>,
      ...(collapsed ? [] : renderFileTree(node.children, depth + 1)),
    ];
  });

  return (
    <>
      <div className="zorai-compact-session__view-toggle" role="group" aria-label="File view">
        {(["list", "tree"] as const).map((mode) => (
          <button key={mode} type="button" aria-pressed={fileViewMode === mode} className={fileViewMode === mode ? "is-active" : undefined} onClick={() => setFileViewMode(mode)}>
            {mode === "list" ? "List" : "Tree"}
          </button>
        ))}
      </div>
      <ul className="zorai-compact-session__list">
        {fileViewMode === "tree" ? renderFileTree(fileTree, 0) : entries.map((entry) => fileRow(entry, true, 0))}
      </ul>
    </>
  );
}

export function WorkTodoList({ todos }: { todos: AgentTodoItem[] }) {
  return (
    <ul className="zorai-compact-session__list">
      {todos.map((todo) => (
        <li key={todo.id} className={`zorai-compact-session__todo zorai-compact-session__todo--${todo.status}`}>
          <span className={`zorai-compact-session__check zorai-compact-session__check--${todo.status}`} aria-hidden="true">
            {todo.status === "completed" ? <svg viewBox="0 0 16 16"><path d="m4 8.5 2.5 2.5L12 5.5" /></svg> : null}
          </span>
          <span className="zorai-compact-session__todo-text">{todo.content}</span>
          <span className="zorai-sr-only">{todo.status === "completed" ? "done" : todo.status === "in_progress" ? "in progress" : "to do"}</span>
        </li>
      ))}
    </ul>
  );
}
