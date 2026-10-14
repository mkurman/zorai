import type { CSSProperties } from "react";
import { useAgentMissionStore } from "@/lib/agentMissionStore";
import { useWorkspaceStore } from "@/lib/workspaceStore";
import type { AgentSettings } from "@/lib/agentStore";
import { AgentExecutionGraph } from "@/components/AgentExecutionGraph";
import { AITrainingView } from "../AITrainingView";
import { BuiltinAgentSetupModal } from "../BuiltinAgentSetupModal";
import { ChatView } from "../ChatView";
import { CodingAgentsView } from "../CodingAgentsView";
import { ContextView } from "../ContextView";
import { PinnedMessagesView } from "../PinnedMessagesView";
import { SpawnedAgentsPanel } from "../SpawnedAgentsPanel";
import { SubagentsView } from "../SubagentsView";
import { TasksView } from "../TasksView";
import { MetricRibbon, SectionTitle, iconButtonStyle } from "../shared";
import { ThreadList } from "../ThreadList";
import { TraceView } from "../TraceView";
import { UsageView } from "../UsageView";
import { useAgentChatPanelRuntime } from "./context";

export function AgentChatPanelScaffold({ style, className }: { style?: CSSProperties; className?: string }) {
  return (
    <div
      style={{
        width: 560,
        minWidth: 380,
        maxWidth: 820,
        height: "100%",
        display: "flex",
        flexDirection: "column",
        background: "var(--zorai-bg)",
        border: "1px solid var(--border)",
        borderRadius: "var(--radius-xl)",
        overflow: "hidden",
        ...(style ?? {}),
      }}
      className={className}
    >
      <AgentChatPanelTabs />
      <AgentChatPanelHeader />
      <div style={{ flex: 1, overflow: "hidden", position: "relative", display: "flex", flexDirection: "column" }}>
        <AgentChatPanelCurrentSurface />
        <BuiltinAgentSetupModal />
      </div>
    </div>
  );
}

export function AgentChatPanelHeader() {
  const runtime = useAgentChatPanelRuntime();
  const {
    view,
    activeThread,
    setActiveThread,
    setView,
    chatBackView,
    setChatBackView,
    togglePanel,
    createThread,
    threadNavigationDepth,
    backThreadTitle,
  } = runtime;
  const activeParticipants = activeThread?.threadParticipants?.filter((participant) => participant.status === "active") ?? [];
  const inactiveParticipants = activeThread?.threadParticipants?.filter((participant) => participant.status === "inactive") ?? [];

  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        padding: "var(--space-4)",
        paddingBottom: "var(--space-2)",
        borderBottom: "1px solid var(--border)",
        flexShrink: 0,
        background: "var(--bg-secondary)",
        gap: "var(--space-2)",
      }}
    >
      <div style={{ display: "flex", alignItems: "flex-start", justifyContent: "space-between", gap: "var(--space-3)" }}>
        <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-1)" }}>
          <div style={{ display: "flex", alignItems: "center", gap: "var(--space-2)" }}>
            {view === "chat" && activeThread && (
              <button
                onClick={() => {
                  setActiveThread(null);
                  setView(chatBackView);
                }}
                style={iconButtonStyle}
                title="Back to threads"
              >
                ←
              </button>
            )}
          </div>
        </div>

        <div style={{ display: "flex", gap: "var(--space-2)" }}>
          <button
            onClick={() => {
              const workspaceId = useWorkspaceStore.getState().activeWorkspaceId;
              createThread({ workspaceId });
              setChatBackView("threads");
              setView("chat");
            }}
            style={{ ...iconButtonStyle, minWidth: 120, color: "var(--success)", borderColor: "var(--success-soft)" }}
            title="New thread"
          >
            + New session
          </button>
          <button onClick={togglePanel} style={{ ...iconButtonStyle, minWidth: 40 }} title="Close">
            ☰
          </button>
        </div>
      </div>

      <div style={{ justifyContent: "center", display: "flex", alignItems: "center" }}>
        <div style={{ display: "grid", gap: 2, justifyItems: "center" }}>
          <span style={{ fontSize: "var(--text-md)", fontWeight: 700 }}>
            {view === "threads" ? "Live Intelligence Surfaces" : activeThread?.title ?? "Conversation Lane"}
          </span>
          {view === "chat" && threadNavigationDepth > 0 && (
            <span style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)" }}>
              {backThreadTitle
                ? `${threadNavigationDepth} hop return path to ${backThreadTitle}`
                : `${threadNavigationDepth} hop return path`}
            </span>
          )}
        </div>
      </div>

      {view === "chat" && activeThread && (activeParticipants.length > 0 || inactiveParticipants.length > 0) && (
        <div style={{ justifyContent: "center", display: "flex", alignItems: "center", gap: "var(--space-2)", flexWrap: "wrap" }}>
          {activeParticipants.map((participant) => (
            <span
              key={`${participant.agentId}:active`}
              title={participant.instruction}
              style={{ fontSize: "var(--text-sm)", border: "1px solid var(--accent)", color: "var(--accent)", borderRadius: 999, padding: "2px 8px", background: "color-mix(in srgb, var(--accent) 12%, transparent)" }}
            >
              {participant.agentName}
            </span>
          ))}
          {inactiveParticipants.map((participant) => (
            <span
              key={`${participant.agentId}:inactive`}
              title={participant.instruction}
              style={{ fontSize: "var(--text-sm)", border: "1px solid var(--glass-border)", color: "var(--text-muted)", borderRadius: 999, padding: "2px 8px" }}
            >
              {participant.agentName} · inactive
            </span>
          ))}
        </div>
      )}
    </div>
  );
}

