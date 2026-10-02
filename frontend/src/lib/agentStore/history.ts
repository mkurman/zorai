import { readPersistedJson, scheduleJsonWrite } from "../persistence";
import { boundRendererText } from "./rendererText";
import { getBridge } from "../bridge";
import { PRIMARY_AGENT_NAME } from "../agentNames";
import { normalizeAgentProviderId, normalizeApiTransport } from "./providers";
import type { AgentSettings } from "./settings";
import type {
  AgentMessage,
  AgentProviderId,
  AgentRole,
  AgentThread,
  AgentTodoItem,
  ProviderAuthState,
} from "./types";

export const AGENT_CHAT_FILE = "agent-chat.json";
export const AGENT_DAEMON_THREAD_MAP_FILE = "agent-daemon-thread-map.json";
export const AGENT_ACTIVE_THREAD_FILE = "agent-active-thread.json";

export type AgentChatState = {
  threads: AgentThread[];
  messages: Record<string, AgentMessage[]>;
  todos: Record<string, AgentTodoItem[]>;
  activeThreadId: string | null;
};

type AgentDbThreadRecord = {
  id: string;
  workspace_id: string | null;
  surface_id: string | null;
  pane_id: string | null;
  agent_name: string | null;
  title: string;
  created_at: number;
  updated_at: number;
  message_count: number;
  total_tokens: number;
  last_preview: string;
  metadata_json: string | null;
};

type AgentDbMessageRecord = {
  id: string;
  thread_id: string;
  created_at: number;
  role: string;
  content: string;
  provider: string | null;
  model: string | null;
  input_tokens: number | null;
  output_tokens: number | null;
  total_tokens: number | null;
  reasoning: string | null;
  tool_calls_json: string | null;
  metadata_json: string | null;
};

export type RemoteAgentMessageRecord = {
  id?: string;
  role?: AgentRole;
  content?: string;
  content_blocks?: AgentMessage["contentBlocks"] | null;
  author_agent_id?: string | null;
  author_agent_name?: string | null;
  provider?: string | null;
  model?: string | null;
  api_transport?: string | null;
  response_id?: string | null;
  provider_final_result?: unknown;
  tool_calls?: AgentMessage["toolCalls"] | null;
  tool_name?: string | null;
  tool_call_id?: string | null;
  tool_arguments?: string | null;
  tool_status?: string | null;
  weles_review?: AgentMessage["welesReview"] | null;
  input_tokens?: number | null;
  output_tokens?: number | null;
  cost?: number | null;
  cost_usd?: number | null;
  reasoning?: string | null;
  timestamp?: number | null;
  message_kind?: "normal" | "compaction_artifact";
  compaction_strategy?: "heuristic" | "weles" | "custom_model" | null;
  compaction_payload?: string | null;
  pinned_for_compaction?: boolean | null;
  feedback?: "up" | "down" | null;
};

export type RemoteAgentThreadRecord = {
  id?: string;
  agent_name?: string | null;
  profile_provider?: string | null;
  profile_model?: string | null;
  profile_reasoning_effort?: string | null;
  profile_context_window_tokens?: number | null;
  title?: string;
  messages?: RemoteAgentMessageRecord[];
  upstream_thread_id?: string | null;
  upstream_transport?: string | null;
  upstream_provider?: string | null;
  upstream_model?: string | null;
  upstream_assistant_id?: string | null;
  created_at?: number | null;
  updated_at?: number | null;
  total_input_tokens?: number | null;
  total_output_tokens?: number | null;
  total_cost_usd?: number | null;
  total_message_count?: number | null;
  loaded_message_start?: number | null;
  loaded_message_end?: number | null;
  active_context_window_start?: number | null;
  active_context_window_end?: number | null;
  active_context_window_tokens?: number | null;
  thread_handoff_state?: {
    origin_agent_id?: string;
    active_agent_id?: string;
    responder_stack?: Array<{
      agent_id?: string;
      agent_name?: string;
      entered_at?: number;
      linked_thread_id?: string | null;
    }>;
    pending_approval_id?: string | null;
  } | null;
  handoff_state?: RemoteAgentThreadRecord["thread_handoff_state"];
  thread_participants?: Array<{
    agent_id?: string;
    agent_name?: string;
    instruction?: string;
    status?: "active" | "inactive";
    created_at?: number | null;
    updated_at?: number | null;
    deactivated_at?: number | null;
    last_contribution_at?: number | null;
  }>;
  queued_participant_suggestions?: Array<{
    id?: string;
    target_agent_id?: string;
    target_agent_name?: string;
    instruction?: string;
    force_send?: boolean;
    status?: "queued" | "failed";
    created_at?: number | null;
    updated_at?: number | null;
    error?: string | null;
  }>;
};

