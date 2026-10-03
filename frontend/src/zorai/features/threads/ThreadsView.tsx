import { LoadingState } from "@/components/LoadingState";
import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode, type UIEvent } from "react";
import { MemoizedToolEventRow } from "@/components/agent-chat-panel/chat-view/ToolEventRow";
import { ToolEventList } from "@/components/agent-chat-panel/chat-view/ToolEventList";
import { buildDisplayItems } from "@/components/agent-chat-panel/chat-view/helpers";
import { useAgentChatPanelRuntime } from "@/components/agent-chat-panel/runtime/context";
import {
  beginProgrammaticThreadHistoryScroll,
  consumeThreadHistoryScroll,
  endProgrammaticThreadHistoryScroll,
  fillThreadHistoryIfUnscrollable,
  resetThreadHistoryPagination,
  setFollowThreadHistoryBottom,
  shouldFollowThreadHistoryBottom,
  threadHasOlderHistory,
} from "@/components/agent-chat-panel/runtime/threadHistoryScroll";
import { useAgentStore, type AgentMessage, type AgentThread } from "@/lib/agentStore";
import { fetchAgentTasks, type AgentQueueTask } from "@/lib/agentTaskQueue";
import { ThreadFilePreviewOverlay } from "./ThreadFilePreviewOverlay";
import { ThreadComposer } from "./ThreadComposer";
import { ThreadCompactSessionBar } from "./ThreadCompactSessionBar";
import { ThreadActivityRow } from "./ThreadActivityRow";
import { classifyThreadActivityMessage } from "./threadActivityModel";
import { endThreadCompaction, messageTimeMs, useThreadCompaction } from "./threadCompactionStatus";
import { isCompactionArtifactMessage } from "@/components/agent-chat-panel/chat-view/compactionArtifact";
import { ThreadHandoffControl } from "./ThreadHandoffControl";
import { ThreadParticipantsDrawer } from "./ThreadParticipantsDrawer";
import { buildThreadAgentOptions } from "./threadHandoffModel";
import { BUILTIN_WORKSPACE_PERSONAS } from "../workspaces/workspaceActorPicker";
import { useThreadSpeech } from "./useThreadSpeech";
import {
  NativeThreadMessageBubble,
  ThreadMessageActionsProvider,
  shouldOfferMessageRetry,
  type ThreadMessageActions,
} from "./NativeThreadMessageBubble";
import { ThreadRetryStatusBanner } from "./ThreadRetryStatusBanner";
import { useThreadRetryStatus } from "./threadRetryStatus";
import { resolveThreadOwnerRuntimeProfile } from "./threadOwnerRuntime";
import type { ZoraiReturnTarget } from "../../shell/zoraiNavigationEvents";
import { isThreadLoading, shouldShowConversationSkeleton, useThreadLoadingStore } from "./threadLoadingStore";
import { threadReadKey, useThreadReadStateStore } from "./threadReadStateStore";
import { DelegatedSessionSlot } from "./DelegatedSessionBar";
import { ThreadSessionTabs } from "./SessionTabStrip";

export { ThreadsRail } from "./ThreadsRail";

function ThreadConversationSkeleton({ embedded = false }: { embedded?: boolean }) {
  return (
    <div className={["zorai-thread-conversation-loader", embedded ? "zorai-thread-conversation-loader--embedded" : ""].filter(Boolean).join(" ")} role="status" aria-label="Loading conversation">
      <LoadingState size={16} label="Loading conversation" />
      <div className="zorai-thread-conversation-loader__messages" aria-hidden="true">
        <div className="zorai-thread-message-skeleton zorai-thread-message-skeleton--assistant"><LoadingState variant="skeleton" lines={3} /></div>
        <div className="zorai-thread-message-skeleton zorai-thread-message-skeleton--user"><LoadingState variant="skeleton" lines={2} /></div>
        <div className="zorai-thread-message-skeleton zorai-thread-message-skeleton--assistant zorai-thread-message-skeleton--short"><LoadingState variant="skeleton" lines={2} /></div>
      </div>
    </div>
  );
}

