import { useEffect, useMemo, useState } from "react";
import { useAgentChatPanelRuntime } from "@/components/agent-chat-panel/runtime/context";
import { refreshDaemonThreadMetadataIntoLocalState } from "@/components/agent-chat-panel/runtime/daemonHelpers";
import { useAgentStore, type AgentMessage, type AgentThread, type AgentTodoItem } from "@/lib/agentStore";
import { fetchThreadWorkContext, type ThreadWorkContext, type WorkContextEntry } from "@/lib/agentWorkContext";
import { getBridge } from "@/lib/bridge";
import { shortenHomePath } from "@/lib/workspaceStore";
import { workContextKindColor, workContextKindLabel } from "@/components/agent-chat-panel/tasks-view/helpers";
import {
  threadContextEntryDisplayPath,
  threadContextEntryKey,
} from "./threadContextPreview";
import { useThreadFilePreview } from "./ThreadFilePreviewContext";
import { useWorkspaceContextStore } from "@/lib/workspaceContextStore";
import { useWorkspaceEditorRequestStore } from "@/lib/workspaceEditorRequestStore";
import { SpawnedContext } from "./ThreadsSpawnedContext";
import { ThreadRuntimeBar } from "./ThreadRuntimeBar";
import { resolveThreadOwnerRuntimeProfile } from "./threadOwnerRuntime";
import { revealThreadMessage } from "./useThreadMessageWindow";

type ContextTab = "todos" | "files" | "spawned";

export function ThreadsContext() {
  const runtime = useAgentChatPanelRuntime();
  const activeThread = runtime.activeThread;
  const [activeTab, setActiveTab] = useState<ContextTab>("todos");
  const [workContext, setWorkContext] = useState<ThreadWorkContext>({ threadId: "", entries: [] });
  const daemonThreadId = activeThread?.daemonThreadId ?? null;
  const spawnedCount = useMemo(
    () => countSpawnedNodes(runtime.spawnedAgentTree),
    [runtime.spawnedAgentTree],
  );
  const agentSettings = useAgentStore((state) => state.agentSettings);
  const conciergeConfig = useAgentStore((state) => state.conciergeConfig);
  const subAgents = useAgentStore((state) => state.subAgents);
  const contextWindowTokens = activeThread
    ? resolveThreadOwnerRuntimeProfile(activeThread, subAgents, agentSettings, conciergeConfig).contextWindowTokens
    : Math.max(1, Math.trunc(agentSettings.context_window_tokens || 1));
  const currentContextTokens = resolveCurrentContextTokens(activeThread, runtime.messages);

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
      {activeThread ? (
        <section className="zorai-thread-runtime-setup">
          <div className="zorai-section-label">Runtime</div>
          <ThreadRuntimeBar key={activeThread.id} thread={activeThread} />
        </section>
      ) : null}

      <ContextWindowSummary
        currentTokens={currentContextTokens}
        contextWindowTokens={contextWindowTokens}
        pinnedCount={runtime.pinnedMessages.length}
      />

      <div className="zorai-context-tabs" role="tablist" aria-label="Thread context">
        <ContextTabButton id="todos" label="Todos" count={runtime.todos.length} activeTab={activeTab} onSelect={setActiveTab} />
        <ContextTabButton id="files" label="Files" count={workContext.entries.length} activeTab={activeTab} onSelect={setActiveTab} />
        <ContextTabButton id="spawned" label="Spawned" count={spawnedCount} activeTab={activeTab} onSelect={setActiveTab} />
      </div>

      {activeTab === "todos" ? <TodoContext todos={runtime.todos} /> : null}
      {activeTab === "files" ? <FilesContext entries={workContext.entries} activeThreadId={activeThread?.id ?? null} /> : null}
      {activeTab === "spawned" ? (
        <SpawnedContext
          tree={runtime.spawnedAgentTree}
          selectedDaemonThreadId={runtime.activeThread?.daemonThreadId ?? null}
          canGoBackThread={runtime.canGoBackThread}
          threadNavigationDepth={runtime.threadNavigationDepth}
          backThreadTitle={runtime.backThreadTitle}
          canOpenSpawnedThread={runtime.canOpenSpawnedThread}
          openSpawnedThread={runtime.openSpawnedThread}
          goBackThread={runtime.goBackThread}
        />
      ) : null}

      {runtime.pinnedMessages.length > 0 ? (
        <PinnedThreadContext
          messages={runtime.pinnedMessages}
          onJumpToMessage={(messageId) => {
            revealThreadMessage(messageId);
          }}
          onUnpinMessage={(messageId) => runtime.activeThreadId
            ? runtime.unpinMessageForCompaction(runtime.activeThreadId, messageId)
            : undefined}
        />
      ) : null}
    </div>
  );
}