type ThreadIdentityLike = {
  id?: string | null;
  daemonThreadId?: string | null;
  title?: string | null;
  lastMessagePreview?: string | null;
};

const GATEWAY_THREAD_PREFIXES = ["slack ", "discord ", "telegram ", "whatsapp "] as const;

function normalizedThreadIdentity(thread: ThreadIdentityLike): string {
  if (typeof thread.daemonThreadId === "string" && thread.daemonThreadId.trim()) {
    return thread.daemonThreadId.trim().toLowerCase();
  }
  return typeof thread.id === "string" ? thread.id.trim().toLowerCase() : "";
}

export function isInternalAgentThread(thread: ThreadIdentityLike): boolean {
  const id = normalizedThreadIdentity(thread);
  const title = typeof thread.title === "string" ? thread.title.trim().toLowerCase() : "";
  return id.startsWith("dm:") || title.startsWith("internal dm");
}

export function isGatewayAgentThread(thread: ThreadIdentityLike): boolean {
  const title = typeof thread.title === "string" ? thread.title.trim().toLowerCase() : "";
  const preview = typeof thread.lastMessagePreview === "string"
    ? thread.lastMessagePreview.trim().toLowerCase()
    : "";
  return GATEWAY_THREAD_PREFIXES.some((prefix) => title.startsWith(prefix))
    || GATEWAY_THREAD_PREFIXES.some((prefix) => preview.startsWith(`[${prefix.trim()} `))
    || GATEWAY_THREAD_PREFIXES.some((prefix) => preview.startsWith(`[${prefix.trim()} —`));
}

export function isHiddenAgentThread(thread: Pick<RemoteAgentThreadRecord, "id" | "title">): boolean {
  const id = normalizedThreadIdentity(thread);
  const title = typeof thread.title === "string" ? thread.title.trim().toLowerCase() : "";
  return id.startsWith("handoff:")
    || title.startsWith("handoff ")
    || title === "weles"
    || title.startsWith("weles ");
}

function coerceRemoteMessageIndex(value: unknown): number | null {
  if (typeof value === "number" && Number.isFinite(value)) {
    return Math.max(0, Math.trunc(value));
  }
  if (typeof value === "string" && value.trim()) {
    const parsed = Number.parseInt(value, 10);
    return Number.isFinite(parsed) ? Math.max(0, parsed) : null;
  }
  return null;
}

function epochMsOrNow(value: number | null | undefined): number {
  const n = Number(value);
  if (!Number.isFinite(n) || n <= 0) return Date.now();
  return n < 1_000_000_000_000 ? n * 1000 : n;
}

type AgentDbApi = {
  dbCreateThread?: (thread: AgentDbThreadRecord) => Promise<boolean>;
  dbDeleteThread?: (id: string) => Promise<boolean>;
  dbListThreads?: () => Promise<AgentDbThreadRecord[]>;
  dbGetThread?: (id: string) => Promise<{ thread: AgentDbThreadRecord | null; messages: AgentDbMessageRecord[] }>;
  dbAddMessage?: (message: AgentDbMessageRecord) => Promise<boolean>;
  dbExportThread?: (threadId: string, messageId: string) => Promise<{ ok: boolean; file_path?: string | null; error?: string }>;
  dbForkThread?: (threadId: string, messageId: string) => Promise<{ ok: boolean; thread_id?: string | null; title?: string | null; error?: string }>;
  dbDeleteMessage?: (threadId: string, messageId: string) => Promise<boolean>;
  dbListMessages?: (threadId: string, limit?: number | null) => Promise<AgentDbMessageRecord[]>;
};

const threadAbortControllers = new Map<string, AbortController>();

let threadIdCounter = 0;
let messageIdCounter = 0;

