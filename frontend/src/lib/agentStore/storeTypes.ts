import type { StoreApi } from "zustand";
import type {
  OperatorProfileProgress,
  OperatorProfileQuestion,
  OperatorProfileSessionCompleted,
  OperatorProfileSessionStarted,
  OperatorProfileState,
  OperatorProfileSummary,
} from "./operatorProfile";
import type { AgentSettings } from "./settings";
import type {
  AgentMessage,
  AgentThread,
  AgentTodoItem,
  ApiTransportMode,
  ProviderAuthState,
  SubAgentDefinition,
} from "./types";
import type { PaneId, SurfaceId, WorkspaceId } from "../types";

export interface ConciergeConfig {
  enabled: boolean;
  detail_level: string;
  provider?: string;
  model?: string;
  reasoning_effort?: string;
  base_url?: string | null;
  api_transport?: ApiTransportMode | null;
  claude_permission_mode?: string | null;
  openrouter_provider_order?: string[];
  openrouter_provider_ignore?: string[];
  openrouter_allow_fallbacks?: boolean | null;
  huggingface_provider?: string | null;
  auto_cleanup_on_navigate: boolean;
}

export interface ConciergeWelcomeAction {
  label: string;
  action_type: string;
  thread_id?: string;
}

export interface GatewayStatusEntry {
  status: string;
  lastError?: string;
  consecutiveFailures?: number;
  updatedAt: number;
}

export interface AgentState {
  threads: AgentThread[];
  messages: Record<string, AgentMessage[]>;
  todos: Record<string, AgentTodoItem[]>;
  activeThreadId: string | null;
  threadHistoryStack: string[];
  agentPanelOpen: boolean;
  agentSettings: AgentSettings;
  agentSettingsHydrated: boolean;
  agentSettingsDirty: boolean;
  searchQuery: string;
  providerAuthStates: ProviderAuthState[];
  subAgents: SubAgentDefinition[];
  conciergeConfig: ConciergeConfig;
  conciergeWelcome: {
    content: string;
    actions: ConciergeWelcomeAction[];
  } | null;
  operatorProfile: OperatorProfileState;
  gatewayStatuses: Record<string, GatewayStatusEntry>;

  createThread: (opts: {
    workspaceId?: WorkspaceId | null;
    surfaceId?: SurfaceId | null;
    paneId?: PaneId | null;
    title?: string;
    agentId?: string | null;
    agentName?: string | null;
    activate?: boolean;
    profileProvider?: string | null;
    profileModel?: string | null;
    profileReasoningEffort?: string | null;
    profileContextWindowTokens?: number | null;
  }) => string;
  deleteThread: (id: string) => void;
  setActiveThread: (id: string | null) => void;
  openSpawnedThread: (fromThreadId: string, toThreadId: string) => void;
  goBackThread: () => void;
  searchThreads: (query: string) => AgentThread[];
  addMessage: (threadId: string, msg: Omit<AgentMessage, "id" | "threadId" | "createdAt">) => void;
  updateLastAssistantMessage: (
    threadId: string,
    content: string,
    streaming?: boolean,
    meta?: Partial<Pick<AgentMessage, "inputTokens" | "outputTokens" | "totalTokens" | "reasoning" | "reasoningTokens" | "audioTokens" | "videoTokens" | "cost" | "tps" | "toolCalls" | "provider" | "model" | "api_transport" | "responseId" | "providerFinalResult">>,
  ) => void;
  getThreadMessages: (threadId: string) => AgentMessage[];
  deleteMessage: (threadId: string, messageId: string) => void;
  setThreadTodos: (threadId: string, todos: AgentTodoItem[]) => void;
  getThreadTodos: (threadId: string) => AgentTodoItem[];
  setThreadDaemonId: (threadId: string, daemonThreadId: string | null) => void;
  setThreadOwner: (threadId: string, owner: { agentId: string; agentName: string }) => void;
  updateThreadTitle: (threadId: string, title: string) => void;
  toggleAgentPanel: () => void;
  setSearchQuery: (query: string) => void;
  updateAgentSetting: <K extends keyof AgentSettings>(key: K, value: AgentSettings[K]) => void;
  resetAgentSettings: () => void;
  refreshAgentSettingsFromDaemon: () => Promise<boolean>;
  markAgentSettingsSynced: () => void;
  refreshProviderAuthStates: () => Promise<void>;
  validateProvider: (providerId: string, base_url: string, api_key: string, auth_source: string) => Promise<{ valid: boolean; error?: string; models?: unknown[] }>;
  loginProvider: (providerId: string, api_key: string, base_url?: string) => Promise<void>;
  logoutProvider: (providerId: string) => Promise<void>;
  addSubAgent: (def: Omit<SubAgentDefinition, "id" | "created_at">) => Promise<void>;
  removeSubAgent: (id: string) => Promise<void>;
  updateSubAgent: (def: SubAgentDefinition) => Promise<void>;
  refreshSubAgents: () => Promise<void>;
  refreshConciergeConfig: () => Promise<void>;
  updateConciergeConfig: (config: Record<string, unknown>) => Promise<void>;
  dismissConciergeWelcome: () => Promise<void>;
  setOperatorProfilePanelOpen: (open: boolean) => void;
  applyOperatorProfileSessionStarted: (event: OperatorProfileSessionStarted) => void;
  applyOperatorProfileQuestion: (question: OperatorProfileQuestion) => void;
  applyOperatorProfileProgress: (progress: OperatorProfileProgress) => void;
  applyOperatorProfileSessionCompleted: (completed: OperatorProfileSessionCompleted) => void;
  startOperatorProfileSession: (kind?: string) => Promise<OperatorProfileQuestion | null>;
  fetchNextOperatorProfileQuestion: (sessionId?: string) => Promise<OperatorProfileQuestion | null>;
  submitOperatorProfileAnswer: (answer: unknown) => Promise<void>;
  skipOperatorProfileQuestion: (reason?: string | null) => Promise<void>;
  deferOperatorProfileQuestion: (deferUntilUnixMs?: number | null) => Promise<void>;
  getOperatorProfileSummary: () => Promise<OperatorProfileSummary | null>;
  setOperatorProfileConsent: (consentKey: string, granted: boolean) => Promise<boolean>;
  maybeStartOperatorProfileOnboarding: () => Promise<void>;
  setGatewayStatus: (platform: string, status: string, lastError?: string, consecutiveFailures?: number) => void;
  getThreadsForPane: (paneId: PaneId) => AgentThread[];
}

export type AgentStoreSet = StoreApi<AgentState>["setState"];
export type AgentStoreGet = StoreApi<AgentState>["getState"];
