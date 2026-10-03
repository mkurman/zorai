import type {
  AgentMessage,
  AgentProviderConfig,
  AgentProviderId,
  ApiTransportMode,
} from "../agentStore";
import type { ToolCall, ToolDefinition } from "../agentTools";

export const APPROX_CHARS_PER_TOKEN = 4;
export const MIN_CONTEXT_TARGET_TOKENS = 1024;

export interface ApiChatMessage {
  role: string;
  content: string;
  reasoning?: string;
  tool_call_id?: string;
  name?: string;
  tool_calls?: ToolCall[];
}

export interface ChatRequest {
  provider: AgentProviderId;
  config: AgentProviderConfig;
  system_prompt: string;
  messages: ApiChatMessage[];
  streaming: boolean;
  signal?: AbortSignal;
  tools?: ToolDefinition[];
  reasoning_effort?: string;
  previousResponseId?: string;
  upstreamThreadId?: string;
}

export interface ChatChunk {
  type: "delta" | "done" | "error" | "tool_calls" | "transport_fallback";
  content: string;
  reasoning?: string;
  inputTokens?: number;
  outputTokens?: number;
  totalTokens?: number;
  cost?: number;
  reasoningTokens?: number;
  audioTokens?: number;
  videoTokens?: number;
  toolCalls?: ToolCall[];
  responseId?: string;
  upstreamThreadId?: string;
  fromTransport?: ApiTransportMode;
  toTransport?: ApiTransportMode;
}

export interface PreparedOpenAIRequest {
  messages: ApiChatMessage[];
  transport: ApiTransportMode;
  previousResponseId?: string;
  upstreamThreadId?: string;
}

export interface ContextCompactionSettings {
  auto_compact_context: boolean;
  max_context_messages: number;
  context_window_tokens: number;
  compact_threshold_pct: number;
  keep_recent_on_compact: number;
  compaction?: {
    strategy?: "heuristic" | "weles" | "custom_model";
    weles?: {
      provider?: string;
      model?: string;
    };
    custom_model?: {
      context_window_tokens?: number;
    };
  };
}

export type OpenAICodexAuthStatus = {
  available?: boolean;
  authMode?: string;
  status?: string;
  accountId?: string;
  expiresAt?: number;
  source?: string;
  error?: string;
};

export type ResolvedProviderAuth = {
  api_key: string;
  accountId?: string;
};

export type ContextAgentMessage = AgentMessage;
export type ContextAgentProviderId = AgentProviderId;
export type ContextApiTransportMode = ApiTransportMode;