// Local ids must never share the daemon's `thread_` prefix: a persisted
// local thread (`thread_10100`) and a daemon thread with the same id would
// collide in identity lookups (findThreadByAuthoritativeIdentity matches
// `id` first), making clicks on a daemon thread open the unrelated local
// one. `local_thread_` keeps the namespaces disjoint.
export function nextThreadId(): string {
  threadIdCounter += 1;
  return `local_thread_${threadIdCounter}`;
}

export function nextMessageId(): string {
  messageIdCounter += 1;
  return `msg_${messageIdCounter}`;
}

export function setThreadAbortController(threadId: string, controller: AbortController): void {
  threadAbortControllers.set(threadId, controller);
}

export function abortThreadStream(threadId: string): void {
  const controller = threadAbortControllers.get(threadId);
  if (!controller) {
    return;
  }
  controller.abort();
  threadAbortControllers.delete(threadId);
}

export function clearThreadAbortController(threadId: string, controller?: AbortController): void {
  const current = threadAbortControllers.get(threadId);
  if (!current) {
    return;
  }
  if (controller && current !== controller) {
    return;
  }
  threadAbortControllers.delete(threadId);
}

export function getAgentDbApi(): AgentDbApi | null {
  const api = getBridge();
  if (!api) {
    return null;
  }
  return api as AgentDbApi;
}

export function shouldPersistHistory(backend: AgentSettings["agent_backend"]): boolean {
  void backend;
  return false;
}

export function buildHydratedRemoteMessage(
  threadId: string,
  message: RemoteAgentMessageRecord,
): AgentMessage {
  const provider = typeof message.provider === "string" ? message.provider : undefined;
  const content = boundRendererText(typeof message.content === "string" ? message.content : "");
  const isCompactionArtifact = message.message_kind === "compaction_artifact"
    || content.trim().startsWith("Pre-compaction context:");

  return {
    id: typeof message.id === "string" && message.id.trim() ? message.id : nextMessageId(),
    threadId,
    createdAt: Number(message.timestamp ?? Date.now()),
    role: message.role ?? "assistant",
    content,
    contentBlocks: Array.isArray(message.content_blocks) ? message.content_blocks : undefined,
    authorAgentId: typeof message.author_agent_id === "string" ? message.author_agent_id : undefined,
    authorAgentName: typeof message.author_agent_name === "string" ? message.author_agent_name : undefined,
    provider,
    model: typeof message.model === "string" ? message.model : undefined,
    api_transport: typeof message.api_transport === "string"
      ? normalizeApiTransport(
        typeof provider === "string" ? normalizeAgentProviderId(provider) : "openai",
        message.api_transport,
      )
      : undefined,
    responseId: typeof message.response_id === "string" ? message.response_id : undefined,
    providerFinalResult:
      message.provider_final_result && typeof message.provider_final_result === "object"
        ? message.provider_final_result
        : undefined,
    toolCalls: Array.isArray(message.tool_calls) ? message.tool_calls : undefined,
    toolName: typeof message.tool_name === "string" ? message.tool_name : undefined,
    toolCallId: typeof message.tool_call_id === "string" ? message.tool_call_id : undefined,
    toolArguments: typeof message.tool_arguments === "string" ? boundRendererText(message.tool_arguments) : undefined,
    toolStatus:
      message.tool_status === "requested"
        || message.tool_status === "executing"
        || message.tool_status === "done"
        || message.tool_status === "error"
        ? message.tool_status
        : undefined,
    welesReview: message.weles_review ?? undefined,
    inputTokens: Number(message.input_tokens ?? 0),
    outputTokens: Number(message.output_tokens ?? 0),
    totalTokens: Number(message.input_tokens ?? 0) + Number(message.output_tokens ?? 0),
    cost: typeof message.cost === "number" && Number.isFinite(message.cost)
      ? message.cost
      : typeof message.cost_usd === "number" && Number.isFinite(message.cost_usd)
      ? message.cost_usd
      : undefined,
    reasoning: typeof message.reasoning === "string" ? boundRendererText(message.reasoning) : undefined,
    isCompactionSummary: isCompactionArtifact,
    messageKind: isCompactionArtifact ? "compaction_artifact" : (message.message_kind ?? "normal"),
    compactionStrategy: message.compaction_strategy ?? undefined,
    compactionPayload: typeof message.compaction_payload === "string" ? message.compaction_payload : undefined,
    pinnedForCompaction: Boolean(message.pinned_for_compaction),
    isStreaming: false,
    feedback: message.feedback === "up" || message.feedback === "down" ? message.feedback : null,
  };
}

