import { Suspense, lazy, useEffect, useState } from "react";
import { getBridge } from "@/lib/bridge";
import { languageForWorkspacePath } from "./codeLanguages";

const WorkspaceDiffEditor = lazy(() => import("@/components/WorkspaceCodeEditor").then((module) => ({ default: module.WorkspaceDiffEditor })));

type DiffState =
  | { status: "loading" }
  | { status: "error"; message: string }
  | { status: "ready"; before: string; current: string; resolvedPath: string; created: boolean; deleted: boolean };

export type AgentOperationDiffTarget = {
  operationId: string;
  path: string;
  tool: string | null;
};

export async function loadAgentOperationDiff(target: AgentOperationDiffTarget): Promise<DiffState> {
  const bridge = getBridge();
  if (!bridge?.agentGetFileOperationBefore || !bridge.readFsText) {
    return { status: "error", message: "Operation diffs require the daemon bridge." };
  }
  const before = await bridge.agentGetFileOperationBefore(target.operationId, target.path);
  if (!before.available) {
    return { status: "error", message: before.error ?? "No retained snapshot exists for this operation." };
  }
  if (before.binary) {
    return { status: "error", message: "The retained snapshot is binary and cannot be shown as a text diff." };
  }
  const resolvedPath = before.resolved_path ?? target.path;
  const current = await bridge.readFsText(resolvedPath).catch(() => null);
  return {
    status: "ready",
    before: before.content ?? "",
    current: current ?? "",
    resolvedPath,
    created: !before.before_existed,
    deleted: current === null,
  };
}

export function AgentOperationDiffView({
  target,
  revertible,
  revertReason,
  onRevert,
  onOpenFile,
  onClose,
}: {
  target: AgentOperationDiffTarget;
  revertible: boolean;
  revertReason: string | null;
  onRevert: () => void;
  onOpenFile: (path: string) => void;
  onClose: () => void;
}) {
  const [state, setState] = useState<DiffState>({ status: "loading" });

  useEffect(() => {
    let cancelled = false;
    setState({ status: "loading" });
    void loadAgentOperationDiff(target)
      .then((next) => {
        if (!cancelled) setState(next);
      })
      .catch((reason: unknown) => {
        if (!cancelled) setState({ status: "error", message: reason instanceof Error ? reason.message : String(reason) });
      });
    return () => {
      cancelled = true;
    };
  }, [target]);

  const ready = state.status === "ready" ? state : null;
  const label = ready?.created ? "Created by this operation" : ready?.deleted ? "Deleted since this operation" : "Before operation → current file";

  return (
    <section className="zorai-file-preview-overlay zorai-operation-diff" aria-label="Agent operation diff">
      <header className="zorai-file-preview-overlay__header">
        <div>
          <div className="zorai-kicker">Agent operation{target.tool ? ` · ${target.tool}` : ""}</div>
          <h2>{ready?.resolvedPath ?? target.path}</h2>
          <span>{label}</span>
        </div>
        <div className="zorai-operation-diff__actions">
          {ready && !ready.deleted ? (
            <button type="button" className="zorai-ghost-button" onClick={() => onOpenFile(ready.resolvedPath)}>Open file</button>
          ) : null}
          <button
            type="button"
            className="zorai-ghost-button zorai-operation-diff__revert"
            disabled={!revertible}
            title={revertible ? "Restore every file in this operation to its pre-operation content" : revertReason ?? "This operation cannot be reverted."}
            onClick={onRevert}
          >
            Revert operation
          </button>
          <button type="button" className="zorai-ghost-button zorai-file-preview-overlay__close" aria-label="Close diff" onClick={onClose}>x</button>
        </div>
      </header>
      <div className="zorai-file-preview-overlay__body">
        {!revertible && revertReason ? <div className="zorai-empty">{revertReason}</div> : null}
        {state.status === "loading" ? <div className="zorai-empty">Loading operation diff…</div> : null}
        {state.status === "error" ? <div className="zorai-empty zorai-empty--danger">{state.message}</div> : null}
        {ready ? (
          <div className="zorai-operation-diff__editor">
            <Suspense fallback={<div className="zorai-empty">Loading diff editor…</div>}>
              <WorkspaceDiffEditor original={ready.before} modified={ready.current} language={languageForWorkspacePath(ready.resolvedPath)} />
            </Suspense>
          </div>
        ) : null}
      </div>
    </section>
  );
}
