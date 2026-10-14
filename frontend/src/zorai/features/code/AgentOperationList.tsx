import { workContextRelativePath } from "@/zorai/features/threads/compactSessionModel";

export type OperationSnapshotSummary = {
  available: boolean;
  revertible: boolean;
  retained_bytes: number;
  reason: string | null;
};

export function operationFileLabel(path: string, root: string | null | undefined): { name: string; folder: string } {
  const relative = workContextRelativePath(path, root);
  const parts = relative.split(/[\\/]/).filter(Boolean);
  const name = parts.pop() ?? relative;
  const folder = parts.length > 3 ? `…/${parts.slice(-2).join("/")}` : parts.join("/");
  return { name, folder: relative.startsWith("/") && parts.length <= 3 ? `/${folder}` : folder };
}

export function formatOperationAge(updatedAt: number, now: number): string {
  const seconds = Math.max(0, Math.round((now - updatedAt) / 1000));
  if (seconds < 60) return "just now";
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  return `${Math.round(hours / 24)}d ago`;
}

export function AgentOperationList({ groups, snapshots, root, now, onShowDiff, onRevert }: {
  groups: Array<[string, ZoraiWorkContextEntry[]]>;
  snapshots: Record<string, OperationSnapshotSummary | undefined>;
  root: string | null | undefined;
  now: number;
  onShowDiff: (operationId: string, entry: ZoraiWorkContextEntry) => void;
  onRevert: (operationId: string) => void;
}) {
  return (
    <div className="zorai-agent-operations">
      {groups.map(([operationId, entries]) => {
        const snapshot = snapshots[operationId];
        const latest = Math.max(...entries.map((entry) => entry.updated_at));
        return (
          <article key={operationId} className="zorai-agent-operation" title={operationId}>
            <header>
              <span className="zorai-agent-operation__tool">{entries[0]?.source ?? "operation"}</span>
              <span className="zorai-agent-operation__meta">{entries.length} file{entries.length === 1 ? "" : "s"} · {formatOperationAge(latest, now)}</span>
            </header>
            {entries.map((entry) => {
              const { name, folder } = operationFileLabel(entry.path, root);
              const created = !entry.before_hash;
              return (
                <button
                  type="button"
                  key={`${operationId}:${entry.path}`}
                  className="zorai-agent-operation__file"
                  title={`${entry.path}\nShow what this operation changed`}
                  onClick={() => onShowDiff(operationId, entry)}
                >
                  <span className={`zorai-agent-operation__mark${created ? " is-created" : ""}`} aria-label={created ? "Created" : "Modified"}>{created ? "A" : "M"}</span>
                  <strong>{name}</strong>
                  {folder ? <small>{folder}</small> : null}
                </button>
              );
            })}
            <footer>
              <span>{snapshot?.available ? `${Math.max(1, Math.round(snapshot.retained_bytes / 1024))} KiB retained` : "No snapshot"}</span>
              <button
                type="button"
                disabled={!snapshot?.revertible}
                title={snapshot?.revertible ? "Restore these files to their pre-operation content" : snapshot?.reason ?? "This operation cannot be reverted."}
                onClick={() => onRevert(operationId)}
              >
                Revert
              </button>
            </footer>
            {snapshot && !snapshot.revertible && snapshot.reason ? <p className="zorai-agent-operation__reason">{snapshot.reason}</p> : null}
          </article>
        );
      })}
    </div>
  );
}