export function canonicalizeHydratedToolCalls(messages: AgentMessage[]): AgentMessage[] {
  const canonicalized = canonicalizeStandaloneToolRows(messages);
  let turnStandaloneCallIds = new Set<string>();

  for (let index = canonicalized.length - 1; index >= 0; index -= 1) {
    const message = canonicalized[index];
    if (message.role === "user") {
      turnStandaloneCallIds = new Set<string>();
      continue;
    }
    if (message.role === "tool" && typeof message.toolCallId === "string") {
      turnStandaloneCallIds.add(message.toolCallId);
      continue;
    }
    if (message.role !== "assistant" || !Array.isArray(message.toolCalls)) continue;

    const remaining = message.toolCalls.filter((call) => !turnStandaloneCallIds.has(call.id));
    if (remaining.length !== message.toolCalls.length) {
      canonicalized[index] = {
        ...message,
        toolCalls: remaining.length > 0 ? remaining : undefined,
      };
    }
  }

  return canonicalized;
}

function canonicalizeStandaloneToolRows(messages: AgentMessage[]): AgentMessage[] {
  const canonicalized: AgentMessage[] = [];
  let toolsByCallId = new Map<string, number>();

  for (const message of messages) {
    if (message.role === "user") {
      toolsByCallId = new Map<string, number>();
      canonicalized.push(message);
      continue;
    }
    if (message.role !== "tool" || !message.toolCallId) {
      canonicalized.push(message);
      continue;
    }

    const existingIndex = toolsByCallId.get(message.toolCallId);
    if (existingIndex == null) {
      toolsByCallId.set(message.toolCallId, canonicalized.length);
      canonicalized.push(message);
      continue;
    }

    const existing = canonicalized[existingIndex];
    canonicalized[existingIndex] = {
      ...existing,
      toolName: message.toolName || existing.toolName,
      toolArguments: message.toolArguments || existing.toolArguments,
      toolStatus: message.toolStatus || (message.content ? "done" : existing.toolStatus),
      content: message.content || existing.content,
      welesReview: message.welesReview ?? existing.welesReview,
    };
  }

  return canonicalized;
}