export function AgentChatPanelTabs() {
  const { tabItems, view, setView } = useAgentChatPanelRuntime();

  return (
    <div
      style={{
        display: "flex",
        gap: "var(--space-1)",
        padding: "var(--space-2) var(--space-3)",
        borderBottom: "1px solid var(--border)",
        background: "var(--bg-secondary)",
        overflowX: "auto",
      }}
    >
      {tabItems.map((tab) => (
        <button
          key={tab.id}
          type="button"
          onClick={() => setView(tab.id)}
          style={{
            padding: "var(--space-1) var(--space-3)",
            borderRadius: "var(--radius-full)",
            border: "1px solid",
            borderColor: view === tab.id ? "var(--accent-soft)" : "transparent",
            background: view === tab.id ? "var(--accent-soft)" : "transparent",
            color: view === tab.id ? "var(--accent)" : "var(--text-muted)",
            fontSize: "var(--text-xs)",
            fontWeight: 500,
            cursor: "pointer",
            whiteSpace: "nowrap",
            transition: "all var(--transition-fast)",
          }}
        >
          {tab.label}
          {tab.count !== null && <span style={{ marginLeft: "var(--space-1)", opacity: 0.7 }}>{tab.count}</span>}
        </button>
      ))}
    </div>
  );
}

export function AgentChatPanelCurrentSurface() {
  const { view, setView, setChatBackView } = useAgentChatPanelRuntime();

  if (view === "threads") return <AgentChatPanelThreadsSurface />;
  if (view === "internal") return <AgentChatPanelInternalSurface />;
  if (view === "gateway") return <AgentChatPanelGatewaySurface />;
  if (view === "chat") return <AgentChatPanelChatSurface />;
  if (view === "pinned") return <AgentChatPanelPinnedSurface />;
  if (view === "trace") return <AgentChatPanelTraceSurface />;
  if (view === "usage") return <AgentChatPanelUsageSurface />;
  if (view === "context") return <AgentChatPanelContextSurface />;
  if (view === "coding-agents") return <AgentChatPanelCodingAgentsSurface />;
  if (view === "ai-training") return <AgentChatPanelAITrainingSurface />;
  if (view === "tasks") return <TasksView onOpenThreadView={() => { setChatBackView("tasks"); setView("chat"); }} />;
  if (view === "subagents") return <SubagentsView onOpenThreadView={() => { setChatBackView("subagents"); setView("chat"); }} onOpenTasksView={() => setView("tasks")} />;
  return <AgentChatPanelGraphSurface />;
}

