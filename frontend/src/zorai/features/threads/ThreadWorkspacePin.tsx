import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useAgentChatPanelRuntime } from "@/components/agent-chat-panel/runtime/context";
import { useWorkspaceContextStore } from "@/lib/workspaceContextStore";
import { useCodeWorkspaceBindingStore } from "../code/codeWorkspaceBindingStore";
import { ComposerIcon } from "./ThreadComposer";

export function ThreadWorkspacePin() {
  const runtime = useAgentChatPanelRuntime();
  const activeThreadId = runtime.activeThreadId;
  const workspaceContext = useWorkspaceContextStore((state) => activeThreadId ? state.byThreadId[activeThreadId] : undefined);
  const clearWorkspaceRoot = useWorkspaceContextStore((state) => state.clearRoot);
  const bindWorkspaceRoot = useWorkspaceContextStore((state) => state.bindRoot);
  const [previewOpen, setPreviewOpen] = useState(false);
  const anchorRef = useRef<HTMLDivElement | null>(null);
  const previewRef = useRef<HTMLDivElement | null>(null);
  const [previewPlace, setPreviewPlace] = useState<{ right: number; bottom: number } | null>(null);
  useEffect(() => {
    if (!previewOpen) return;
    const place = () => {
      const rect = anchorRef.current?.getBoundingClientRect();
      if (rect) setPreviewPlace({ right: Math.max(8, Math.round(window.innerWidth - rect.right)), bottom: Math.round(window.innerHeight - rect.top + 6) });
    };
    place();
    const onPointerDown = (event: PointerEvent) => {
      const target = event.target instanceof Node ? event.target : null;
      if (target && (previewRef.current?.contains(target) || anchorRef.current?.contains(target))) return;
      setPreviewOpen(false);
    };
    const onKeyDown = (event: KeyboardEvent) => { if (event.key === "Escape") setPreviewOpen(false); };
    window.addEventListener("resize", place);
    document.addEventListener("pointerdown", onPointerDown, true);
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("resize", place);
      document.removeEventListener("pointerdown", onPointerDown, true);
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [previewOpen]);
  const pickWorkspaceRoot = async () => {
    if (!activeThreadId || !window.zorai?.workspaceSelectFolder) return;
    const selection = await window.zorai.workspaceSelectFolder().catch(() => null);
    if (!selection || selection.canceled || !selection.root) return;
    bindWorkspaceRoot(activeThreadId, selection.root.root);
  };

  if (!activeThreadId) return null;
  if (!workspaceContext) {
    return (
      <div className="zorai-compact-session__workspace">
        <button type="button" className="zorai-composer-context-chip zorai-workspace-pin__empty" onClick={() => void pickWorkspaceRoot()}>
          <WorkspaceFolderIcon />
          <span>Pin workspace</span>
        </button>
      </div>
    );
  }

  return (
    <div className="zorai-compact-session__workspace">
      <div ref={anchorRef} className="zorai-composer-workspace-context-header">
        <button type="button" className="zorai-composer-context-chip zorai-workspace-pin__name" onClick={() => setPreviewOpen((open) => !open)} title={workspaceContext.root}>
          <WorkspaceFolderIcon />
          <span>{workspaceContext.root.split(/[\\/]/).slice(-1)[0]}</span>
        </button>
        <button
          type="button"
          className="zorai-composer-context-chip zorai-workspace-pin__icon"
          aria-label="Change workspace"
          title="Change workspace"
          onClick={() => void pickWorkspaceRoot()}
        ><ComposerIcon kind="edit" /></button>
        <button
          type="button"
          className="zorai-composer-context-chip zorai-workspace-pin__icon"
          aria-label="Unpin workspace from thread"
          title="Unpin workspace from thread"
          onClick={() => {
            const unpinnedRoot = workspaceContext.root;
            clearWorkspaceRoot(activeThreadId);
            setPreviewOpen(false);
            const daemonThreadId = runtime.activeThread?.daemonThreadId;
            const bindings = useCodeWorkspaceBindingStore.getState();
            bindings.forgetProjectThread(unpinnedRoot, activeThreadId);
            if (daemonThreadId) bindings.forgetProjectThread(unpinnedRoot, daemonThreadId);
            if (daemonThreadId) void window.zorai?.agentSetThreadWorkspaceContext?.(daemonThreadId, null);
          }}
        ><ComposerIcon kind="cancel" /></button>
      </div>
      {previewOpen && previewPlace && typeof document !== "undefined" ? createPortal(
        <div ref={previewRef} className="zorai-workspace-context-card" role="tooltip" style={{ right: previewPlace.right, bottom: previewPlace.bottom }}>
          <header className="zorai-workspace-context-card__header">
            <WorkspaceFolderIcon />
            <div>
              <strong>{workspaceContext.root.split(/[\\/]/).filter(Boolean).slice(-1)[0] ?? workspaceContext.root}</strong>
              <span>Workspace the agent works in</span>
            </div>
          </header>
          <dl className="zorai-workspace-context-card__rows">
            <div><dt>Folder</dt><dd title={workspaceContext.root}>{workspaceContext.root}</dd></div>
            {workspaceContext.activeFile ? <div><dt>Active file</dt><dd title={workspaceContext.activeFile}>{workspaceContext.activeFile}</dd></div> : null}
            {workspaceContext.selection && workspaceContext.activeFile ? <div><dt>Selection</dt><dd>Lines {workspaceContext.selection.startLine}–{workspaceContext.selection.endLine}</dd></div> : null}
            {workspaceContext.attachedFiles.length > 0 ? <div><dt>Attached</dt><dd title={workspaceContext.attachedFiles.join(", ")}>{workspaceContext.attachedFiles.join(", ")}</dd></div> : null}
          </dl>
          <p className="zorai-workspace-context-card__note">Tools read files from this folder when they need them. Nothing is sent to the model automatically.</p>
        </div>,
        document.body,
      ) : null}
    </div>
  );
}

function WorkspaceFolderIcon() {
  return (
    <svg className="zorai-workspace-pin__folder" viewBox="0 0 16 16" aria-hidden="true">
      <path d="M2 4.5A1.5 1.5 0 0 1 3.5 3h2.4l1.3 1.5h5.3A1.5 1.5 0 0 1 14 6v5.5a1.5 1.5 0 0 1-1.5 1.5h-9A1.5 1.5 0 0 1 2 11.5z" />
    </svg>
  );
}