export function buildHydratedRemoteThread(
  thread: RemoteAgentThreadRecord,
  agent_name: string,
): {
  thread: AgentThread;
  messages: AgentMessage[];
} | null {
  if (typeof thread.id !== "string" || !thread.id.trim()) {
    return null;
  }

  if (isHiddenAgentThread(thread)) {
    return null;
  }

  const localThreadId = nextThreadId();
  const messages = Array.isArray(thread.messages)
    ? canonicalizeHydratedToolCalls(
      thread.messages.map((message) => buildHydratedRemoteMessage(localThreadId, message)),
    )
    : [];
  const totalInputTokens = Number(thread.total_input_tokens ?? 0);
  const totalOutputTokens = Number(thread.total_output_tokens ?? 0);
  const totalMessageCount = Number(thread.total_message_count ?? messages.length);
  let loadedMessageStart = coerceRemoteMessageIndex(thread.loaded_message_start);
  let loadedMessageEnd = coerceRemoteMessageIndex(thread.loaded_message_end);
  if (loadedMessageStart === null && loadedMessageEnd !== null && totalMessageCount > messages.length) {
    loadedMessageStart = Math.max(0, loadedMessageEnd - messages.length);
  } else if (
    loadedMessageStart === null
    && loadedMessageEnd === null
    && totalMessageCount > messages.length
  ) {
    loadedMessageEnd = totalMessageCount;
    loadedMessageStart = Math.max(0, totalMessageCount - messages.length);
  }
  const activeContextWindowStart = typeof thread.active_context_window_start === "number" ? thread.active_context_window_start : null;
  const activeContextWindowEnd = typeof thread.active_context_window_end === "number" ? thread.active_context_window_end : null;
  const activeContextWindowTokens = typeof thread.active_context_window_tokens === "number" ? thread.active_context_window_tokens : null;
  const resolvedAgentName = typeof thread.agent_name === "string" && thread.agent_name.trim()
    ? thread.agent_name
    : agent_name;
  const rawHandoffState = thread.thread_handoff_state ?? thread.handoff_state ?? null;
  const threadHandoffState = rawHandoffState && typeof rawHandoffState === "object"
    ? {
      originAgentId: typeof rawHandoffState.origin_agent_id === "string" ? rawHandoffState.origin_agent_id : "",
      activeAgentId: typeof rawHandoffState.active_agent_id === "string" ? rawHandoffState.active_agent_id : "",
      responderStack: Array.isArray(rawHandoffState.responder_stack)
        ? rawHandoffState.responder_stack
          .filter((frame) => typeof frame?.agent_id === "string" && typeof frame?.agent_name === "string")
          .map((frame) => ({
            agentId: frame.agent_id as string,
            agentName: frame.agent_name as string,
            enteredAt: Number(frame.entered_at ?? 0),
            linkedThreadId: typeof frame.linked_thread_id === "string" ? frame.linked_thread_id : null,
          }))
        : [],
      pendingApprovalId: typeof rawHandoffState.pending_approval_id === "string"
        ? rawHandoffState.pending_approval_id
        : null,
    }
    : null;

  return {
    thread: {
      id: localThreadId,
      daemonThreadId: thread.id,
      workspaceId: null,
      surfaceId: null,
      paneId: null,
      agent_name: resolvedAgentName,
      title: typeof thread.title === "string" && thread.title.trim()
        ? thread.title
        : "Conversation",
      createdAt: epochMsOrNow(thread.created_at),
      updatedAt: epochMsOrNow(thread.updated_at),
      messageCount: totalMessageCount,
      loadedMessageStart,
      loadedMessageEnd,
      activeContextWindowStart,
      activeContextWindowEnd,
      activeContextWindowTokens,
      profileProvider: typeof thread.profile_provider === "string" && thread.profile_provider.trim()
        ? thread.profile_provider
        : null,
      profileModel: typeof thread.profile_model === "string" && thread.profile_model.trim()
        ? thread.profile_model
        : null,
      profileReasoningEffort: typeof thread.profile_reasoning_effort === "string" && thread.profile_reasoning_effort.trim()
        ? thread.profile_reasoning_effort
        : null,
      profileContextWindowTokens: typeof thread.profile_context_window_tokens === "number" && Number.isFinite(thread.profile_context_window_tokens)
        ? Math.max(1, Math.trunc(thread.profile_context_window_tokens))
        : null,
      totalInputTokens,
      totalOutputTokens,
      totalTokens: totalInputTokens + totalOutputTokens,
    totalCostUsd: typeof thread.total_cost_usd === "number" && Number.isFinite(thread.total_cost_usd)
      ? thread.total_cost_usd
      : null,
      compactionCount: messages.filter((message) => message.messageKind === "compaction_artifact").length,
      lastMessagePreview: messages[messages.length - 1]?.content?.slice(0, 100) ?? "",
      upstreamThreadId: typeof thread.upstream_thread_id === "string" ? thread.upstream_thread_id : null,
      upstreamTransport: typeof thread.upstream_transport === "string"
        ? normalizeApiTransport(
          typeof thread.upstream_provider === "string"
            ? normalizeAgentProviderId(thread.upstream_provider)
            : "openai",
          thread.upstream_transport,
        )
        : undefined,
      upstreamProvider: typeof thread.upstream_provider === "string"
        ? normalizeAgentProviderId(thread.upstream_provider)
        : null,
      upstreamModel: typeof thread.upstream_model === "string" ? thread.upstream_model : null,
      upstreamAssistantId: typeof thread.upstream_assistant_id === "string"
        ? thread.upstream_assistant_id
        : null,
      threadParticipants: Array.isArray(thread.thread_participants)
        ? thread.thread_participants
          .filter((participant) => typeof participant?.agent_id === "string" && typeof participant?.agent_name === "string")
          .map((participant) => ({
            agentId: participant.agent_id as string,
            agentName: participant.agent_name as string,
            instruction: typeof participant.instruction === "string" ? participant.instruction : "",
            status: participant.status === "inactive" ? "inactive" : "active",
            createdAt: Number(participant.created_at ?? Date.now()),
            updatedAt: Number(participant.updated_at ?? Date.now()),
            deactivatedAt: typeof participant.deactivated_at === "number" ? participant.deactivated_at : null,
            lastContributionAt: typeof participant.last_contribution_at === "number"
              ? participant.last_contribution_at
              : null,
          }))
        : [],
      queuedParticipantSuggestions: Array.isArray(thread.queued_participant_suggestions)
        ? thread.queued_participant_suggestions
          .filter((suggestion) =>
            typeof suggestion?.id === "string"
            && typeof suggestion?.target_agent_id === "string"
            && typeof suggestion?.target_agent_name === "string")
          .map((suggestion) => ({
            id: suggestion.id as string,
            targetAgentId: suggestion.target_agent_id as string,
            targetAgentName: suggestion.target_agent_name as string,
            instruction: typeof suggestion.instruction === "string" ? suggestion.instruction : "",
            forceSend: Boolean(suggestion.force_send),
            status: suggestion.status === "failed" ? "failed" : "queued",
            createdAt: Number(suggestion.created_at ?? Date.now()),
            updatedAt: Number(suggestion.updated_at ?? Date.now()),
            error: typeof suggestion.error === "string" ? suggestion.error : null,
          }))
        : [],
      threadHandoffState,
    },
    messages,
  };
}