function AgentChatPanelThreadBrowserSurface({
  browserView,
}: {
  browserView: "threads" | "internal" | "gateway";
}) {
  const { filteredThreads, searchQuery, setSearchQuery, refreshThreadList, openThread, setView, setChatBackView, deleteThread, activeThreadId } = useAgentChatPanelRuntime();

  return (
    <ThreadList
      threads={filteredThreads}
      searchQuery={searchQuery}
      onSearch={setSearchQuery}
      onRefresh={() => void refreshThreadList()}
      onSelect={(thread) => {
        openThread(thread.id);
        setChatBackView(browserView);
        setView("chat");
      }}
      onDelete={deleteThread}
      activeThreadId={activeThreadId}
    />
  );
}

export function AgentChatPanelThreadsSurface() {
  return <AgentChatPanelThreadBrowserSurface browserView="threads" />;
}

export function AgentChatPanelInternalSurface() {
  return <AgentChatPanelThreadBrowserSurface browserView="internal" />;
}

export function AgentChatPanelGatewaySurface() {
  return <AgentChatPanelThreadBrowserSurface browserView="gateway" />;
}

export function AgentChatPanelChatSurface() {
  const runtime = useAgentChatPanelRuntime();
  return (
    <div
      style={{
        flex: 1,
        minHeight: 0,
        display: "flex",
        gap: "var(--space-3)",
        padding: "var(--space-3)",
      }}
    >
      <div
        style={{
          flex: 1,
          minWidth: 0,
          minHeight: 0,
          display: "flex",
          flexDirection: "column",
          border: "1px solid var(--border)",
          borderRadius: "var(--radius-xl)",
          overflow: "hidden",
          background: "var(--zorai-bg)",
        }}
      >
        <ChatView
          messages={runtime.messages}
          todos={runtime.todos}
          inputRef={runtime.inputRef}
          onKeyDown={runtime.handleKeyDown}
          agentSettings={runtime.agentSettings}
          isStreamingResponse={runtime.isStreamingResponse}
          activeThread={runtime.activeThread}
          messagesEndRef={runtime.messagesEndRef}
          onLoadOlderMessages={runtime.loadOlderThreadMessages}
          onTrimMessagesToLatestWindow={() => runtime.trimThreadMessagesToLatestWindow(runtime.activeThreadId)}
          onSendMessage={runtime.sendMessage}
          onSendParticipantSuggestion={runtime.sendParticipantSuggestion}
          onDismissParticipantSuggestion={runtime.dismissParticipantSuggestion}
          onStopStreaming={() => runtime.stopStreaming(runtime.activeThreadId)}
          onDeleteMessage={(messageId) => {
            const tid = runtime.activeThreadId;
            if (tid) runtime.deleteMessage(tid, messageId);
          }}
          onForkMessage={(messageId) => { void runtime.forkThread(messageId); }}
          onExportThread={(messageId) => { void runtime.exportThread(messageId); }}
          onPinMessage={runtime.activeThreadId ? (messageId) => runtime.pinMessageForCompaction(runtime.activeThreadId as string, messageId) : undefined}
          onUnpinMessage={runtime.activeThreadId ? (messageId) => runtime.unpinMessageForCompaction(runtime.activeThreadId as string, messageId) : undefined}
          onFeedbackMessage={runtime.activeThreadId ? (messageId, reaction) => runtime.submitMessageFeedback(runtime.activeThreadId as string, messageId, reaction) : undefined}
          onUpdateReasoningEffort={(value) => runtime.updateAgentSetting("reasoning_effort", value as AgentSettings["reasoning_effort"])}
          canStartGoalRun={runtime.canStartGoalRun}
          onStartGoalRun={runtime.startGoalRunFromPrompt}
          welesHealth={runtime.welesHealth}
        />
      </div>

      <SpawnedAgentsPanel
        tree={runtime.spawnedAgentTree}
        selectedDaemonThreadId={runtime.activeThread?.daemonThreadId ?? null}
        canGoBackThread={runtime.canGoBackThread}
        threadNavigationDepth={runtime.threadNavigationDepth}
        backThreadTitle={runtime.backThreadTitle}
        canOpenSpawnedThread={runtime.canOpenSpawnedThread}
        openSpawnedThread={runtime.openSpawnedThread}
        goBackThread={runtime.goBackThread}
      />
    </div>
  );
}

