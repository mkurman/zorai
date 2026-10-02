import type React from "react";
import type { getTerminalController } from "@/lib/terminalRegistry";
import type { AgentMessage, AgentThread, AgentTodoItem, useAgentStore } from "@/lib/agentStore";
import type { AgentRun } from "@/lib/agentRuns";
import type { GoalRun } from "@/lib/goalRuns";
import type { useAgentMissionStore } from "@/lib/agentMissionStore";
import type { SpawnedAgentTree } from "@/lib/spawnedAgentTree";
import type { useSnippetStore } from "@/lib/snippetStore";
import type { useTranscriptStore } from "@/lib/transcriptStore";
import type { useWorkspaceStore } from "@/lib/workspaceStore";
import type { AgentProviderId, WelesHealthState } from "@/lib/agentStore/types";
import type { AgentContentBlock } from "@/lib/agentStore/types";

export type AgentChatPanelView =
  | "threads"
  | "internal"
  | "gateway"
  | "chat"
  | "pinned"
  | "trace"
  | "usage"
  | "context"
  | "graph"
  | "coding-agents"
  | "ai-training"
  | "tasks"
  | "subagents";

export type AgentStoreState = ReturnType<typeof useAgentStore.getState>;
export type AgentMissionStoreState = ReturnType<typeof useAgentMissionStore.getState>;
export type WorkspaceStoreState = ReturnType<typeof useWorkspaceStore.getState>;
export type SnippetStoreState = ReturnType<typeof useSnippetStore.getState>;
export type TranscriptStoreState = ReturnType<typeof useTranscriptStore.getState>;

export type ThreadMutationResult = { ok: true } | { ok: false; error: string };

export type OperationStatusView = {
  operationId: string;
  kind: string;
  state: "accepted" | "started" | "completed" | "failed" | "unknown";
  revision: number;
  dedup?: string | null;
};