export function syncChatCounters(chat: AgentChatState): void {
  let maxThread = 0;
  let maxMessage = 0;

  for (const thread of chat.threads) {
    const match = /^(?:local_thread_|thread_)(\d+)$/.exec(thread.id);
    if (match) {
      maxThread = Math.max(maxThread, Number(match[1]));
    }
  }

  for (const threadMessages of Object.values(chat.messages)) {
    for (const message of threadMessages) {
      const match = /^msg_(\d+)$/.exec(message.id);
      if (match) {
        maxMessage = Math.max(maxMessage, Number(match[1]));
      }
    }
  }

  threadIdCounter = Math.max(threadIdCounter, maxThread);
  messageIdCounter = Math.max(messageIdCounter, maxMessage);
}

export function persistDaemonThreadMap(threads: AgentThread[]): void {
  const mapping = Object.fromEntries(
    threads
      .filter((thread) => typeof thread.daemonThreadId === "string" && thread.daemonThreadId)
      .map((thread) => [thread.id, thread.daemonThreadId]),
  );
  scheduleJsonWrite(AGENT_DAEMON_THREAD_MAP_FILE, mapping);
}

export function isValidProviderAuthStates(value: unknown): value is ProviderAuthState[] {
  return Array.isArray(value)
    && value.length > 0
    && value.every((entry) =>
      entry
      && typeof entry === "object"
      && typeof (entry as ProviderAuthState).provider_id === "string"
      && typeof (entry as ProviderAuthState).provider_name === "string");
}

export function serializeThread(thread: AgentThread): AgentDbThreadRecord {
  return {
    id: thread.id,
    workspace_id: thread.workspaceId ?? null,
    surface_id: thread.surfaceId ?? null,
    pane_id: thread.paneId ?? null,
    agent_name: thread.agent_name ?? null,
    title: thread.title,
    created_at: thread.createdAt,
    updated_at: thread.updatedAt,
    message_count: thread.messageCount,
    total_tokens: thread.totalTokens,
    last_preview: thread.lastMessagePreview,
    metadata_json: JSON.stringify({
      upstreamThreadId: thread.upstreamThreadId ?? null,
      targetAgentId: thread.targetAgentId ?? null,
      upstreamTransport: thread.upstreamTransport ?? null,
      upstreamProvider: thread.upstreamProvider ?? null,
      upstreamModel: thread.upstreamModel ?? null,
      upstreamAssistantId: thread.upstreamAssistantId ?? null,
      threadParticipants: thread.threadParticipants ?? [],
      queuedParticipantSuggestions: thread.queuedParticipantSuggestions ?? [],
      profileProvider: thread.profileProvider ?? null,
      profileModel: thread.profileModel ?? null,
      profileReasoningEffort: thread.profileReasoningEffort ?? null,
      profileContextWindowTokens: thread.profileContextWindowTokens ?? null,
    }),
  };
}

