import { useEffect, useState } from "react";
import { getBridge } from "@/lib/bridge";
import { WorkspaceExplorerTree } from "./WorkspaceExplorerTree";

function ExtraRoot({ root, onOpen, onRemove }: { root: string; onOpen: (absolutePath: string) => void; onRemove: () => void }) {
  const [entries, setEntries] = useState<ZoraiWorkspaceEntry[]>([]);
  const [status, setStatus] = useState<Map<string, string>>(() => new Map());
  const [error, setError] = useState<string | null>(null);
  const [refresh, setRefresh] = useState(0);
  useEffect(() => {
    const bridge = getBridge();
    let cancelled = false;
    void bridge?.workspaceListDirectory?.(root, "").then((next) => { if (!cancelled) setEntries(next); }).catch((reason) => { if (!cancelled) setError(reason instanceof Error ? reason.message : String(reason)); });
    void bridge?.workspaceGitStatus?.(root).then((next) => {
      if (!cancelled) setStatus(new Map(next.map((entry) => [entry.path, entry.indexStatus === "?" ? "U" : (entry.worktreeStatus || entry.indexStatus || "M").trim() || "M"])));
    }).catch(() => undefined);
    return () => { cancelled = true; };
  }, [root, refresh]);
  const name = root.split(/[\\/]/).filter(Boolean).pop() ?? root;
  return (
    <details className="zorai-code-files" open>
      <summary className="zorai-code-files-heading" title={root}>
        <span>{name}</span>
        <span className="zorai-code-explorer-actions">
          <button type="button" title="Refresh folder" aria-label={`Refresh ${name}`} onClick={(event) => { event.preventDefault(); setRefresh((value) => value + 1); }}>⟳</button>
          <button type="button" title="Remove folder from workspace" aria-label={`Remove ${name} from workspace`} onClick={(event) => { event.preventDefault(); onRemove(); }}>×</button>
        </span>
      </summary>
      {error ? <p className="zorai-workspace-empty">{error}</p> : <WorkspaceExplorerTree root={root} entries={entries} status={status} onOpen={(path) => onOpen(`${root.replace(/[\\/]+$/, "")}/${path}`)} refreshToken={refresh} />}
    </details>
  );
}

export function CodeWorkspaceFolders({ roots, onOpen, onChange }: { roots: string[]; onOpen: (absolutePath: string) => void; onChange: (roots: string[]) => void }) {
  return (
    <>
      {roots.map((root) => <ExtraRoot key={root} root={root} onOpen={onOpen} onRemove={() => onChange(roots.filter((entry) => entry !== root))} />)}
      <button
        type="button"
        className="zorai-code-add-folder"
        onClick={() => void getBridge()?.workspaceSelectFolder?.().then((result) => { if (!result.canceled && result.root) onChange([...roots, result.root.root]); })}
      >
        + Add folder to workspace
      </button>
    </>
  );
}
