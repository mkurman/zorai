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
        <button type="button" className="zorai-composer-context-chip" onClick={() => void pickWorkspaceRoot()}>Pin workspace…</button>
      </div>
    );
  }

  return (
    <div className="zorai-compact-session__workspace">
      <div className="zorai-composer-workspace-context-header">
        <button type="button" className="zorai-composer-context-chip" onClick={() => setPreviewOpen((open) => !open)} title={workspaceContext.root}>
          {workspaceContext.root.split(/[\\/]/).slice(-1)[0]}
        </button>
        <button
          type="button"
          className="zorai-composer-context-chip"
          aria-label="Change workspace"
          title="Change workspace"
          onClick={() => void pickWorkspaceRoot()}
        ><ComposerIcon kind="edit" /></button>
        <button
          type="button"
          className="zorai-composer-context-chip"
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