export function serializeMessage(message: AgentMessage): AgentDbMessageRecord {
  return {
    id: message.id,
    thread_id: message.threadId,
    created_at: message.createdAt,
    role: message.role,
    content: message.content,
    provider: message.provider ?? null,
    model: message.model ?? null,
    input_tokens: message.inputTokens,
    output_tokens: message.outputTokens,
    total_tokens: message.totalTokens,
    reasoning: message.reasoning ?? null,
    tool_calls_json: message.toolCalls ? JSON.stringify(message.toolCalls) : null,
    metadata_json: JSON.stringify({
      toolName: message.toolName ?? null,
      toolCallId: message.toolCallId ?? null,
      toolArguments: message.toolArguments ?? null,
      toolStatus: message.toolStatus ?? null,
      welesReview: message.welesReview ?? null,
      api_transport: message.api_transport ?? null,
      responseId: message.responseId ?? null,
      providerFinalResult: message.providerFinalResult ?? null,
      reasoningTokens: message.reasoningTokens ?? null,
      audioTokens: message.audioTokens ?? null,
      videoTokens: message.videoTokens ?? null,
      authorAgentId: message.authorAgentId ?? null,
      authorAgentName: message.authorAgentName ?? null,
      cost: message.cost ?? null,
      tps: message.tps ?? null,
      isCompactionSummary: message.isCompactionSummary,
      messageKind: message.messageKind ?? "normal",
      compactionStrategy: message.compactionStrategy ?? null,
      compactionPayload: message.compactionPayload ?? null,
      pinned_for_compaction: Boolean(message.pinnedForCompaction),
      pinnedForCompaction: Boolean(message.pinnedForCompaction),
      isStreaming: message.isStreaming ?? false,
    }),
  };
}

export function deserializeThread(thread: AgentDbThreadRecord): AgentThread {
  let metadata: Record<string, unknown> = {};
  try {
    metadata = typeof thread.metadata_json === "string"
      ? JSON.parse(thread.metadata_json)
      : {};
  } catch {
    metadata = {};
  }
  return {
    id: thread.id,
    daemonThreadId: null,
    workspaceId: thread.workspace_id,
    surfaceId: thread.surface_id,
    paneId: thread.pane_id,
    agent_name: thread.agent_name ?? PRIMARY_AGENT_NAME,
    title: thread.title,
    createdAt: thread.created_at,
    updatedAt: thread.updated_at,
    messageCount: thread.message_count,
    totalInputTokens: 0,
    totalOutputTokens: 0,
    totalTokens: thread.total_tokens,
    compactionCount: 0,
    lastMessagePreview: thread.last_preview,
    upstreamThreadId: typeof metadata.upstreamThreadId === "string" ? metadata.upstreamThreadId : null,
    targetAgentId: typeof metadata.targetAgentId === "string" ? metadata.targetAgentId : null,
    upstreamTransport: typeof metadata.upstreamTransport === "string"
      ? normalizeApiTransport(
        typeof metadata.upstreamProvider === "string"
          ? normalizeAgentProviderId(metadata.upstreamProvider)
          : "openai",
        metadata.upstreamTransport,
      )
      : undefined,
    upstreamProvider: typeof metadata.upstreamProvider === "string"
      ? normalizeAgentProviderId(metadata.upstreamProvider as AgentProviderId)
      : null,
    upstreamModel: typeof metadata.upstreamModel === "string" ? metadata.upstreamModel : null,
    upstreamAssistantId: typeof metadata.upstreamAssistantId === "string" ? metadata.upstreamAssistantId : null,
    threadParticipants: Array.isArray(metadata.threadParticipants) ? metadata.threadParticipants as AgentThread["threadParticipants"] : [],
    queuedParticipantSuggestions: Array.isArray(metadata.queuedParticipantSuggestions)
      ? metadata.queuedParticipantSuggestions as AgentThread["queuedParticipantSuggestions"]
      : [],
    profileProvider: typeof metadata.profileProvider === "string" ? metadata.profileProvider : null,
    profileModel: typeof metadata.profileModel === "string" ? metadata.profileModel : null,
    profileReasoningEffort: typeof metadata.profileReasoningEffort === "string" ? metadata.profileReasoningEffort : null,
    profileContextWindowTokens: typeof metadata.profileContextWindowTokens === "number" && Number.isFinite(metadata.profileContextWindowTokens)
      ? Math.max(1, Math.trunc(metadata.profileContextWindowTokens))
      : null,
  };
}

