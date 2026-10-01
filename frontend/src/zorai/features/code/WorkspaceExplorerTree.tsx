import { useEffect, useMemo, useState, useSyncExternalStore } from "react";
import { getBridge } from "@/lib/bridge";
import { CodeFileIcon, CodeFolderChevron } from "./CodeFileIcon";
import { createWorkspaceExplorerController, createWorkspaceExplorerLoader, type WorkspaceExplorerSnapshot } from "./workspaceExplorerController";
import { filterExplorerEntries } from "./workspaceExplorerFilter";

export type WorkspaceExplorerTreeProps = {
  root: string;
  entries: ZoraiWorkspaceEntry[];
  status: Map<string, string>;
  onOpen: (path: string) => void;
  refreshToken: number;
};

function WorkspaceExplorerTreeNode({ entry, depth, status, onOpen, snapshot, onToggle, query = "" }: {
  entry: ZoraiWorkspaceEntry;
  depth: number;
  status: Map<string, string>;
  onOpen: (path: string) => void;
  snapshot: WorkspaceExplorerSnapshot;
  onToggle: (path: string) => Promise<void>;
  query?: string;
}) {
  const expanded = snapshot.expandedPaths.has(entry.path);
  const loading = snapshot.loadingPaths.has(entry.path);
  const children = snapshot.childrenByPath.get(entry.path) ?? [];
  const marker = status.get(entry.path) ?? "";
  const error = snapshot.errorByPath.get(entry.path);
  const activate = () => entry.isDirectory ? onToggle(entry.path) : Promise.resolve(onOpen(entry.path));

  return <div>
    <button type="button" role="treeitem" aria-expanded={entry.isDirectory ? expanded : undefined} aria-disabled={loading || undefined} className="zorai-workspace-tree-row" style={{ paddingLeft: 8 + depth * 14 }} onClick={() => void activate()}>
      {entry.isDirectory ? <span className="zorai-workspace-chevron"><CodeFolderChevron expanded={expanded} /></span> : null}
      {!entry.isDirectory ? <CodeFileIcon path={entry.path} /> : null}
      <span className="zorai-workspace-tree-name">{entry.name}</span>
      {marker ? <span className="zorai-workspace-git-marker">{marker}</span> : null}
    </button>
    {loading ? <div className="zorai-workspace-tree-loading" style={{ paddingLeft: 24 + depth * 14 }}>Loading…</div> : null}
    {error ? <div className="zorai-workspace-tree-error" role="alert" style={{ paddingLeft: 24 + depth * 14 }}>{error}</div> : null}
    {expanded ? filterExplorerEntries(children, query, (path) => snapshot.childrenByPath.get(path) ?? []).map((child) => <WorkspaceExplorerTreeNode key={child.path} entry={child} depth={depth + 1} status={status} onOpen={onOpen} snapshot={snapshot} onToggle={onToggle} query={query} />) : null}
  </div>;
}

export function WorkspaceExplorerTreeView({ entries, status, onOpen, snapshot, onToggle, query = "" }: {
  entries: ZoraiWorkspaceEntry[];
  status: Map<string, string>;
  onOpen: (path: string) => void;
  snapshot: WorkspaceExplorerSnapshot;
  onToggle: (path: string) => Promise<void>;
  query?: string;
}) {
  return <div className="zorai-workspace-tree" role="tree" aria-label="Workspace files">{entries.map((entry) => <WorkspaceExplorerTreeNode key={entry.path} entry={entry} depth={0} status={status} onOpen={onOpen} snapshot={snapshot} onToggle={onToggle} query={query} />)}</div>;
}

const EMPTY_EXPLORER_SNAPSHOT: WorkspaceExplorerSnapshot = {
  expandedPaths: new Set(),
  childrenByPath: new Map(),
  loadingPaths: new Set(),
  errorByPath: new Map(),
};

export function WorkspaceExplorerTree({ root, entries, status, onOpen, refreshToken }: WorkspaceExplorerTreeProps) {
  const bridge = getBridge();
  const [query, setQuery] = useState("");
  const controller = useMemo(() => createWorkspaceExplorerController(createWorkspaceExplorerLoader(bridge, root)), [bridge, root]);
  const snapshot = useSyncExternalStore(controller.subscribe, controller.getSnapshot, () => EMPTY_EXPLORER_SNAPSHOT);
  const visibleEntries = useMemo(
    () => filterExplorerEntries(entries, query, (path) => snapshot.childrenByPath.get(path) ?? []),
    [entries, query, snapshot],
  );

  useEffect(() => {
    if (refreshToken > 0) void controller.refreshExpanded();
  }, [controller, refreshToken]);

  return (
    <div className="zorai-workspace-files">
      <input
        className="zorai-search-input zorai-workspace-files__search"
        value={query}
        onChange={(event) => setQuery(event.target.value)}
        placeholder="Filter files"
        aria-label="Filter workspace files"
      />
      <WorkspaceExplorerTreeView entries={visibleEntries} status={status} onOpen={onOpen} snapshot={snapshot} onToggle={controller.toggle} query={query} />
    </div>
  );
}