export function ThreadsView({
  returnTarget = null,
  onReturnTarget,
  variant = "full",
  compactHeaderActions = null,
}: {
  returnTarget?: ZoraiReturnTarget | null;
  onReturnTarget?: () => void;
  variant?: "full" | "compact";
  compactHeaderActions?: ReactNode;
} = {}) {
  const threadLoadingPending = useThreadLoadingStore((state) => state.pending);
  const threadLoadingByThreadId = useThreadLoadingStore((state) => state.byThreadId);
  const runtime = useAgentChatPanelRuntime();
  const [pinLimitResult, setPinLimitResult] = useState<ZoraiThreadMessagePinResult | null>(null);
  const [participantsOpen, setParticipantsOpen] = useState(false);
  const [pinnedToBottom, setPinnedToBottom] = useState(true);
  const [readTasks, setReadTasks] = useState<AgentQueueTask[]>([]);
  const subAgents = useAgentStore((state) => state.subAgents);
  const messageActionsRef = useRef<ThreadMessageActions | null>(null);
  const viewMountedAtRef = useRef(Date.now());
  const scrollerRef = useRef<HTMLDivElement | null>(null);
  const displayItems = useMemo(() => buildDisplayItems(runtime.messages), [runtime.messages]);
  const lastDisplayItem = displayItems[displayItems.length - 1];
  const streamingContinuesAttributedToolSet = lastDisplayItem?.type === "toolList"
    && Boolean(lastDisplayItem.attribution);
  const activeOperations = useMemo(() => runtime.messages.flatMap((message) => {
    const activity = classifyThreadActivityMessage(message);
    return activity?.kind === "operation"
      ? activity.operations.filter((operation) => operation.state === "accepted" || operation.state === "started")
      : [];
  }), [runtime.messages]);
  const threadEnds = useMemo(() => latestThreadEnds(runtime.messages), [runtime.messages]);
  const latestUserMessage = threadEnds.latestUserMessage;
  const latestAssistantMessageId = threadEnds.latestAssistantMessageId;
  const regenerateAssistantMessage = useCallback((messageId: string) => {
    const index = runtime.messages.findIndex((entry) => entry.id === messageId);
    if (index <= 0) return;
    const previousUserMessage = runtime.messages
      .slice(0, index)
      .reverse()
      .find((entry) => entry.role === "user" && entry.content.trim());
    if (!previousUserMessage) return;
    runtime.sendMessage({
      text: previousUserMessage.content,
      localContentBlocks: previousUserMessage.contentBlocks,
    });
  }, [runtime.messages, runtime.sendMessage]);
  const retryLastMessage = useCallback(() => {
    if (!latestUserMessage) return;
    runtime.sendMessage({
      text: latestUserMessage.content,
      localContentBlocks: latestUserMessage.contentBlocks,
    });
  }, [latestUserMessage, runtime.sendMessage]);
  const speech = useThreadSpeech(runtime.messages);
  const retryStatus = useThreadRetryStatus(
    runtime.activeThread?.daemonThreadId,
    runtime.activeThread?.id,
  );
  const compactionStatus = useThreadCompaction(runtime.activeThread?.daemonThreadId ?? runtime.activeThread?.id);
  useEffect(() => {
    if (!compactionStatus) return;
    const arrived = runtime.messages.some((message) => (
      isCompactionArtifactMessage(message)
      && messageTimeMs(message.createdAt) >= compactionStatus.startedAt - 5_000
    ));
    if (arrived) endThreadCompaction(compactionStatus.daemonThreadId);
  }, [compactionStatus, runtime.messages]);

  // Global shortcuts for the thread surface:
  //   Ctrl+L — speak/stop the latest assistant message
  useEffect(() => {
    const onKeyDown = (event: globalThis.KeyboardEvent) => {
      if (!event.ctrlKey || event.shiftKey || event.metaKey || event.altKey) return;
      if (event.code !== "KeyL") return;
      const target = event.target as HTMLElement | null;
      if (target && (target.tagName === "TEXTAREA" || target.tagName === "INPUT" || target.isContentEditable)) {
        return;
      }
      if (!speech.ttsEnabled) return;
      event.preventDefault();
      speech.speakLatestAssistantMessage();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [speech]);

  const activeThreadId = runtime.activeThread?.id ?? null;
  const activeReadThread = runtime.activeThread;
  const activeReadThreadId = activeReadThread?.id ?? null;
  const activeReadDaemonThreadId = activeReadThread?.daemonThreadId ?? null;
  const activeReadMessages = runtime.messages;

  useEffect(() => {
    if (!activeReadThreadId) {
      setReadTasks([]);
      return;
    }
    let active = true;
    void fetchAgentTasks().then((tasks) => {
      if (active) setReadTasks(tasks);
    });
    return () => { active = false; };
  }, [activeReadDaemonThreadId, activeReadThreadId]);

  useEffect(() => {
    if (!activeReadThread) return;
    const store = useThreadReadStateStore.getState();
    const localKey = `local:${activeReadThread.id}`;
    const daemonKey = activeReadThread.daemonThreadId?.trim()
      ? `daemon:${activeReadThread.daemonThreadId.trim()}`
      : null;
    if (daemonKey) store.migrateThreadKey(localKey, daemonKey);
    const key = threadReadKey(activeReadThread);
    const newestDisplayedAt = activeReadMessages.reduce(
      (latest, message) => Math.max(latest, message.createdAt ?? 0),
      0,
    );
    const identities = new Set([activeReadThread.id, activeReadThread.daemonThreadId].filter(Boolean));
    const newestTaskCompletionAt = readTasks.reduce((latest, task) => {
      const matches = [task.thread_id, task.parent_thread_id].some((id) => id && identities.has(id));
      const terminal = ["completed", "failed", "cancelled", "budget_exceeded"].includes(task.status);
      return matches && terminal ? Math.max(latest, task.completed_at ?? 0) : latest;
    }, 0);
    const newestGoalCompletionAt = runtime.goalRunsForTrace.reduce((latest, goal) => {
      const goalThreads = [goal.thread_id, goal.root_thread_id, goal.active_thread_id, ...(goal.execution_thread_ids ?? [])];
      const matches = goalThreads.some((id) => id && identities.has(id));
      const terminal = ["completed", "failed", "cancelled"].includes(goal.status);
      return matches && terminal ? Math.max(latest, goal.completed_at ?? 0) : latest;
    }, 0);
    const newestReadAt = Math.max(newestDisplayedAt, newestTaskCompletionAt, newestGoalCompletionAt);
    if (!key || newestReadAt <= 0) return;
    const frame = requestAnimationFrame(() => {
      useThreadReadStateStore.getState().markRead(key, newestReadAt);
    });
    return () => cancelAnimationFrame(frame);
  }, [activeReadMessages, activeReadThread, readTasks, runtime.goalRunsForTrace]);

  useEffect(() => {
    resetThreadHistoryPagination();
    setFollowThreadHistoryBottom(true);
    setPinnedToBottom(true);
  }, [activeThreadId]);

  useEffect(() => {
    const scroller = scrollerRef.current;
    if (!scroller || !activeThreadId || !shouldFollowThreadHistoryBottom()) return;

    beginProgrammaticThreadHistoryScroll();
    scroller.scrollTop = scroller.scrollHeight;
    endProgrammaticThreadHistoryScroll();
  }, [activeThreadId, runtime.messages]);

  const activeThreadLoading = runtime.activeThread
    ? isThreadLoading(threadLoadingByThreadId, runtime.activeThread.id, runtime.activeThread.daemonThreadId)
    : false;
  const showConversationSkeleton = shouldShowConversationSkeleton({
    pending: threadLoadingPending,
    activeThreadLoading,
    hasActiveThread: Boolean(runtime.activeThread),
    loadedMessageCount: runtime.messages.length,
    knownHistory: Boolean(
      runtime.activeThread
      && (runtime.activeThread.messageCount > 0 || runtime.activeThread.lastMessagePreview),
    ),
  });

  useEffect(() => {
    if (showConversationSkeleton || threadLoadingPending > 0) return;
    const frame = requestAnimationFrame(() => {
      fillThreadHistoryIfUnscrollable({
        scroller: scrollerRef.current,
        loadOlder: runtime.loadOlderThreadMessages,
        hasOlderHistory: threadHasOlderHistory(runtime.activeThread, runtime.messages.length),
      });
    });
    return () => cancelAnimationFrame(frame);
  }, [
    showConversationSkeleton,
    threadLoadingPending,
    runtime.activeThread,
    runtime.loadOlderThreadMessages,
  ]);

  if (showConversationSkeleton && !runtime.activeThread) {
    return <ThreadConversationSkeleton />;
  }

  if (!runtime.activeThread) {
    return (
      <div className="zorai-empty-main">
        <div className="zorai-empty-kicker">Zorai</div>
        <h1>Start with a thread.</h1>
        <p>
          Threads are the default Zorai surface. Create a conversation, bring in an agent,
          then promote durable work into goals or workspace cards when needed.
        </p>
        <button
          type="button"
          className="zorai-primary-button"
          onClick={() => runtime.createThread({ workspaceId: runtime.activeWorkspace?.id ?? null })}
        >
          + New Thread
        </button>
      </div>
    );
  }

  const activeThread = runtime.activeThread;
  const activeHandoffAgentId = activeThread.threadHandoffState?.activeAgentId
    ?? activeThread.agent_name;
  const agentOptions = buildThreadAgentOptions(
    [
      { id: "swarog", name: "Svarog" },
      { id: "rarog", name: "Rarog" },
      ...BUILTIN_WORKSPACE_PERSONAS.map((persona) => ({ id: persona.id, name: persona.label })),
    ],
    subAgents.filter((agent) => agent.enabled).map((agent) => ({ id: agent.id, name: agent.name })),
    activeHandoffAgentId,
  );

  const handleThreadScroll = (event: UIEvent<HTMLDivElement>) => {
    consumeThreadHistoryScroll({
      scroller: event.currentTarget,
      loadOlder: runtime.loadOlderThreadMessages,
      hasOlderHistory: threadHasOlderHistory(runtime.activeThread, runtime.messages.length),
      onFollowBottomChange: setPinnedToBottom,
    });
  };
  const scrollThreadToLatest = () => {
    const scroller = scrollerRef.current;
    if (!scroller) return;
    setFollowThreadHistoryBottom(true);
    setPinnedToBottom(true);
    beginProgrammaticThreadHistoryScroll();
    scroller.scrollTop = scroller.scrollHeight;
    endProgrammaticThreadHistoryScroll();
  };
  const profile = resolveThreadOwnerRuntimeProfile(activeThread, subAgents, useAgentStore.getState().agentSettings, useAgentStore.getState().conciergeConfig);
  const activeThreadIdForActions = runtime.activeThread?.id;
  messageActionsRef.current = {
    speak: (message) => { void speech.speakMessage(message); },
    feedback: (threadId, messageId, reaction) => runtime.submitMessageFeedback(activeThreadIdForActions ?? threadId, messageId, reaction),
    regenerate: regenerateAssistantMessage,
    fork: (messageId) => runtime.forkThread(messageId),
    deleteMessage: (threadId, messageId) => runtime.deleteMessage(activeThreadIdForActions ?? threadId, messageId),
    retry: retryLastMessage,
    pin: async (threadId, messageId) => {
      const result = await runtime.pinMessageForCompaction(activeThreadIdForActions ?? threadId, messageId);
      if (result && result.ok === false && result.error === "pinned_budget_exceeded") setPinLimitResult(result);
    },
    unpin: (threadId, messageId) => { void runtime.unpinMessageForCompaction(activeThreadIdForActions ?? threadId, messageId); },
  };
  return (
    <section className={["zorai-thread-surface", "zorai-native-thread-surface", variant === "compact" ? "zorai-thread-surface--compact" : ""].filter(Boolean).join(" ")}>
      <ThreadSessionTabs />
      {variant === "full" ? (
        <>
          <ThreadHeader
        thread={runtime.activeThread}
        messageCount={runtime.messages.length}
        agentOptions={agentOptions}
        returnTarget={returnTarget}
        onReturnTarget={onReturnTarget}
        onPushHandoff={runtime.pushHandoff}
        onReturnHandoff={runtime.returnHandoff}
        onOpenParticipants={() => setParticipantsOpen(true)}
        activeOperationCount={activeOperations.length}
            onOpenOperations={() => {
              const latest = activeOperations[activeOperations.length - 1];
              if (latest && latest.operationId !== "unknown") {
                document.getElementById(`zorai-operation-${latest.operationId}`)?.scrollIntoView({ block: "center" });
              }
            }}
          />
          <ParticipantStrip thread={runtime.activeThread} onOpen={() => setParticipantsOpen(true)} />
        </>
      ) : (
        <header className="zorai-code-agent-thread-header">
          {runtime.canGoBackThread ? (
            <button
              type="button"
              className="zorai-code-agent-thread-back"
              title={runtime.backThreadTitle ? `Back to ${runtime.backThreadTitle}` : "Back to parent"}
              onClick={runtime.goBackThread}
            >
              <span aria-hidden="true">←</span>
              <span>{runtime.backThreadTitle ? `Back to ${runtime.backThreadTitle}` : "Back to parent"}</span>
            </button>
          ) : null}
          <div className="zorai-code-agent-thread-header__meta">
            <strong>{activeThread.title}</strong>
            <span>Responder · {actualThreadResponderLabel(activeThread)} · {profile.model} · {profile.provider}</span>
          </div>
          <div className="zorai-code-agent-thread-actions">
            {compactHeaderActions}
          </div>
        </header>
      )}

      <div className="zorai-thread-chat">
        <DelegatedSessionSlot />
        <div ref={scrollerRef} className="zorai-thread-chat-scroll" onScroll={(event) => void handleThreadScroll(event)}>
        {showConversationSkeleton ? (
          <ThreadConversationSkeleton embedded />
        ) : runtime.messages.length === 0 ? (
          <div className="zorai-thread-empty-state">
            <div className="zorai-brand-mark"><span>Z</span></div>
            <strong>Start a Zorai thread</strong>
            <span>Ask for a plan, delegate work, or turn a request into a goal.</span>
          </div>
        ) : (
        <ThreadMessageActionsProvider actionsRef={messageActionsRef}>
        {displayItems.map((item) => {
          if (item.type === "toolList") {
            return (
              <ToolEventList
                key={item.key}
                groups={item.groups}
                attribution={item.attribution}
                fallbackAuthorName={runtime.activeThread?.agent_name}
              />
            );
          }
          if (item.type === "tool") {
            return <MemoizedToolEventRow key={`tool_${item.group.key}`} group={item.group} />;
          }

          const message = item.message;
          const activity = classifyThreadActivityMessage(message);
          if (activity) {
            return (
              <ThreadActivityRow
                key={message.id}
                activity={activity}
                createdAt={message.createdAt}
                onRefreshOperation={runtime.getOperationStatus}
                onCancelOperation={runtime.cancelOperation}
              />
            );
          }

          return (
            <NativeThreadMessageBubble
              key={message.id}
              message={message}
              threadAgentName={runtime.activeThread?.agent_name}
              ttsEnabled={speech.ttsEnabled}
              speaking={speech.speakingMessageId === message.id}
              speechLoading={speech.loadingMessageId === message.id}
              speechQueued={speech.queuedMessageIds.includes(message.id)}
              offerRetry={shouldOfferMessageRetry(
                message,
                latestAssistantMessageId,
                viewMountedAtRef.current,
                Boolean(latestUserMessage),
              )}
            />
          );
        })}
        </ThreadMessageActionsProvider>
        )}
        {retryStatus ? (
          <ThreadRetryStatusBanner
            status={retryStatus}
            onRetryNow={() => runtime.retryStreamNow(runtime.activeThreadId)}
            onStop={() => runtime.stopStreaming(runtime.activeThreadId, retryStatus.daemonThreadId)}
          />
        ) : null}
        {compactionStatus ? (
          <ThinkingIndicator label="Compaction" />
        ) : runtime.isStreamingResponse ? (
          <ThinkingIndicator
            label={runtime.activeThread.agent_name}
            continueAttributedTurn={streamingContinuesAttributedToolSet}
          />
        ) : null}
        <div ref={runtime.messagesEndRef} />
        </div>
        <ThreadScrollToBottomButton hidden={pinnedToBottom} onClick={scrollThreadToLatest} />
      </div>

      <div className="zorai-thread-composer-stack">
        {variant === "compact" ? <ThreadCompactSessionBar /> : null}
        <ThreadComposer showTargetSelector={variant === "compact"} compact={variant === "compact"} />
      </div>

      {variant === "full" && pinLimitResult ? (
        <PinLimitModal result={pinLimitResult} onClose={() => setPinLimitResult(null)} />
      ) : null}
      {variant === "full" && participantsOpen ? (
        <ThreadParticipantsDrawer
          thread={runtime.activeThread}
          agentOptions={agentOptions}
          onClose={() => setParticipantsOpen(false)}
          onUpsert={runtime.upsertParticipant}
          onDeactivate={runtime.deactivateParticipant}
          onSendSuggestion={runtime.sendParticipantSuggestion}
          onDismissSuggestion={runtime.dismissParticipantSuggestion}
        />
      ) : null}
      <ThreadFilePreviewOverlay />
    </section>
  );
}

function latestThreadEnds(messages: AgentMessage[]): {
  latestUserMessage: AgentMessage | undefined;
  latestAssistantMessageId: string | undefined;
} {
  let latestUserMessage: AgentMessage | undefined;
  let latestAssistantMessageId: string | undefined;
  for (let index = messages.length - 1; index >= 0; index -= 1) {
    const message = messages[index];
    if (!latestAssistantMessageId && message.role === "assistant") {
      latestAssistantMessageId = message.id;
    }
    if (!latestUserMessage && message.role === "user" && message.content.trim()) {
      latestUserMessage = message;
    }
    if (latestUserMessage && latestAssistantMessageId) break;
  }
  return { latestUserMessage, latestAssistantMessageId };
}

function actualThreadResponderLabel(thread: AgentThread): string {
  const stack = thread.threadHandoffState?.responderStack ?? [];
  return stack[stack.length - 1]?.agentName ?? thread.agent_name;
}

function ThreadHeader({
  thread,
  messageCount,
  agentOptions,
  returnTarget,
  onReturnTarget,
  onPushHandoff,
  onReturnHandoff,
  onOpenParticipants,
  activeOperationCount,
  onOpenOperations,
}: {
  thread: AgentThread;
  messageCount: number;
  agentOptions: ReturnType<typeof buildThreadAgentOptions>;
  returnTarget?: ZoraiReturnTarget | null;
  onReturnTarget?: () => void;
  onPushHandoff: ReturnType<typeof useAgentChatPanelRuntime>["pushHandoff"];
  onReturnHandoff: ReturnType<typeof useAgentChatPanelRuntime>["returnHandoff"];
  onOpenParticipants: () => void;
  activeOperationCount: number;
  onOpenOperations: () => void;
}) {
  const participants = thread.threadParticipants ?? [];
  const activeParticipants = participants.filter((participant) => participant.status === "active");
  const responderStack = thread.threadHandoffState?.responderStack ?? [];
  const activeResponder = responderStack[responderStack.length - 1]?.agentName
    ?? thread.agent_name;

  const agentSettings = useAgentStore((state) => state.agentSettings);
  const conciergeConfig = useAgentStore((state) => state.conciergeConfig);
  const subAgents = useAgentStore((state) => state.subAgents);
  const profile = resolveThreadOwnerRuntimeProfile(thread, subAgents, agentSettings, conciergeConfig);
  const provider = profile.provider || "default provider";
  const model = profile.model || "default model";
  const effort = profile.effort || "default effort";
  const context = profile.contextWindowTokens.toLocaleString();
  return (
    <header className="zorai-thread-header">
      <div>
        <h3>{thread.title}</h3>
        <span>{messageCount} messages / {activeResponder} · {provider} · {model} · effort: {effort} · context: {context} tokens</span>
      </div>
      <div className="zorai-thread-header__actions">
        {returnTarget && onReturnTarget ? (
          <button type="button" className="zorai-ghost-button" onClick={onReturnTarget}>
            {returnTarget.label}
          </button>
        ) : null}
        <ThreadHandoffControl
          daemonLinked={Boolean(thread.daemonThreadId)}
          handoffState={thread.threadHandoffState}
          options={agentOptions}
          onPush={onPushHandoff}
          onReturn={onReturnHandoff}
        />
        <button type="button" className="zorai-ghost-button" onClick={onOpenParticipants}>
          Participants {activeParticipants.length}
        </button>
        <button
          type="button"
          className="zorai-ghost-button"
          disabled={activeOperationCount === 0}
          onClick={onOpenOperations}
        >
          Operations {activeOperationCount}
        </button>
      </div>
    </header>
  );
}

function ThinkingIndicator({
  label,
  continueAttributedTurn = false,
}: {
  label: string;
  continueAttributedTurn?: boolean;
}) {
  return (
    <div
      className={["zorai-thinking", continueAttributedTurn ? "zorai-thinking--continued" : ""].filter(Boolean).join(" ")}
      role="status"
      aria-live="polite"
      aria-label={continueAttributedTurn ? `${label} is still working` : `${label} in progress`}
    >
      <div className="zorai-thinking__body">
        <div className="zorai-thinking__label">
          {continueAttributedTurn ? null : <strong>{label}</strong>}
          <span className="zorai-thinking__dots" aria-hidden="true">
            <span />
            <span />
            <span />
          </span>
        </div>
      </div>
    </div>
  );
}

function ThreadScrollToBottomButton({ hidden, onClick }: { hidden: boolean; onClick: () => void }) {
  return (
    <button
      type="button"
      className={["zorai-thread-scroll-bottom", hidden ? "is-hidden" : ""].filter(Boolean).join(" ")}
      aria-hidden={hidden}
      aria-label="Scroll to latest messages"
      tabIndex={hidden ? -1 : 0}
      onClick={onClick}
    >
      <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false">
        <path d="M6 10l6 6 6-6" />
      </svg>
    </button>
  );
}

function ParticipantStrip({ thread, onOpen }: { thread: AgentThread; onOpen: () => void }) {
  const participants = thread.threadParticipants ?? [];
  const queued = thread.queuedParticipantSuggestions ?? [];
  if (participants.length === 0 && queued.length === 0) return null;

  return (
    <button type="button" className="zorai-thread-participants" onClick={onOpen}>
      {participants.map((participant) => (
        <span key={participant.agentId} className="zorai-status-pill">
          {participant.agentName} / {participant.status}
        </span>
      ))}
      {queued.map((suggestion) => (
        <span key={suggestion.id} className="zorai-status-pill">
          queued: {suggestion.targetAgentName}
        </span>
      ))}
      <span className="zorai-thread-participants__action">Manage</span>
    </button>
  );
}

function PinLimitModal({
  result,
  onClose,
}: {
  result: ZoraiThreadMessagePinResult;
  onClose: () => void;
}) {
  const attempted = Math.max(0, (result.candidate_pinned_chars ?? 0) - result.current_pinned_chars);

  return (
    <div className="zorai-pin-limit-overlay" role="presentation">
      <section className="zorai-pin-limit-dialog" role="dialog" aria-modal="true" aria-labelledby="zorai-pin-limit-title">
        <div className="zorai-section-label">Pin Limit Reached</div>
        <h2 id="zorai-pin-limit-title">This message cannot be pinned for compaction.</h2>
        <p>
          Pinned messages are injected after the owner compaction artifact and are capped
          at 25% of the active model context window.
        </p>
        <div className="zorai-pin-limit-stats">
          <span>Current pinned chars: {result.current_pinned_chars.toLocaleString()}</span>
          <span>Pinned budget chars: {result.pinned_budget_chars.toLocaleString()}</span>
          <span>Attempted total chars: {(result.candidate_pinned_chars ?? 0).toLocaleString()}</span>
          <span>Attempted message size: {attempted.toLocaleString()}</span>
        </div>
        <div className="zorai-card-actions">
          <button type="button" className="zorai-primary-button" onClick={onClose}>Close</button>
        </div>
      </section>
    </div>
  );
}
