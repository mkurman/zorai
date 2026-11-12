import { useState, type CSSProperties } from "react";
import { CodeFileIcon, CodeFolderChevron } from "./CodeFileIcon";
import { buildGitChangeTree, type GitChangeTreeNode } from "./gitChangeTree";

export type WorkspaceGitAction = "stage" | "unstage" | "discard";

export function WorkspaceGitChangeRow({ entry, staged, onOpen, onReview, onAction, showParent = true, style }: {
  entry: ZoraiWorkspaceGitStatus;
  staged: boolean;
  showParent?: boolean;
  style?: CSSProperties;
  onOpen: (path: string) => Promise<void>;
  onReview: (path: string, staged: boolean) => Promise<void>;
  onAction: (action: WorkspaceGitAction, path: string) => Promise<void>;
}) {
  const status = staged ? entry.indexStatus : entry.worktreeStatus || entry.indexStatus;
  const parent = entry.path.split(/[\\/]/).slice(0, -1).join("/");
  return <div className="zorai-workspace-change-row" style={style}>
    <button type="button" className="zorai-workspace-change-path" title={entry.path} onClick={() => void onOpen(entry.path)}><CodeFileIcon path={entry.path} /><span><strong>{entry.path.split(/[\\/]/).pop()}</strong>{parent && showParent ? <small>{parent}</small> : null}</span><em>{status.trim() || "?"}</em></button>
    <span className="zorai-workspace-change-actions">
      <button type="button" title="Review hunks" aria-label={`Review hunks for ${entry.path}`} onClick={() => void onReview(entry.path, staged)}><ChangeActionIcon d="M3 4h4M3 8h10M3 12h7M10.5 2.5v3M9 4h3" /></button>
      <button type="button" title={staged ? "Unstage" : "Stage"} aria-label={`${staged ? "Unstage" : "Stage"} ${entry.path}`} onClick={() => void onAction(staged ? "unstage" : "stage", entry.path)}><ChangeActionIcon d={staged ? "M3.5 8h9" : "M8 3.5v9M3.5 8h9"} /></button>
      {!staged && entry.worktreeStatus.trim() && entry.worktreeStatus !== "?" ? <button type="button" title="Discard changes" aria-label={`Discard changes in ${entry.path}`} onClick={() => void onAction("discard", entry.path)}><ChangeActionIcon d="M5.5 3.5 3 6l2.5 2.5M3 6h6.5a3.5 3.5 0 0 1 0 7H7" /></button> : null}
    </span>
  </div>;
}

function ChangeActionIcon({ d }: { d: string }) {
  return <svg className="zorai-workspace-change-icon" viewBox="0 0 16 16" aria-hidden="true" focusable="false"><path d={d} /></svg>;
}

export type WorkspaceSourceControlPresentation = "auto" | "normal" | "narrow";
export type WorkspaceChangesView = "list" | "tree";

const CHANGES_VIEW_STORAGE_KEY = "zorai.code.changesView";

function readChangesView(): WorkspaceChangesView {
  if (typeof localStorage === "undefined") return "list";
  return localStorage.getItem(CHANGES_VIEW_STORAGE_KEY) === "tree" ? "tree" : "list";
}

type RowHandlers = {
  onOpen: (path: string) => Promise<void>;
  onReview: (path: string, staged: boolean) => Promise<void>;
  onAction: (action: WorkspaceGitAction, path: string) => Promise<void>;
};

