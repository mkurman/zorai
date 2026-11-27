import { useEffect, useMemo, useRef, useState, useSyncExternalStore, type KeyboardEvent, type MouseEvent } from "react";
import { getBridge } from "@/lib/bridge";
import { CodeFileIcon, CodeFolderChevron } from "./CodeFileIcon";
import { workspaceExplorerControllerFor, type WorkspaceExplorerSnapshot } from "./workspaceExplorerController";
import { filterExplorerEntries, searchWorkspaceFiles } from "./workspaceExplorerFilter";
import { EXPLORER_DELETE_KEY, EXPLORER_RENAME_KEY } from "./workspaceExplorerMenu";
import { startCodeFileDrag } from "./codeSplitStore";

export type WorkspaceExplorerTreeProps = {
  root: string;
  entries: ZoraiWorkspaceEntry[];
  status: Map<string, string>;
  onOpen: (path: string) => void;
  refreshToken: number;
  allFiles?: string[] | null;
  onEntryContextMenu?: ExplorerContextMenuHandler;
  onEntryShortcut?: ExplorerShortcutHandler;
  createRequest?: ExplorerCreateRequest | null;
  onCreate?: (kind: ExplorerCreateKind, path: string) => Promise<boolean>;
  onCreateCancel?: () => void;
};

export type ExplorerCreateKind = "file" | "directory";
export type ExplorerCreateRequest = { kind: ExplorerCreateKind; parent?: string; token: number };
type ExplorerCreate = { kind: ExplorerCreateKind; parent: string; submit: (name: string) => Promise<boolean>; cancel: () => void };

export function explorerCreateParent(selected: { path: string; isDirectory: boolean } | null): string {
  if (!selected) return "";
  return selected.isDirectory ? selected.path : selected.path.split("/").slice(0, -1).join("/");
}

export function explorerChildPath(parent: string, name: string): string {
  const clean = name.trim().replace(/^\/+/, "");
  return parent ? `${parent}/${clean}` : clean;
}

function ExplorerCreateRow({ create, depth }: { create: ExplorerCreate; depth: number }) {
  const [name, setName] = useState("");
  const busy = useRef(false);
  const finish = async (value: string) => {
    if (busy.current) return;
    if (!value.trim()) { create.cancel(); return; }
    busy.current = true;
    await create.submit(value);
    busy.current = false;
  };
  return <div className="zorai-workspace-tree-row zorai-workspace-tree-create" style={{ paddingLeft: 8 + depth * 14 }}>
    {create.kind === "directory" ? <span className="zorai-workspace-chevron"><CodeFolderChevron expanded={false} /></span> : <CodeFileIcon path={name || "file"} />}
    <input autoFocus value={name} aria-label={create.kind === "directory" ? "New folder name" : "New file name"} spellCheck={false} onChange={(event) => setName(event.target.value)} onBlur={() => void finish(name)} onKeyDown={(event) => {
      if (event.key === "Enter") { event.preventDefault(); void finish(name); }
      if (event.key === "Escape") { event.preventDefault(); create.cancel(); }
    }} />
  </div>;
}

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

export function revealScrollDelta(row: { top: number; bottom: number }, container: { top: number; bottom: number }): number {
  if (row.top < container.top) return row.top - container.top;
  if (row.bottom > container.bottom) return row.bottom - container.bottom;
  return 0;
}

function WorkspaceExplorerTreeNode({ entry, depth, status, onOpen, snapshot, onToggle, query = "", onEntryContextMenu, onEntryShortcut, selectedPath, onSelect, create }: {
  entry: ZoraiWorkspaceEntry;
  depth: number;
  status: Map<string, string>;
  onOpen: (path: string) => void;
  snapshot: WorkspaceExplorerSnapshot;
  onToggle: (path: string) => Promise<void>;
  query?: string;
  onEntryContextMenu?: ExplorerContextMenuHandler;
  onEntryShortcut?: ExplorerShortcutHandler;
  selectedPath?: string | null;
  onSelect?: (entry: ZoraiWorkspaceEntry) => void;
  create?: ExplorerCreate | null;
}) {
  const expanded = snapshot.expandedPaths.has(entry.path);
  const loading = snapshot.loadingPaths.has(entry.path);
  const children = snapshot.childrenByPath.get(entry.path) ?? [];
  const marker = status.get(entry.path) ?? "";
  const error = snapshot.errorByPath.get(entry.path);
  const activate = () => entry.isDirectory ? onToggle(entry.path) : Promise.resolve(onOpen(entry.path));
  const revealed = snapshot.revealedPath === entry.path;
  const rowRef = useRef<HTMLButtonElement | null>(null);
  useEffect(() => {
    const row = rowRef.current;
    const tree = row?.closest<HTMLElement>(".zorai-workspace-tree");
    if (revealed && row && tree) tree.scrollTop += revealScrollDelta(row.getBoundingClientRect(), tree.getBoundingClientRect());
  }, [revealed, snapshot.revealToken]);

  return <div>
    <button ref={rowRef} type="button" role="treeitem" aria-expanded={entry.isDirectory ? expanded : undefined} aria-disabled={loading || undefined} aria-current={revealed || undefined} className={["zorai-workspace-tree-row", entry.isGitIgnored ? "zorai-workspace-tree-row--ignored" : "", revealed ? "is-revealed" : "", selectedPath === entry.path ? "is-selected" : ""].filter(Boolean).join(" ")} style={{ paddingLeft: 8 + depth * 14 }} draggable={!entry.isDirectory} onDragStart={entry.isDirectory ? undefined : (event) => startCodeFileDrag(event, entry.path)} onClick={() => { onSelect?.(entry); void activate(); }} onContextMenu={explorerContextMenu(onEntryContextMenu, entry)} onKeyDown={explorerShortcut(onEntryShortcut, entry)}>
      {entry.isDirectory ? <span className="zorai-workspace-chevron"><CodeFolderChevron expanded={expanded} /></span> : null}
      {!entry.isDirectory ? <CodeFileIcon path={entry.path} /> : null}
      <span className="zorai-workspace-tree-name">{entry.name}</span>
      {marker ? <span className="zorai-workspace-git-marker">{marker}</span> : null}
    </button>
    {loading ? <div className="zorai-workspace-tree-loading" style={{ paddingLeft: 24 + depth * 14 }}>Loading…</div> : null}
    {error ? <div className="zorai-workspace-tree-error" role="alert" style={{ paddingLeft: 24 + depth * 14 }}>{error}</div> : null}
    {expanded && create && create.parent === entry.path ? <ExplorerCreateRow create={create} depth={depth + 1} /> : null}
    {expanded ? filterExplorerEntries(children, query, (path) => snapshot.childrenByPath.get(path) ?? []).map((child) => <WorkspaceExplorerTreeNode key={child.path} entry={child} depth={depth + 1} status={status} onOpen={onOpen} snapshot={snapshot} onToggle={onToggle} query={query} onEntryContextMenu={onEntryContextMenu} onEntryShortcut={onEntryShortcut} selectedPath={selectedPath} onSelect={onSelect} create={create} />) : null}
  </div>;
}

