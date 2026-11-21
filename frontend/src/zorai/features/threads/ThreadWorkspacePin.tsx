import { useState } from "react";
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
      <div className="zorai-composer-workspace-context-header">
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
      {previewOpen ? (
        <div className="zorai-composer-context-preview">
          <strong>Effective daemon workspace context</strong>
          <code>Workspace: {workspaceContext.root}</code>
          {workspaceContext.activeFile ? <code>Active file: {workspaceContext.activeFile}</code> : null}
          {workspaceContext.selection && workspaceContext.activeFile ? <code>Selection: {workspaceContext.activeFile}:{workspaceContext.selection.startLine}-{workspaceContext.selection.endLine}</code> : null}
          {workspaceContext.attachedFiles.length > 0 ? <code>Explicit attachments: {workspaceContext.attachedFiles.join(", ")}</code> : null}
          <span>File contents are read from disk by tools on demand; they are not injected automatically.</span>
        </div>
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