export function WorkspaceSourceControlChanges({ status, onOpen, onReview, onAction, onBulkAction, presentation = "auto" }: {
  status: ZoraiWorkspaceGitStatus[];
  onOpen: (path: string) => Promise<void>;
  onReview: (path: string, staged: boolean) => Promise<void>;
  onAction: (action: WorkspaceGitAction, path: string) => Promise<void>;
  onBulkAction: (action: "stage" | "unstage", entries: ZoraiWorkspaceGitStatus[]) => Promise<void>;
  presentation?: WorkspaceSourceControlPresentation;
}) {
  const [view, setView] = useState<WorkspaceChangesView>(readChangesView);
  const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(() => new Set());
  const staged = status.filter((entry) => entry.indexStatus.trim() && entry.indexStatus !== "?");
  const unstaged = status.filter((entry) => entry.worktreeStatus.trim() || entry.indexStatus === "?");
  if (staged.length === 0 && unstaged.length === 0) return null;
  const handlers = { onOpen, onReview, onAction };
  const toggleView = () => {
    const next = view === "list" ? "tree" : "list";
    setView(next);
    if (typeof localStorage !== "undefined") localStorage.setItem(CHANGES_VIEW_STORAGE_KEY, next);
  };
  const toggleFolder = (key: string) => setCollapsed((current) => {
    const next = new Set(current);
    if (next.has(key)) next.delete(key);
    else next.add(key);
    return next;
  });
  const viewButton = (
    <button type="button" className="zorai-workspace-changes-view" title={view === "list" ? "View as tree" : "View as list"} aria-label={view === "list" ? "View as tree" : "View as list"} onClick={toggleView}>
      <svg viewBox="0 0 16 16" aria-hidden="true">
        {view === "list"
          ? <path d="M2.5 3.5h3M5 3.5v8M5 7.5h3.5M5 11.5h3.5M10.5 7.5h3M10.5 11.5h3M7.5 3.5h6" />
          : <path d="M2.5 4h11M2.5 8h11M2.5 12h11" />}
      </svg>
    </button>
  );
  const renderEntries = (entries: ZoraiWorkspaceGitStatus[], isStaged: boolean) => view === "tree"
    ? <GitChangeTree nodes={buildGitChangeTree(entries.slice(0, 500))} staged={isStaged} depth={0} collapsed={collapsed} onToggle={toggleFolder} handlers={handlers} />
    : entries.slice(0, 500).map((entry) => <WorkspaceGitChangeRow key={`${isStaged ? "staged" : "change"}:${entry.path}`} entry={entry} staged={isStaged} {...handlers} />);
  return <div className={`zorai-workspace-source-control is-${presentation}`} data-presentation={presentation}>
    {staged.length > 0 ? <details open><summary><span>Staged Changes</span><span className="zorai-workspace-changes-actions" onClick={(event) => event.preventDefault()}>{viewButton}<button type="button" aria-label="Unstage all changes" onClick={() => void onBulkAction("unstage", staged)}>− All</button></span></summary>{renderEntries(staged, true)}</details> : null}
    {unstaged.length > 0 ? <details open><summary><span>Changes</span><span className="zorai-workspace-changes-actions" onClick={(event) => event.preventDefault()}>{staged.length > 0 ? null : viewButton}<button type="button" aria-label="Stage all changes" onClick={() => void onBulkAction("stage", unstaged)}>＋ All</button></span></summary>{renderEntries(unstaged, false)}</details> : null}
  </div>;
}

function GitChangeTree({ nodes, staged, depth, collapsed, onToggle, handlers }: {
  nodes: GitChangeTreeNode[];
  staged: boolean;
  depth: number;
  collapsed: ReadonlySet<string>;
  onToggle: (key: string) => void;
  handlers: RowHandlers;
}) {
  return <>
    {nodes.map((node) => {
      if (node.kind === "file") {
        return <WorkspaceGitChangeRow key={`${staged ? "staged" : "change"}:${node.path}`} entry={node.entry} staged={staged} showParent={false} style={{ paddingLeft: 5 + depth * 12 }} {...handlers} />;
      }
      const key = `${staged ? "staged" : "change"}:${node.path}`;
      const open = !collapsed.has(key);
      return <div key={key} className="zorai-workspace-change-folder-group">
        <button type="button" className="zorai-workspace-change-folder" style={{ paddingLeft: 7 + depth * 12 }} aria-expanded={open} title={node.path} onClick={() => onToggle(key)}>
          <CodeFolderChevron expanded={open} />
          <span>{node.name}</span>
        </button>
        {open ? <GitChangeTree nodes={node.children} staged={staged} depth={depth + 1} collapsed={collapsed} onToggle={onToggle} handlers={handlers} /> : null}
      </div>;
    })}
  </>;
}
