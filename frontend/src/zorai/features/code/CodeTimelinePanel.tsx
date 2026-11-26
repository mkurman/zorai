import { useEffect, useState } from "react";
import { getBridge } from "@/lib/bridge";
import { relativeTime } from "./codeGitBlame";
import { mergeTimeline, type TimelineEntry } from "./codeTimeline";

export function CodeTimelinePanel({ root, path, refreshToken, onCompare, onRestore }: {
  root: string;
  path: string | null;
  refreshToken: unknown;
  onCompare: (content: string) => void;
  onRestore: (content: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const [entries, setEntries] = useState<TimelineEntry[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const bridge = getBridge();
    if (!open || !path) {
      setEntries([]);
      return;
    }
    let cancelled = false;
    void Promise.all([
      bridge?.workspaceLocalHistoryList?.(root, path).catch(() => []) ?? Promise.resolve([]),
      bridge?.workspaceGitHistory?.(root, { path, limit: 50 }).catch(() => []) ?? Promise.resolve([]),
    ]).then(([local, commits]) => {
      if (!cancelled) setEntries(mergeTimeline(local, commits));
    });
    return () => { cancelled = true; };
  }, [open, path, refreshToken, root]);

  const contentFor = async (entry: TimelineEntry) => {
    const bridge = getBridge();
    if (!path) return null;
    try {
      return entry.kind === "local"
        ? await bridge?.workspaceLocalHistoryRead?.(root, path, entry.id) ?? null
        : await bridge?.workspaceGitShow?.(root, path, entry.id) ?? null;
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
      return null;
    }
  };

  return (
    <details className="zorai-code-timeline" open={open} onToggle={(event) => setOpen((event.currentTarget as HTMLDetailsElement).open)}>
      <summary>Timeline{path ? ` · ${path.split("/").pop()}` : ""}</summary>
      {!path ? <p className="zorai-code-timeline__empty">Open a file to see its history.</p> : null}
      {path && open && entries.length === 0 ? <p className="zorai-code-timeline__empty">No saves or commits recorded for this file yet.</p> : null}
      {error ? <p className="zorai-code-timeline__empty is-error">{error}</p> : null}
      {entries.map((entry) => (
        <div key={`${entry.kind}:${entry.id}`} className="zorai-code-timeline__entry">
          <button
            type="button"
            title={entry.kind === "commit" ? `${entry.author} · ${entry.id.slice(0, 12)} — compare with the current file` : "Compare this saved version with the current file"}
            onClick={() => void contentFor(entry).then((content) => { if (content !== null) onCompare(content); })}
          >
            <span className={`zorai-code-timeline__dot zorai-code-timeline__dot--${entry.kind}`} />
            <span className="zorai-code-timeline__label">{entry.label}</span>
            <span className="zorai-code-timeline__time">{relativeTime(entry.at)}</span>
          </button>
          <button
            type="button"
            className="zorai-code-timeline__restore"
            title="Put this version in the editor (unsaved, so you can still undo)"
            onClick={() => void contentFor(entry).then((content) => { if (content !== null) onRestore(content); })}
          >
            Restore
          </button>
        </div>
      ))}
    </details>
  );
}
