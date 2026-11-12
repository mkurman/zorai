import { useEffect, useMemo, useState, useSyncExternalStore, type KeyboardEvent, type MouseEvent } from "react";
import { getBridge } from "@/lib/bridge";
import { CodeFileIcon, CodeFolderChevron } from "./CodeFileIcon";
import { workspaceExplorerControllerFor, type WorkspaceExplorerSnapshot } from "./workspaceExplorerController";
import { filterExplorerEntries, searchWorkspaceFiles } from "./workspaceExplorerFilter";
import { EXPLORER_DELETE_KEY, EXPLORER_RENAME_KEY } from "./workspaceExplorerMenu";

export type WorkspaceExplorerTreeProps = {
  root: string;
  entries: ZoraiWorkspaceEntry[];
  status: Map<string, string>;
  onOpen: (path: string) => void;
  refreshToken: number;
  allFiles?: string[] | null;
  onEntryContextMenu?: ExplorerContextMenuHandler;
  onEntryShortcut?: ExplorerShortcutHandler;
};

export type ExplorerShortcutHandler = (entry: ZoraiWorkspaceEntry, action: "rename" | "delete") => void;

function explorerShortcut(handler: ExplorerShortcutHandler | undefined, entry: ZoraiWorkspaceEntry) {
  if (!handler) return undefined;
  return (event: KeyboardEvent) => {
    const action = event.key === EXPLORER_RENAME_KEY ? "rename" : event.key === EXPLORER_DELETE_KEY ? "delete" : null;
    if (!action || event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return;
    event.preventDefault();
    handler(entry, action);
  };
}

export type ExplorerContextMenuHandler = (entry: ZoraiWorkspaceEntry | null, x: number, y: number) => void;

function explorerContextMenu(handler: ExplorerContextMenuHandler | undefined, entry: ZoraiWorkspaceEntry | null) {
  if (!handler) return undefined;
  return (event: MouseEvent) => {
    event.preventDefault();
    event.stopPropagation();
    handler(entry, event.clientX, event.clientY);
  };
}

function WorkspaceExplorerTreeNode({ entry, depth, status, onOpen, snapshot, onToggle, query = "", onEntryContextMenu, onEntryShortcut }: {
  entry: ZoraiWorkspaceEntry;
  depth: number;
  status: Map<string, string>;
  onOpen: (path: string) => void;
  snapshot: WorkspaceExplorerSnapshot;
  onToggle: (path: string) => Promise<void>;
  query?: string;
  onEntryContextMenu?: ExplorerContextMenuHandler;
  onEntryShortcut?: ExplorerShortcutHandler;
}) {
  const expanded = snapshot.expandedPaths.has(entry.path);
  const loading = snapshot.loadingPaths.has(entry.path);
  const children = snapshot.childrenByPath.get(entry.path) ?? [];
  const marker = status.get(entry.path) ?? "";
  const error = snapshot.errorByPath.get(entry.path);
  const activate = () => entry.isDirectory ? onToggle(entry.path) : Promise.resolve(onOpen(entry.path));

  return <div>
    <button type="button" role="treeitem" aria-expanded={entry.isDirectory ? expanded : undefined} aria-disabled={loading || undefined} className={entry.isGitIgnored ? "zorai-workspace-tree-row zorai-workspace-tree-row--ignored" : "zorai-workspace-tree-row"} style={{ paddingLeft: 8 + depth * 14 }} onClick={() => void activate()} onContextMenu={explorerContextMenu(onEntryContextMenu, entry)} onKeyDown={explorerShortcut(onEntryShortcut, entry)}>
      {entry.isDirectory ? <span className="zorai-workspace-chevron"><CodeFolderChevron expanded={expanded} /></span> : null}
      {!entry.isDirectory ? <CodeFileIcon path={entry.path} /> : null}
      <span className="zorai-workspace-tree-name">{entry.name}</span>
      {marker ? <span className="zorai-workspace-git-marker">{marker}</span> : null}
    </button>
    {loading ? <div className="zorai-workspace-tree-loading" style={{ paddingLeft: 24 + depth * 14 }}>Loading…</div> : null}
    {error ? <div className="zorai-workspace-tree-error" role="alert" style={{ paddingLeft: 24 + depth * 14 }}>{error}</div> : null}
    {expanded ? filterExplorerEntries(children, query, (path) => snapshot.childrenByPath.get(path) ?? []).map((child) => <WorkspaceExplorerTreeNode key={child.path} entry={child} depth={depth + 1} status={status} onOpen={onOpen} snapshot={snapshot} onToggle={onToggle} query={query} onEntryContextMenu={onEntryContextMenu} onEntryShortcut={onEntryShortcut} />) : null}
  </div>;
}

export function WorkspaceExplorerTreeView({ entries, status, onOpen, snapshot, onToggle, query = "", onEntryContextMenu, onEntryShortcut }: {
  entries: ZoraiWorkspaceEntry[];
  status: Map<string, string>;
  onOpen: (path: string) => void;
  snapshot: WorkspaceExplorerSnapshot;
  onToggle: (path: string) => Promise<void>;
  query?: string;
  onEntryContextMenu?: ExplorerContextMenuHandler;
  onEntryShortcut?: ExplorerShortcutHandler;
}) {
  return <div className="zorai-workspace-tree" role="tree" aria-label="Workspace files">{entries.map((entry) => <WorkspaceExplorerTreeNode key={entry.path} entry={entry} depth={0} status={status} onOpen={onOpen} snapshot={snapshot} onToggle={onToggle} query={query} onEntryContextMenu={onEntryContextMenu} onEntryShortcut={onEntryShortcut} />)}</div>;
}

const EMPTY_EXPLORER_SNAPSHOT: WorkspaceExplorerSnapshot = {
  expandedPaths: new Set(),
  childrenByPath: new Map(),
  loadingPaths: new Set(),
  errorByPath: new Map(),
};

export function WorkspaceExplorerTree({ root, entries, status, onOpen, refreshToken, allFiles, onEntryContextMenu, onEntryShortcut }: WorkspaceExplorerTreeProps) {
  const bridge = getBridge();
  const [query, setQuery] = useState("");
  const controller = useMemo(() => workspaceExplorerControllerFor(bridge, root), [bridge, root]);
  const snapshot = useSyncExternalStore(controller.subscribe, controller.getSnapshot, () => EMPTY_EXPLORER_SNAPSHOT);
  const visibleEntries = useMemo(
    () => filterExplorerEntries(entries, query, (path) => snapshot.childrenByPath.get(path) ?? []),
    [entries, query, snapshot],
  );
  const fileMatches = useMemo(() => allFiles && query.trim() ? searchWorkspaceFiles(allFiles, query) : null, [allFiles, query]);

  useEffect(() => {
    if (refreshToken > 0) void controller.refreshExpanded();
  }, [controller, refreshToken]);

  return (
    <div className="zorai-workspace-files" onContextMenu={explorerContextMenu(onEntryContextMenu, null)}>
      <input
        className="zorai-search-input zorai-workspace-files__search"
        value={query}
        onChange={(event) => setQuery(event.target.value)}
        placeholder="Filter files"
        aria-label="Filter workspace files"
      />
      {fileMatches ? (
        <div className="zorai-workspace-tree" role="list" aria-label="Matching files">
          {fileMatches.length === 0 ? <div className="zorai-workspace-empty">No files match “{query.trim()}”.</div> : fileMatches.map((path) => {
            const parts = path.split(/[\\/]/);
            const name = parts.pop() ?? path;
            const marker = status.get(path) ?? "";
            return (
              <button type="button" role="listitem" key={path} className="zorai-workspace-tree-row" style={{ paddingLeft: 8 }} title={path} onClick={() => onOpen(path)} onContextMenu={explorerContextMenu(onEntryContextMenu, { name, path, isDirectory: false, isSymbolicLink: false, sizeBytes: null, modifiedAt: null })} onKeyDown={explorerShortcut(onEntryShortcut, { name, path, isDirectory: false, isSymbolicLink: false, sizeBytes: null, modifiedAt: null })}>
                <CodeFileIcon path={path} />
                <span className="zorai-workspace-tree-name">{name}</span>
                {parts.length > 0 ? <span className="zorai-workspace-tree-dir">{parts.join("/")}</span> : null}
                {marker ? <span className="zorai-workspace-git-marker">{marker}</span> : null}
              </button>
            );
          })}
        </div>
      ) : (
        <WorkspaceExplorerTreeView entries={visibleEntries} status={status} onOpen={onOpen} snapshot={snapshot} onToggle={controller.toggle} query={query} onEntryContextMenu={onEntryContextMenu} onEntryShortcut={onEntryShortcut} />
      )}
    </div>
  );
}