export type AgentChatPanelRuntimeValue = {
  togglePanel: () => void;
  activeWorkspace: ReturnType<WorkspaceStoreState["activeWorkspace"]>;
  threads: AgentThread[];
  activeThread: AgentThread | undefined;
  activeThreadId: string | null;
  createThread: AgentStoreState["createThread"];
  deleteThread: AgentStoreState["deleteThread"];
  setActiveThread: AgentStoreState["setActiveThread"];
  openThread: (threadId: string) => void;
  agentSettings: AgentStoreState["agentSettings"];
  updateAgentSetting: AgentStoreState["updateAgentSetting"];
  searchQuery: string;
  setSearchQuery: (query: string) => void;
  refreshThreadList: () => Promise<void>;
  fetchThreadList: (options?: { agentFilter?: string | null; includeInternal?: boolean }) => Promise<AgentThread[]>;
  loadOlderThreadMessages: () => Promise<boolean>;
  trimThreadMessagesToLatestWindow: (threadId?: string | null) => boolean;
  messages: AgentMessage[];
  todos: AgentTodoItem[];
  daemonTodosByThread: Record<string, AgentTodoItem[]>;
  spawnedAgentTree: SpawnedAgentTree<AgentRun> | null;
  canGoBackThread: boolean;
  goBackThread: AgentStoreState["goBackThread"];
  canOpenSpawnedThread: (run: AgentRun) => boolean;
  openSpawnedThread: (run: AgentRun) => Promise<boolean>;
  threadNavigationDepth: number;
  backThreadTitle: string | null;
  goalRunsForTrace: GoalRun[];
  allMessagesByThread: Record<string, AgentMessage[]>;
  pendingApprovals: AgentMissionStoreState["approvals"];
  scopedOperationalEvents: AgentMissionStoreState["operationalEvents"];
  scopedCognitiveEvents: AgentMissionStoreState["cognitiveEvents"];
  latestContextSnapshot: AgentMissionStoreState["contextSnapshots"][number] | undefined;
  memory: AgentMissionStoreState["memory"];
  updateMemory: AgentMissionStoreState["updateMemory"];
  historySummary: AgentMissionStoreState["historySummary"];
  historyHits: AgentMissionStoreState["historyHits"];
  symbolHits: AgentMissionStoreState["symbolHits"];
  snippets: SnippetStoreState["snippets"];
  transcripts: TranscriptStoreState["transcripts"];
  scopePaneId: string | null;
  scopeController: ReturnType<typeof getTerminalController>;
  historyQuery: string;
  setHistoryQuery: React.Dispatch<React.SetStateAction<string>>;
  symbolQuery: string;
  setSymbolQuery: React.Dispatch<React.SetStateAction<string>>;
  view: AgentChatPanelView;
  setView: React.Dispatch<React.SetStateAction<AgentChatPanelView>>;
  chatBackView: AgentChatPanelView;
  setChatBackView: React.Dispatch<React.SetStateAction<AgentChatPanelView>>;
  usageMessageCount: number;
  filteredThreads: AgentThread[];
  isStreamingResponse: boolean;
  messagesEndRef: React.RefObject<HTMLDivElement | null>;
  inputRef: React.RefObject<HTMLTextAreaElement | null>;
  sendMessage: (payload: { text: string; contentBlocksJson?: string | null; localContentBlocks?: AgentContentBlock[] }) => void;
  spawnSubagent: (request: { title: string; description: string; cwd?: string | null }) => Promise<{ ok: boolean; content?: string; error?: string }>;
  pushHandoff: (request: { targetAgentId: string; reason: string; summary: string }) => Promise<ThreadMutationResult>;
  returnHandoff: (request: { reason: string; summary: string }) => Promise<ThreadMutationResult>;
  upsertParticipant: (request: { targetAgentId: string; instruction: string }) => Promise<ThreadMutationResult>;
  deactivateParticipant: (targetAgentId: string) => Promise<ThreadMutationResult>;
  getOperationStatus: (operationId: string) => Promise<OperationStatusView | null>;
  cancelOperation: (operationId: string) => Promise<ThreadMutationResult>;
  sendParticipantSuggestion: (threadId: string, suggestionId: string, forceSend?: boolean) => Promise<void>;
  dismissParticipantSuggestion: (threadId: string, suggestionId: string) => Promise<void>;
  deleteMessage: (threadId: string, messageId: string) => void;
  forkThread: (messageId: string) => Promise<void>;
  exportThread: (messageId: string) => Promise<void>;
  pinMessageForCompaction: (threadId: string, messageId: string) => Promise<ZoraiThreadMessagePinResult | null>;
  unpinMessageForCompaction: (threadId: string, messageId: string) => Promise<ZoraiThreadMessagePinResult | null>;
  submitMessageFeedback: (threadId: string, messageId: string, reaction: "up" | "down" | null) => Promise<void>;
  stopStreaming: (threadId?: string | null, daemonThreadId?: string | null) => void;
  retryStreamNow: (threadId?: string | null) => void;
  handleSend: () => void;
  handleKeyDown: (event: React.KeyboardEvent) => void;
  canStartGoalRun: boolean;
  startGoalRunFromPrompt: (text: string) => Promise<boolean>;
  tabItems: Array<{ id: AgentChatPanelView; label: string; count: number | null }>;
  pinnedMessages: AgentMessage[];
  pinnedBudgetChars: number;
  pinnedUsageChars: number;
  pinnedOverBudget: boolean;
  welesHealth: WelesHealthState | null;
  builtinAgentSetup: BuiltinAgentSetupState | null;
  submitBuiltinAgentSetup: (providerId: AgentProviderId, model: string) => Promise<void>;
  cancelBuiltinAgentSetup: () => void;
};

export type BuiltinAgentSetupState = {
  targetAgentId: string;
  targetAgentName: string;
  providerId: AgentProviderId;
  model: string;
  error: string | null;
};

export type NormalizeBridgePayload = (payload: any) => any;

export type AppendDaemonSystemMessage = (content: string) => void;
