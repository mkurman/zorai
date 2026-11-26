import { useEffect, useMemo, useState } from "react";
import { useAgentChatPanelRuntime } from "@/components/agent-chat-panel/runtime/context";
import { refreshDaemonThreadMetadataIntoLocalState } from "@/components/agent-chat-panel/runtime/daemonHelpers";
import { useAgentStore, type AgentMessage } from "@/lib/agentStore";
import { fetchThreadWorkContext, type ThreadWorkContext, type WorkContextEntry } from "@/lib/agentWorkContext";
import { getBridge } from "@/lib/bridge";
import { shortenHomePath } from "@/lib/workspaceStore";
import { useThreadFilePreview } from "./ThreadFilePreviewContext";
import { useWorkspaceContextStore } from "@/lib/workspaceContextStore";
import { useWorkspaceEditorRequestStore } from "@/lib/workspaceEditorRequestStore";
import { SpawnedContext } from "./ThreadsSpawnedContext";
import { ThreadRuntimeBar } from "./ThreadRuntimeBar";
import { resolveThreadOwnerRuntimeProfile } from "./threadOwnerRuntime";
import { revealThreadMessage } from "./useThreadMessageWindow";
import { ThreadGoalStatus } from "./ThreadGoalStatus";
import { goalRunForThread } from "./threadGoalModel";
import { formatGoalRunStatus } from "@/lib/goalRuns";
import { ContextMeter, ContextRow } from "./ThreadContextRow";
import { ContextUsageDetails, useContextUsage } from "./ComposerContextCircle";
import { useThreadWorkspace } from "./ThreadWorkspacePin";
import { WorkFileList, WorkTodoList } from "./WorkFileList";

export function ThreadsContext() {
  const runtime = useAgentChatPanelRuntime();
  const activeThread = runtime.activeThread;
  const [workContext, setWorkContext] = useState<ThreadWorkContext>({ threadId: "", entries: [] });
  const daemonThreadId = activeThread?.daemonThreadId ?? null;
  const spawnedCount = useMemo(
    () => countSpawnedNodes(runtime.spawnedAgentTree),
    [runtime.spawnedAgentTree],
  );
  const agentSettings = useAgentStore((state) => state.agentSettings);
  const conciergeConfig = useAgentStore((state) => state.conciergeConfig);
  const coderConfig = useAgentStore((state) => state.coderConfig);
  const subAgents = useAgentStore((state) => state.subAgents);
  const profile = activeThread ? resolveThreadOwnerRuntimeProfile(activeThread, subAgents, agentSettings, conciergeConfig, coderConfig) : null;
  const usage = useContextUsage(activeThread ?? null, runtime.messages);
  const workspace = useThreadWorkspace();
  const goal = useMemo(
    () => goalRunForThread(runtime.goalRunsForTrace, [daemonThreadId, activeThread?.id]),
    [activeThread?.id, daemonThreadId, runtime.goalRunsForTrace],
  );

  useEffect(() => {
    if (!daemonThreadId) {
      setWorkContext({ threadId: "", entries: [] });
      return;
    }

    let cancelled = false;
    void fetchThreadWorkContext(daemonThreadId).then((next) => {
      if (!cancelled) {
        setWorkContext(next);
      }
    });

    return () => {
      cancelled = true;
    };
  }, [daemonThreadId]);

  useEffect(() => {
    if (!daemonThreadId) {
      return;
    }

    void refreshDaemonThreadMetadataIntoLocalState({
      daemonThreadId,
      setThreadTodos: useAgentStore.getState().setThreadTodos,
      setDaemonTodosByThread: () => {},
    });
  }, [daemonThreadId]);

  useEffect(() => {
    const bridge = getBridge();
    if (!daemonThreadId || !bridge?.onAgentEvent) {
      return;
    }

    return bridge.onAgentEvent((event: any) => {
      if (event?.type !== "work_context_update" || event?.thread_id !== daemonThreadId) {
        return;
      }
      void fetchThreadWorkContext(daemonThreadId).then(setWorkContext);
    });
  }, [daemonThreadId]);

  return (
    <div className="zorai-thread-context-stack">
      <div className="zorai-context-group">
        <div className="zorai-context-group__title">Thread</div>
        {activeThread && profile ? (
          <ContextRow icon="runtime" label="Runtime" value={[profile.provider, profile.model, profile.effort].filter(Boolean).join(" · ")}>
            <ThreadRuntimeBar key={activeThread.id} thread={activeThread} />
          </ContextRow>
        ) : null}
        {activeThread ? (
          <ContextRow icon="workspace" label="Workspace" value={workspace.workspaceContext ? workspace.workspaceContext.root.split(/[\\/]/).filter(Boolean).slice(-1)[0] : "Not set"}>
            <div className="zorai-context-workspace">
              {workspace.workspaceContext ? <code title={workspace.workspaceContext.root}>{shortenHomePath(workspace.workspaceContext.root)}</code> : <p>Pick the folder the agent works in for this thread.</p>}
              <div className="zorai-context-workspace__actions">
                <button type="button" className="zorai-ghost-button zorai-ghost-button--compact" onClick={() => void workspace.pick()}>{workspace.workspaceContext ? "Change…" : "Set workspace…"}</button>
                {workspace.workspaceContext ? <button type="button" className="zorai-ghost-button zorai-ghost-button--compact" onClick={workspace.unpin}>Unpin</button> : null}
              </div>
            </div>
          </ContextRow>
        ) : null}
        <ContextRow icon="context" label="Context" value={<><span>{usage.pct}%</span><ContextMeter percent={usage.pct} /></>}>
          <div className="zorai-context-usage">
            <ContextUsageDetails usage={usage} thread={activeThread ?? null} />
          </div>
        </ContextRow>
      </div>

      <div className="zorai-context-group">
        <div className="zorai-context-group__title">Work</div>
        <ContextRow key={`todos:${activeThread?.id ?? ""}`} icon="todos" label="Todos" value={runtime.todos.length} defaultOpen={runtime.todos.length > 0}>
          {runtime.todos.length > 0 ? <WorkTodoList todos={runtime.todos} /> : <div className="zorai-empty">No todos for this thread yet.</div>}
        </ContextRow>
        <ContextRow icon="files" label="Files" value={workContext.entries.length}>
          <FilesContext entries={workContext.entries} activeThreadId={activeThread?.id ?? null} />
        </ContextRow>
        <ContextRow icon="spawned" label="Spawned" value={spawnedCount}>
          <div className="zorai-compact-session__spawned-tree">
            <SpawnedContext
              tree={runtime.spawnedAgentTree}
              selectedDaemonThreadId={runtime.activeThread?.daemonThreadId ?? null}
              canGoBackThread={runtime.canGoBackThread}
              threadNavigationDepth={runtime.threadNavigationDepth}
              backThreadTitle={runtime.backThreadTitle}
              canOpenSpawnedThread={runtime.canOpenSpawnedThread}
              openSpawnedThread={runtime.openSpawnedThread}
              goBackThread={runtime.goBackThread}
              compact
            />
          </div>
        </ContextRow>
        {goal ? (
          <ContextRow icon="goal" label="Goal" value={formatGoalRunStatus(goal.status)}>
            <ThreadGoalStatus goal={goal} />
          </ContextRow>
        ) : null}
        {runtime.pinnedMessages.length > 0 ? (
          <ContextRow icon="pinned" label="Pinned" value={runtime.pinnedMessages.length}>
            <PinnedThreadContext
              messages={runtime.pinnedMessages}
              onJumpToMessage={(messageId) => {
                revealThreadMessage(messageId);
              }}
              onUnpinMessage={(messageId) => runtime.activeThreadId
                ? runtime.unpinMessageForCompaction(runtime.activeThreadId, messageId)
                : undefined}
            />
          </ContextRow>
        ) : null}
      </div>
    </div>
  );
}