function ContextWindowSummary({
  currentTokens,
  contextWindowTokens,
  pinnedCount,
}: {
  currentTokens: number;
  contextWindowTokens: number;
  pinnedCount: number;
}) {
  const utilization = contextWindowTokens > 0
    ? Math.min(100, Math.round((currentTokens / contextWindowTokens) * 100))
    : 0;

  return (
    <section className="zorai-context-window-summary">
      <div>
        <div className="zorai-section-label">Context Window</div>
        <strong>{formatTokens(currentTokens)} / {formatTokens(contextWindowTokens)} tokens</strong>
      </div>
      <div className="zorai-context-window-meter" aria-label={`${utilization}% context used`}>
        <span style={{ width: `${utilization}%` }} />
      </div>
      <div className="zorai-context-window-meta">
        <span>{utilization}% used</span>
        <span>{pinnedCount} pinned</span>
      </div>
    </section>
  );
}

function ContextTabButton({
  id,
  label,
  count,
  activeTab,
  onSelect,
}: {
  id: ContextTab;
  label: string;
  count: number;
  activeTab: ContextTab;
  onSelect: (tab: ContextTab) => void;
}) {
  return (
    <button
      type="button"
      role="tab"
      aria-selected={activeTab === id}
      className={activeTab === id ? "zorai-context-tab zorai-context-tab--active" : "zorai-context-tab"}
      onClick={() => onSelect(id)}
    >
      <span>{label}</span>
      <b>{count}</b>
    </button>
  );
}

function TodoContext({ todos }: { todos: AgentTodoItem[] }) {
  if (todos.length === 0) {
    return <div className="zorai-empty">No todos for this thread yet.</div>;
  }

  return (
    <section className="zorai-todo-context-list">
      {todos.map((todo) => (
        <article key={todo.id} className="zorai-todo-context-item">
          <span className={`zorai-todo-checkbox zorai-todo-checkbox--${todo.status}`} aria-hidden="true" />
          <div>
            <span className="zorai-todo-context-title">{todo.content}</span>
            <span className="zorai-todo-context-status">{todo.status.replace(/_/g, " ")}</span>
          </div>
        </article>
      ))}
    </section>
  );
}

function FilesContext({ entries, activeThreadId }: { entries: WorkContextEntry[]; activeThreadId: string | null }) {
  const { openThreadFilePreview, previewTarget } = useThreadFilePreview();
  const workspaceContext = useWorkspaceContextStore((state) =>
    activeThreadId ? state.byThreadId[activeThreadId] : undefined,
  );
  const requestFileView = useWorkspaceEditorRequestStore((state) => state.requestFileView);
  const [selectedKey, setSelectedKey] = useState<string | null>(null);
  const overlayEntryKey = previewTarget ? threadContextEntryKey(previewTarget.entry) : null;

  useEffect(() => {
    setSelectedKey((current) => {
      if (current && entries.some((entry) => threadContextEntryKey(entry) === current)) {
        return current;
      }
      return entries[0] ? threadContextEntryKey(entries[0]) : null;
    });
  }, [entries]);

  if (entries.length === 0) {
    return <div className="zorai-empty">No file or artifact context recorded for this thread yet.</div>;
  }

  return (
    <section className="zorai-file-context">
      <div className="zorai-file-context__list">
        {entries.slice(0, 24).map((entry) => {
          const entryKey = threadContextEntryKey(entry);
          const selected = entryKey === selectedKey || entryKey === overlayEntryKey;
          const workspaceRoot = workspaceContext?.root ?? null;
          const entryWithinWorkspace = workspaceRoot !== null && entry.path
            && (!entry.repoRoot || entry.repoRoot === workspaceRoot);
          const canOpenInEditor = Boolean(activeThreadId && entryWithinWorkspace && entry.isText !== false);
          const editorView: "diff" | "edit" = entry.kind === "repo_change" || entry.changeKind !== null ? "diff" : "edit";
          const title = canOpenInEditor
            ? editorView === "diff" ? "Open inline diff in editor" : "Open in editor"
            : "Preview file";
          return (
            <button
              key={entryKey}
              type="button"
              className={selected ? "zorai-file-context__item zorai-file-context__item--active" : "zorai-file-context__item"}
              title={title}
              onClick={() => {
                setSelectedKey(entryKey);
                // When a Code workspace is bound, open with inline git diff in the editor; otherwise fall back to preview overlay.
                if (canOpenInEditor && activeThreadId) {
                  requestFileView(activeThreadId, entry.path, editorView);
                } else {
                  openThreadFilePreview(entry);
                }
              }}
            >
              <span style={{ color: workContextKindColor(entry) }}>{workContextKindLabel(entry)}</span>
              <strong>{threadContextEntryDisplayPath(entry, shortenHomePath)}</strong>
              {entry.previousPath ? <small>from {shortenHomePath(entry.previousPath)}</small> : null}
            </button>
          );
        })}
      </div>
    </section>
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

function resolveCurrentContextTokens(thread: AgentThread | undefined, messages: AgentMessage[]): number {
  if (typeof thread?.activeContextWindowTokens === "number" && thread.activeContextWindowTokens > 0) {
    return Math.trunc(thread.activeContextWindowTokens);
  }

  return messages.reduce(
    (sum, message) => sum + Math.ceil((message.content?.length ?? 0) / 4) + 8,
    0,
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

function formatTokens(value: number): string {
  return Math.max(0, Math.trunc(value)).toLocaleString();
}

function formatTime(timestamp: number): string {
  return Number.isFinite(timestamp)
    ? new Date(timestamp).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })
    : "pending";
}