export function AgentChatPanelPinnedSurface() {
  const runtime = useAgentChatPanelRuntime();
  return (
    <PinnedMessagesView
      messages={runtime.pinnedMessages}
      pinnedUsageChars={runtime.pinnedUsageChars}
      pinnedBudgetChars={runtime.pinnedBudgetChars}
      pinnedOverBudget={runtime.pinnedOverBudget}
      onJumpToMessage={(messageId) => {
        runtime.setChatBackView("pinned");
        runtime.setView("chat");
        window.setTimeout(() => {
          document.getElementById(`agent-message-${messageId}`)?.scrollIntoView({
            behavior: "smooth",
            block: "center",
          });
        }, 50);
      }}
      onUnpinMessage={(messageId) => runtime.activeThreadId
        ? runtime.unpinMessageForCompaction(runtime.activeThreadId, messageId)
        : Promise.resolve(null)}
    />
  );
}

export function AgentChatPanelTraceSurface() {
  const { scopedOperationalEvents, scopedCognitiveEvents, pendingApprovals, daemonTodosByThread, goalRunsForTrace } = useAgentChatPanelRuntime();
  return (
    <TraceView
      operationalEvents={scopedOperationalEvents}
      cognitiveEvents={scopedCognitiveEvents}
      pendingApprovals={pendingApprovals}
      todosByThread={daemonTodosByThread}
      goalRuns={goalRunsForTrace}
    />
  );
}

export function AgentChatPanelUsageSurface() {
  const { threads, allMessagesByThread } = useAgentChatPanelRuntime();
  return <UsageView threads={threads} messagesByThread={allMessagesByThread} />;
}

export function AgentChatPanelContextSurface() {
  const runtime = useAgentChatPanelRuntime();
  return (
    <ContextView
      agentSettings={runtime.agentSettings}
      snippets={runtime.snippets}
      transcripts={runtime.transcripts}
      scopePaneId={runtime.scopePaneId}
      threads={runtime.threads}
      activeThreadId={runtime.activeThreadId}
      latestContextSnapshot={runtime.latestContextSnapshot}
      memory={runtime.memory}
      updateMemory={runtime.updateMemory}
      historyQuery={runtime.historyQuery}
      setHistoryQuery={runtime.setHistoryQuery}
      historySummary={runtime.historySummary}
      historyHits={runtime.historyHits}
      symbolQuery={runtime.symbolQuery}
      setSymbolQuery={runtime.setSymbolQuery}
      symbolHits={runtime.symbolHits}
      scopeController={runtime.scopeController}
    />
  );
}

export function AgentChatPanelGraphSurface() {
  const { scopedOperationalEvents, approvals, scopePaneId } = useAgentChatPanelGraphData();

  return (
    <div style={{ padding: "var(--space-4)", height: "100%", overflow: "auto" }}>
      <MetricRibbon
        items={[
          { label: "Commands", value: String(scopedOperationalEvents.filter((event) => event.kind === "command-started").length) },
          { label: "Approvals", value: String(approvals.length) },
          { label: "Scope", value: scopePaneId ?? "all panes" },
        ]}
      />
      <SectionTitle title="Execution Graph" subtitle="Visualized command pipeline" />
      <AgentExecutionGraph paneId={scopePaneId} />
    </div>
  );
}

export function AgentChatPanelCodingAgentsSurface() {
  return <CodingAgentsView />;
}

export function AgentChatPanelAITrainingSurface() {
  return <AITrainingView />;
}

function useAgentChatPanelGraphData() {
  const { scopedOperationalEvents, scopePaneId } = useAgentChatPanelRuntime();
  const approvals = useAgentMissionStore((state) => state.approvals);
  return { scopedOperationalEvents, approvals, scopePaneId };
}