function FilesContext({ entries, activeThreadId }: { entries: WorkContextEntry[]; activeThreadId: string | null }) {
  const { openThreadFilePreview } = useThreadFilePreview();
  const workspaceContext = useWorkspaceContextStore((state) =>
    activeThreadId ? state.byThreadId[activeThreadId] : undefined,
  );
  const requestFileView = useWorkspaceEditorRequestStore((state) => state.requestFileView);
  if (entries.length === 0) {
    return <div className="zorai-empty">No file or artifact context recorded for this thread yet.</div>;
  }
  const workspaceRoot = workspaceContext?.root ?? null;
  return (
    <div className="zorai-compact-session__files">
      <WorkFileList
        entries={entries}
        workspaceRoot={workspaceRoot}
        onOpen={(entry) => {
          const entryWithinWorkspace = workspaceRoot !== null && entry.path && (!entry.repoRoot || entry.repoRoot === workspaceRoot);
          if (activeThreadId && entryWithinWorkspace && entry.isText !== false) {
            requestFileView(activeThreadId, entry.path, entry.kind === "repo_change" || entry.changeKind !== null ? "diff" : "edit");
          } else {
            openThreadFilePreview(entry);
          }
        }}
      />
    </div>
  );
}

function PinnedThreadContext({
  messages,
  onJumpToMessage,
  onUnpinMessage,
}: {
  messages: AgentMessage[];
  onJumpToMessage: (messageId: string) => void;
  onUnpinMessage: (messageId: string) => void | Promise<unknown> | undefined;
}) {
  return (
    <section className="zorai-pinned-context">
      <div>
        <div className="zorai-section-label">Pinned Messages</div>
      </div>
      {messages.map((message) => (
        <article key={message.id} className="zorai-pinned-message">
          <div>
            <strong>{message.role}</strong>
            <span>{formatTime(message.createdAt)}</span>
          </div>
          <p>{message.content || summarizePinnedMessage(message)}</p>
          <div className="zorai-card-actions">
            <button type="button" className="zorai-ghost-button" onClick={() => onJumpToMessage(message.id)}>
              Jump
            </button>
            <button type="button" className="zorai-ghost-button" onClick={() => void onUnpinMessage(message.id)}>
              Unpin
            </button>
          </div>
        </article>
      ))}
    </section>
  );
}

function countSpawnedNodes(tree: ReturnType<typeof useAgentChatPanelRuntime>["spawnedAgentTree"]): number {
  if (!tree) return 0;
  const countNode = (node: NonNullable<typeof tree>["roots"][number]): number =>
    1 + node.children.reduce((sum, child) => sum + countNode(child), 0);
  return (tree.anchor ? countNode(tree.anchor) : 0) + tree.roots.reduce((sum, root) => sum + countNode(root), 0);
}

function summarizePinnedMessage(message: AgentMessage): string {
  if (message.toolName) return `Tool: ${message.toolName} (${message.toolStatus ?? "done"})`;
  return "No text content";
}

function formatTime(timestamp: number): string {
  return Number.isFinite(timestamp)
    ? new Date(timestamp).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })
    : "pending";
}