export function WorkspaceExplorerTreeView({ entries, status, onOpen, snapshot, onToggle, query = "", onEntryContextMenu, onEntryShortcut, selectedPath, onSelect, create }: {
  entries: ZoraiWorkspaceEntry[];
  status: Map<string, string>;
  onOpen: (path: string) => void;
  snapshot: WorkspaceExplorerSnapshot;
  onToggle: (path: string) => Promise<void>;
  query?: string;
  onEntryContextMenu?: ExplorerContextMenuHandler;
  onEntryShortcut?: ExplorerShortcutHandler;
  selectedPath?: string | null;
  onSelect?: (entry: ZoraiWorkspaceEntry) => void;
  create?: ExplorerCreate | null;
}) {
  return <div className="zorai-workspace-tree" role="tree" aria-label="Workspace files">
    {create && create.parent === "" ? <ExplorerCreateRow create={create} depth={0} /> : null}
    {entries.map((entry) => <WorkspaceExplorerTreeNode key={entry.path} entry={entry} depth={0} status={status} onOpen={onOpen} snapshot={snapshot} onToggle={onToggle} query={query} onEntryContextMenu={onEntryContextMenu} onEntryShortcut={onEntryShortcut} selectedPath={selectedPath} onSelect={onSelect} create={create} />)}
  </div>;
}

const EMPTY_EXPLORER_SNAPSHOT: WorkspaceExplorerSnapshot = {
  expandedPaths: new Set(),
  childrenByPath: new Map(),
  loadingPaths: new Set(),
  errorByPath: new Map(),
  revealedPath: null,
  revealToken: 0,
};

export function WorkspaceExplorerTree({ root, entries, status, onOpen, refreshToken, allFiles, onEntryContextMenu, onEntryShortcut, createRequest, onCreate, onCreateCancel }: WorkspaceExplorerTreeProps) {
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

  const [selected, setSelected] = useState<{ path: string; isDirectory: boolean } | null>(null);
  useEffect(() => {
    if (snapshot.revealedPath) setSelected({ path: snapshot.revealedPath, isDirectory: false });
  }, [snapshot.revealedPath, snapshot.revealToken]);
  const [createParent, setCreateParent] = useState<string | null>(null);
  useEffect(() => {
    if (!createRequest) { setCreateParent(null); return; }
    const parent = createRequest.parent ?? explorerCreateParent(selected);
    setQuery("");
    setCreateParent(parent);
    if (parent) void controller.expand(parent);
  }, [createRequest?.token]);
  const create: ExplorerCreate | null = createRequest && createParent !== null && onCreate ? {
    kind: createRequest.kind,
    parent: createParent,
    submit: (name) => onCreate(createRequest.kind, explorerChildPath(createParent, name)),
    cancel: () => onCreateCancel?.(),
  } : null;

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
              <button type="button" role="listitem" key={path} className="zorai-workspace-tree-row" style={{ paddingLeft: 8 }} title={path} draggable onDragStart={(event) => startCodeFileDrag(event, path)} onClick={() => onOpen(path)} onContextMenu={explorerContextMenu(onEntryContextMenu, { name, path, isDirectory: false, isSymbolicLink: false, sizeBytes: null, modifiedAt: null })} onKeyDown={explorerShortcut(onEntryShortcut, { name, path, isDirectory: false, isSymbolicLink: false, sizeBytes: null, modifiedAt: null })}>
                <CodeFileIcon path={path} />
                <span className="zorai-workspace-tree-name">{name}</span>
                {parts.length > 0 ? <span className="zorai-workspace-tree-dir">{parts.join("/")}</span> : null}
                {marker ? <span className="zorai-workspace-git-marker">{marker}</span> : null}
              </button>
            );
          })}
        </div>
      ) : (
        <WorkspaceExplorerTreeView entries={visibleEntries} status={status} onOpen={onOpen} snapshot={snapshot} onToggle={controller.toggle} query={query} onEntryContextMenu={onEntryContextMenu} onEntryShortcut={onEntryShortcut} selectedPath={selected?.path ?? null} onSelect={(entry) => setSelected({ path: entry.path, isDirectory: entry.isDirectory })} create={create} />
      )}
    </div>
  );
}
