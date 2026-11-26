import { useEffect, useMemo, useRef, useState } from "react";
import { getBridge } from "@/lib/bridge";
import { pushToast } from "@/lib/toastStore";
import { pendingReviewFiles, reviewKey, useAgentDiffReviewStore, type AgentChangeEntry, type PendingReviewFile } from "./agentDiffReviewStore";

type Snapshot = { before: string; existed: boolean } | null;

export function useAgentDiffReview({ root, threadId, changes, activePath, openFile, onFileRewritten, saveFile }: {
  root: string | null;
  threadId: string | null;
  changes: AgentChangeEntry[];
  activePath: string | null;
  openFile: (path: string) => void;
  onFileRewritten: (path: string, file: ZoraiWorkspaceFile | null) => void;
  saveFile: (path: string) => void;
}) {
  const decided = useAgentDiffReviewStore((state) => (threadId ? state.decided[threadId] : undefined));
  const markDecided = useAgentDiffReviewStore((state) => state.markDecided);
  const [snapshots, setSnapshots] = useState<Record<string, Snapshot>>({});
  const [busy, setBusy] = useState(false);
  const loadingRef = useRef(new Map<string, Promise<Snapshot>>());
  const candidates = useMemo(() => pendingReviewFiles(changes, decided ?? []), [changes, decided]);
  const pending = useMemo(() => candidates.filter((file) => snapshots[snapshotKey(file)] !== null), [candidates, snapshots]);
  const active = pending.find((file) => file.path === activePath) ?? null;

  const loadSnapshot = async (file: PendingReviewFile): Promise<Snapshot> => {
    const key = snapshotKey(file);
    if (snapshots[key] !== undefined) return snapshots[key];
    const inFlight = loadingRef.current.get(key);
    if (inFlight) return inFlight;
    const request = (async () => {
      const result = await getBridge()?.agentGetFileOperationBefore?.(file.baselineOperationId, file.path).catch(() => null);
      const snapshot = result?.available && !result.binary ? { before: result.content ?? "", existed: result.before_existed } : null;
      setSnapshots((current) => ({ ...current, [key]: snapshot }));
      loadingRef.current.delete(key);
      return snapshot;
    })();
    loadingRef.current.set(key, request);
    return request;
  };

  useEffect(() => {
    if (active && snapshots[snapshotKey(active)] === undefined) void loadSnapshot(active);
  });

  const decide = (files: PendingReviewFile[]) => {
    if (!threadId) return;
    markDecided(threadId, files.flatMap((file) => file.operationIds.map((operationId) => reviewKey(operationId, file.path))));
  };

  const undoFiles = async (files: PendingReviewFile[]) => {
    const bridge = getBridge();
    if (!root || !bridge?.workspaceWriteFile) return;
    setBusy(true);
    const failed: string[] = [];
    for (const file of files) {
      const snapshot = await loadSnapshot(file);
      try {
        if (!snapshot) throw new Error("no snapshot");
        if (snapshot.existed) onFileRewritten(file.path, await bridge.workspaceWriteFile(root, file.path, snapshot.before));
        else if (await bridge.workspaceDeletePath?.(root, file.path)) onFileRewritten(file.path, null);
        else throw new Error("delete failed");
        decide([file]);
      } catch {
        failed.push(file.path);
      }
    }
    setBusy(false);
    if (failed.length > 0) pushToast(`Could not undo the agent's changes in ${failed.join(", ")}.`, "error");
  };

  const activeSnapshot = active ? snapshots[snapshotKey(active)] : undefined;
  const reviewForPath = (path: string) => (active && active.path === path && activeSnapshot ? {
    key: snapshotKey(active),
    before: activeSnapshot.before,
    onSettled: () => decide([active]),
    onEdit: () => saveFile(path),
  } : null);

  const index = active ? pending.indexOf(active) : -1;
  const step = (offset: number) => {
    if (pending.length === 0) return;
    openFile(pending[(index + offset + pending.length) % pending.length].path);
  };

  const bar = pending.length > 0 ? (
    <div className="zorai-agent-review-bar" role="toolbar" aria-label="Review the agent's changes">
      <span className="zorai-agent-review-bar__count">
        {active ? `File ${index + 1} of ${pending.length}` : `${pending.length} file${pending.length === 1 ? "" : "s"} changed by the agent`}
      </span>
      <button type="button" title="Previous changed file" onClick={() => step(-1)}>‹</button>
      <button type="button" title={active ? "Next changed file" : "Open the first changed file"} onClick={() => step(active ? 1 : 0)}>›</button>
      {active ? (
        <>
          <button type="button" disabled={busy} title="Keep every agent change in this file" onClick={() => decide([active])}>Keep file</button>
          <button type="button" disabled={busy} title="Restore this file to how it was before the agent changed it" onClick={() => void undoFiles([active])}>Undo file</button>
        </>
      ) : null}
      <button type="button" disabled={busy} title="Keep every agent change in every file" onClick={() => decide(pending)}>Keep all</button>
      <button
        type="button"
        disabled={busy}
        className="is-danger"
        title="Restore every changed file to how it was before the agent changed it"
        onClick={() => { if (window.confirm(`Undo the agent's changes in ${pending.length} file(s)? Files the agent created will be deleted.`)) void undoFiles(pending); }}
      >
        Undo all
      </button>
    </div>
  ) : null;

  return { reviewForPath, bar };
}

function snapshotKey(file: PendingReviewFile) {
  return `${file.baselineOperationId}\u0000${file.path}`;
}