export function deserializeMessage(message: AgentDbMessageRecord): AgentMessage {
  let metadata: Record<string, unknown> = {};
  try {
    metadata = typeof message.metadata_json === "string"
      ? JSON.parse(message.metadata_json)
      : {};
  } catch {
    metadata = {};
  }
  let toolCalls: AgentMessage["toolCalls"];
  let contentBlocks: AgentMessage["contentBlocks"];
  try {
    toolCalls = typeof message.tool_calls_json === "string" ? JSON.parse(message.tool_calls_json) : undefined;
  } catch {
    toolCalls = undefined;
  }
  try {
    contentBlocks = Array.isArray((metadata as any).content_blocks)
      ? (metadata as any).content_blocks
      : Array.isArray((metadata as any).contentBlocks)
      ? (metadata as any).contentBlocks
      : undefined;
  } catch {
    contentBlocks = undefined;
  }
  return {
    id: message.id,
    threadId: message.thread_id,
    createdAt: message.created_at,
    role: message.role as AgentRole,
    content: boundRendererText(message.content),
    contentBlocks,
    authorAgentId: typeof metadata.authorAgentId === "string" ? metadata.authorAgentId : undefined,
    authorAgentName: typeof metadata.authorAgentName === "string" ? metadata.authorAgentName : undefined,
    provider: message.provider ?? undefined,
    model: message.model ?? undefined,
    api_transport: typeof metadata.api_transport === "string"
      ? normalizeApiTransport(
        typeof message.provider === "string"
          ? normalizeAgentProviderId(message.provider)
          : "openai",
        metadata.api_transport,
      )
      : undefined,
    responseId: typeof metadata.responseId === "string" ? metadata.responseId : undefined,
    providerFinalResult:
      metadata.providerFinalResult && typeof metadata.providerFinalResult === "object"
        ? metadata.providerFinalResult
        : undefined,
    toolCalls,
    toolName: (metadata.toolName as string) ?? undefined,
    toolCallId: (metadata.toolCallId as string) ?? undefined,
    toolArguments: typeof metadata.toolArguments === "string" ? boundRendererText(metadata.toolArguments) : undefined,
    toolStatus: (metadata.toolStatus as AgentMessage["toolStatus"]) ?? undefined,
    welesReview: (metadata.welesReview as AgentMessage["welesReview"]) ?? undefined,
    inputTokens: message.input_tokens ?? 0,
    outputTokens: message.output_tokens ?? 0,
    totalTokens: message.total_tokens ?? 0,
    reasoning: typeof message.reasoning === "string" ? boundRendererText(message.reasoning) : undefined,
    reasoningTokens: (metadata.reasoningTokens as number) ?? undefined,
    audioTokens: (metadata.audioTokens as number) ?? undefined,
    videoTokens: (metadata.videoTokens as number) ?? undefined,
    cost: (metadata.cost as number) ?? undefined,
    tps: (metadata.tps as number) ?? undefined,
    isCompactionSummary: Boolean(metadata.isCompactionSummary),
    messageKind:
      metadata.messageKind === "compaction_artifact" ? "compaction_artifact" : "normal",
    compactionStrategy:
      metadata.compactionStrategy === "heuristic"
        || metadata.compactionStrategy === "weles"
        || metadata.compactionStrategy === "custom_model"
        ? metadata.compactionStrategy
        : undefined,
    compactionPayload:
      typeof metadata.compactionPayload === "string"
        ? metadata.compactionPayload
        : undefined,
    pinnedForCompaction:
      typeof metadata.pinned_for_compaction === "boolean"
        ? metadata.pinned_for_compaction
        : typeof metadata.pinnedForCompaction === "boolean"
        ? metadata.pinnedForCompaction
        : undefined,
    // Streaming is ephemeral process state. A persisted message may have been
    // written mid-stream before an app exit/crash, but must never resurrect a
    // Thinking widget or Stop action after hydration.
    isStreaming: false,
  };
}

export { readPersistedJson };
