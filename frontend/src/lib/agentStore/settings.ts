import {
  getDefaultApiTransport,
  getDefaultAuthSource,
  getDefaultModelForProvider,
  getProviderDefinition,
  normalizeAgentProviderId,
  normalizeApiTransport,
  normalizeAuthSource,
  normalizeProviderConfig,
} from "./providers.ts";
import type {
  AgentBackend,
  AgentProviderConfig,
  AgentProviderId,
  ApiTransportMode,
  AuthSource,
} from "./types.ts";
import { PRIMARY_AGENT_NAME } from "../agentNames.ts";
import {
  DEFAULT_CHAT_HISTORY_PAGE_SIZE,
  normalizeReactChatHistoryPageSize,
  normalizeTuiChatHistoryPageSize,
} from "../chatHistoryPageSize.ts";

export type CompactionStrategy = "heuristic" | "weles" | "custom_model";
export type AutoThreadTitleMode = "off" | "rarog" | "weles";

export interface AgentCompactionWelesSettings {
  provider: AgentProviderId;
  model: string;
  reasoning_effort: AgentSettings["reasoning_effort"];
  api_transport?: ApiTransportMode | null;
}

export interface AgentCompactionCustomModelSettings {
  provider: AgentProviderId;
  base_url: string;
  model: string;
  api_key: string;
  assistant_id: string;
  auth_source: AuthSource;
  api_transport: ApiTransportMode;
  context_window_tokens: number;
  reasoning_effort: AgentSettings["reasoning_effort"];
}

export interface AgentCompactionSettings {
  strategy: CompactionStrategy;
  weles: AgentCompactionWelesSettings;
  custom_model: AgentCompactionCustomModelSettings;
}

export interface AgentSkillRecommendationSettings {
  enabled: boolean;
  background_community_search: boolean;
  community_preapprove_timeout_secs: number;
  suggest_global_enable_after_approvals: number;
}

export type MlflowCaptureMode = "metadata" | "guarded" | "full";

export interface AgentMlflowTracingSettings {
  enabled: boolean;
  tracking_uri: string;
  experiment_name: string;
  experiment_id: string | null;
  capture_mode: MlflowCaptureMode;
  scopes: {
    visible_operator: boolean;
    gateway: boolean;
    goal_task: boolean;
    subagent: boolean;
    concierge: boolean;
    heartbeat_autonomous: boolean;
  };
  max_user_chars: number;
  max_assistant_chars: number;
  max_reasoning_chars: number;
  max_tool_argument_chars: number;
  max_tool_result_chars: number;
  batch_size: number;
  flush_interval_ms: number;
  queue_capacity: number;
  request_timeout_ms: number;
  max_retries: number;
}

export interface AgentSettings {
  [providerId: string]: any;
  enabled: boolean;
  agent_name: string;
  handler: string;
  additionalHandlers: string[];
  system_prompt: string;
  active_provider: AgentProviderId;
  featherless: AgentProviderConfig;
  anthropic: AgentProviderConfig;
  openai: AgentProviderConfig;
  deepseek: AgentProviderConfig;
  gemini: AgentProviderConfig;
  xai: AgentProviderConfig;
  "azure-openai": AgentProviderConfig;
  "github-copilot": AgentProviderConfig;
  "hermes-agent-api": AgentProviderConfig;
  qwen: AgentProviderConfig;
  "qwen-deepinfra": AgentProviderConfig;
  kimi: AgentProviderConfig;
  "kimi-coding-plan": AgentProviderConfig;
  "z.ai": AgentProviderConfig;
  "z.ai-coding-plan": AgentProviderConfig;
  arcee: AgentProviderConfig;
  nvidia: AgentProviderConfig;
  poolside: AgentProviderConfig;
  "nous-portal": AgentProviderConfig;
  openrouter: AgentProviderConfig;
  elevenlabs: AgentProviderConfig;
  cerebras: AgentProviderConfig;
  together: AgentProviderConfig;
  groq: AgentProviderConfig;
  ollama: AgentProviderConfig;
  chutes: AgentProviderConfig;
  huggingface: AgentProviderConfig;
  minimax: AgentProviderConfig;
  "minimax-coding-plan": AgentProviderConfig;
  "alibaba-coding-plan": AgentProviderConfig;
  "alibaba-token-plan": AgentProviderConfig;
  "xiaomi-mimo-token-plan": AgentProviderConfig;
  "opencode-zen": AgentProviderConfig;
  "opencode-go": AgentProviderConfig;
  cursor: AgentProviderConfig;
  custom: AgentProviderConfig;
  enable_bash_tool: boolean;
  managed_sandbox_enabled: boolean;
  managed_security_level: "highest" | "moderate" | "lowest" | "yolo";
  enable_vision_tool: boolean;
  enable_web_browsing_tool: boolean;
  bash_timeout_seconds: number;
  enable_web_search_tool: boolean;
  search_provider: "none" | "firecrawl" | "duckduckgo" | "exa" | "tavily";
  duckduckgo_region: string;
  duckduckgo_safe_search: "off" | "moderate" | "strict";
  firecrawl_api_key: string;
  exa_api_key: string;
  tavily_api_key: string;
  search_max_results: number;
  search_timeout_secs: number;
  browse_provider: "auto" | "lightpanda" | "chrome" | "none";
  enable_streaming: boolean;
  enable_conversation_memory: boolean;
  enable_honcho_memory: boolean;
  honcho_api_key: string;
  honcho_base_url: string;
  honcho_workspace_id: string;
  anticipatory_enabled: boolean;
  anticipatory_morning_brief: boolean;
  anticipatory_predictive_hydration: boolean;
  anticipatory_stuck_detection: boolean;
  operator_model_enabled: boolean;
  operator_model_allow_message_statistics: boolean;
  operator_model_allow_approval_learning: boolean;
  operator_model_allow_attention_tracking: boolean;
  operator_model_allow_implicit_feedback: boolean;
  collaboration_enabled: boolean;
  compliance_mode: "standard" | "soc2" | "hipaa" | "fedramp";
  compliance_retention_days: number;
  compliance_sign_all_events: boolean;
  tool_synthesis_enabled: boolean;
  tool_synthesis_require_activation: boolean;
  tool_synthesis_max_generated_tools: number;
  gateway_enabled: boolean;
  slack_token: string;
  slack_channel_filter: string;
  telegram_token: string;
  telegram_allowed_chats: string;
  discord_token: string;
  discord_channel_filter: string;
  discord_allowed_users: string;
  whatsapp_token: string;
  whatsapp_phone_id: string;
  whatsapp_allowed_contacts: string;
  gateway_command_prefix: string;
  chatFontFamily: string;
  chatFontSize: number;
  audio_stt_enabled: boolean;
  audio_stt_provider: AgentProviderId;
  audio_stt_model: string;
  audio_stt_language: string;
  audio_tts_enabled: boolean;
  audio_tts_provider: AgentProviderId;
  audio_tts_model: string;
  audio_tts_voice: string;
  audio_tts_auto_speak: boolean;
  image_generation_provider: AgentProviderId;
  image_generation_model: string;
  semantic_embedding_enabled: boolean;
  semantic_embedding_provider: AgentProviderId;
  semantic_embedding_model: string;
  semantic_embedding_dimensions: number;
  semantic_embedding_batch_size: number;
  semantic_embedding_max_concurrency: number;
  auto_thread_title: AutoThreadTitleMode;
  reasoning_effort: "none" | "minimal" | "low" | "medium" | "high" | "xhigh" | "max";
  auto_compact_context: boolean;
  max_context_messages: number;
  react_chat_history_page_size: number;
  tui_chat_history_page_size: number;
  participant_observer_restore_window_hours: number;
  auto_refresh_interval_secs: number;
  max_tool_loops: number;
  max_retries: number;
  retry_delay_ms: number;
  message_loop_delay_ms: number;
  tool_call_delay_ms: number;
  llm_stream_chunk_timeout_secs: number;
  auto_retry: boolean;
  context_window_tokens: number;
  compact_threshold_pct: number;
  keep_recent_on_compact: number;
  weles_max_concurrent_reviews: number;
  code_review_agent: string;
  code_review_commit_window: number;
  compaction: AgentCompactionSettings;
  skill_recommendation: AgentSkillRecommendationSettings;
  mlflow_tracing: AgentMlflowTracingSettings;
  agent_backend: AgentBackend;
}

export const DEFAULT_AGENT_SETTINGS: AgentSettings = {
  enabled: false,
  agent_name: PRIMARY_AGENT_NAME,
  handler: "/agent",
  additionalHandlers: [],
  system_prompt: `You are ${PRIMARY_AGENT_NAME} - The Smith (He is a blacksmith god, the creator and craftsman of the heavens in ancient Slavic belief. As an AI agent:\n- Creation: Ideal for tasks intended for use from scratch (coding, writing, design).\n- Rhythm: Associated with the sun and fire, he naturally determines the daily cycles (sunrise-sunset).\n- Personality: Strict but fair; an accessible "doer" who ensures this through perfect tools.) operating in zorai, an agentic runtime assistant. You can execute terminal commands, check system resources, and send messages to connected chat platforms (Slack, Discord, Telegram, WhatsApp) via the gateway. Use your tools proactively when the user asks you to perform actions. Be concise and direct.`,
  active_provider: "openai",
  featherless: { base_url: "https://api.featherless.ai/v1", model: "meta-llama/Llama-3.3-70B-Instruct", custom_model_name: "", api_key: "", assistant_id: "", api_transport: "chat_completions", auth_source: "api_key", context_window_tokens: null },
  anthropic: { base_url: "https://api.anthropic.com", model: "claude-opus-4-7", custom_model_name: "", api_key: "", assistant_id: "", api_transport: "chat_completions", auth_source: "api_key", context_window_tokens: null },
  openai: { base_url: "https://api.openai.com/v1", model: "gpt-5.5", custom_model_name: "", api_key: "", assistant_id: "", api_transport: "responses", auth_source: "api_key", context_window_tokens: null },
  deepseek: { base_url: "https://api.deepseek.com", model: "deepseek-v4-pro", custom_model_name: "", api_key: "", assistant_id: "", api_transport: "chat_completions", auth_source: "api_key", context_window_tokens: null },
  gemini: { base_url: "https://generativelanguage.googleapis.com/v1beta", model: "gemini-3.8-flash", custom_model_name: "", api_key: "", assistant_id: "", api_transport: "chat_completions", auth_source: "api_key", context_window_tokens: null },
  xai: { base_url: "https://api.x.ai/v1", model: "grok-4.6", custom_model_name: "", api_key: "", assistant_id: "", api_transport: "responses", auth_source: "api_key", context_window_tokens: null },
  "azure-openai": { base_url: "https://YOUR-RESOURCE-NAME.openai.azure.com/openai/v1", model: "", custom_model_name: "", api_key: "", assistant_id: "", api_transport: "responses", auth_source: "api_key", context_window_tokens: null },
  "github-copilot": { base_url: "https://api.githubcopilot.com", model: "gpt-5.4", custom_model_name: "", api_key: "", assistant_id: "", api_transport: "responses", auth_source: "github_copilot", context_window_tokens: null },
  "hermes-agent-api": { base_url: "http://localhost:8642/v1", model: "hermes-agent", custom_model_name: "", api_key: "", assistant_id: "", api_transport: "chat_completions", auth_source: "api_key", context_window_tokens: null },
  qwen: { base_url: "https://dashscope-intl.aliyuncs.com/compatible-mode/v1", model: "qwen-max", custom_model_name: "", api_key: "", assistant_id: "", api_transport: "native_assistant", auth_source: "api_key", context_window_tokens: null },
  "qwen-deepinfra": { base_url: "https://api.deepinfra.com/v1/openai", model: "Qwen/Qwen2.5-72B-Instruct", custom_model_name: "", api_key: "", assistant_id: "", api_transport: "chat_completions", auth_source: "api_key", context_window_tokens: null },
  kimi: { base_url: "https://api.moonshot.ai/v1", model: "moonshot-v1-32k", custom_model_name: "", api_key: "", assistant_id: "", api_transport: "chat_completions", auth_source: "api_key", context_window_tokens: null },
  "kimi-coding-plan": { base_url: "https://api.kimi.com/coding/v1", model: "k3", custom_model_name: "", api_key: "", assistant_id: "", api_transport: "chat_completions", auth_source: "api_key", context_window_tokens: null },
  "z.ai": { base_url: "https://api.z.ai/api/paas/v4", model: "glm-4-plus", custom_model_name: "", api_key: "", assistant_id: "", api_transport: "chat_completions", auth_source: "api_key", context_window_tokens: null },
  "z.ai-coding-plan": { base_url: "https://api.z.ai/api/coding/paas/v4", model: "glm-5.3", custom_model_name: "", api_key: "", assistant_id: "", api_transport: "chat_completions", auth_source: "api_key", context_window_tokens: null },
  arcee: { base_url: "https://api.arcee.ai/api/v1", model: "trinity-large-thinking", custom_model_name: "", api_key: "", assistant_id: "", api_transport: "chat_completions", auth_source: "api_key", context_window_tokens: 256_000 },
  nvidia: { base_url: "https://integrate.api.nvidia.com/v1", model: "minimaxai/minimax-m2.7", custom_model_name: "", api_key: "", assistant_id: "", api_transport: "chat_completions", auth_source: "api_key", context_window_tokens: null },
  poolside: { base_url: "https://inference.poolside.ai/v1", model: "poolside/laguna-m.1", custom_model_name: "", api_key: "", assistant_id: "", api_transport: "chat_completions", auth_source: "api_key", context_window_tokens: null },
  "nous-portal": { base_url: "https://inference-api.nousresearch.com/v1", model: "nousresearch/hermes-4-70b", custom_model_name: "", api_key: "", assistant_id: "", api_transport: "chat_completions", auth_source: "api_key", context_window_tokens: null },
  openrouter: { base_url: "https://openrouter.ai/api/v1", model: "arcee-ai/trinity-large-thinking", custom_model_name: "", api_key: "", assistant_id: "", api_transport: "chat_completions", auth_source: "api_key", context_window_tokens: null, openrouter_response_cache_enabled: false },
  elevenlabs: { base_url: "https://api.elevenlabs.io", model: "scribe_v2", custom_model_name: "", api_key: "", assistant_id: "", api_transport: "chat_completions", auth_source: "api_key", context_window_tokens: null },
  cerebras: { base_url: "https://api.cerebras.ai/v1", model: "llama-3.3-70b", custom_model_name: "", api_key: "", assistant_id: "", api_transport: "chat_completions", auth_source: "api_key", context_window_tokens: null },
  together: { base_url: "https://api.together.xyz/v1", model: "meta-llama/Llama-3.3-70B-Instruct-Turbo", custom_model_name: "", api_key: "", assistant_id: "", api_transport: "chat_completions", auth_source: "api_key", context_window_tokens: null },
  groq: { base_url: "https://api.groq.com/openai/v1", model: "llama-3.3-70b-versatile", custom_model_name: "", api_key: "", assistant_id: "", api_transport: "responses", auth_source: "api_key", context_window_tokens: null },
  ollama: { base_url: "http://localhost:11434/v1", model: "llama3.1", custom_model_name: "", api_key: "", assistant_id: "", api_transport: "chat_completions", auth_source: "api_key", context_window_tokens: null },
  chutes: { base_url: "https://llm.chutes.ai/v1", model: "deepseek-ai/DeepSeek-R1", custom_model_name: "", api_key: "", assistant_id: "", api_transport: "chat_completions", auth_source: "api_key", context_window_tokens: null },
  huggingface: { base_url: "https://router.huggingface.co/v1", model: "meta-llama/Llama-3.3-70B-Instruct", custom_model_name: "", api_key: "", assistant_id: "", api_transport: "chat_completions", auth_source: "api_key", context_window_tokens: null },
  minimax: { base_url: "https://api.minimax.io/anthropic", model: "MiniMax-M3", custom_model_name: "", api_key: "", assistant_id: "", api_transport: "chat_completions", auth_source: "api_key", context_window_tokens: null },
  "minimax-coding-plan": { base_url: "https://api.minimax.io/anthropic", model: "MiniMax-M3", custom_model_name: "", api_key: "", assistant_id: "", api_transport: "chat_completions", auth_source: "api_key", context_window_tokens: null },
  "alibaba-coding-plan": { base_url: "https://coding-intl.dashscope.aliyuncs.com/v1", model: "qwen3.6-plus", custom_model_name: "", api_key: "", assistant_id: "", api_transport: "chat_completions", auth_source: "api_key", context_window_tokens: null },
  "alibaba-token-plan": { base_url: "https://token-plan.ap-southeast-1.maas.aliyuncs.com/compatible-mode/v1", model: "qwen3.7-max", custom_model_name: "", api_key: "", assistant_id: "", api_transport: "chat_completions", auth_source: "api_key", context_window_tokens: null },
  "xiaomi-mimo-token-plan": { base_url: "https://token-plan-ams.xiaomimimo.com/v1", model: "mimo-v2.5-pro", custom_model_name: "", api_key: "", assistant_id: "", api_transport: "chat_completions", auth_source: "api_key", context_window_tokens: null },
  "opencode-zen": { base_url: "https://opencode.ai/zen/v1", model: "claude-sonnet-4-5", custom_model_name: "", api_key: "", assistant_id: "", api_transport: "chat_completions", auth_source: "api_key", context_window_tokens: null },
  "opencode-go": { base_url: "https://opencode.ai/zen/go/v1", model: "glm-5.1", custom_model_name: "", api_key: "", assistant_id: "", api_transport: "chat_completions", auth_source: "api_key", context_window_tokens: null },
  cursor: { base_url: "", model: "auto", custom_model_name: "", api_key: "", assistant_id: "", api_transport: "native_assistant", auth_source: "api_key", context_window_tokens: null },
  custom: { base_url: "", model: "", custom_model_name: "", api_key: "", assistant_id: "", api_transport: "responses", auth_source: "api_key", context_window_tokens: 128_000 },
  enable_bash_tool: true,
  managed_sandbox_enabled: false,
  managed_security_level: "lowest",
  enable_vision_tool: false,
  enable_web_browsing_tool: false,
  bash_timeout_seconds: 30,
  enable_web_search_tool: false,
  search_provider: "none",
  duckduckgo_region: "us-en",
  duckduckgo_safe_search: "moderate",
  firecrawl_api_key: "",
  exa_api_key: "",
  tavily_api_key: "",
  search_max_results: 8,
  search_timeout_secs: 20,
  browse_provider: "auto",
  enable_streaming: true,
  enable_conversation_memory: true,
  enable_honcho_memory: false,
  honcho_api_key: "",
  honcho_base_url: "",
  honcho_workspace_id: "zorai",
  anticipatory_enabled: true,
  anticipatory_morning_brief: true,
  anticipatory_predictive_hydration: true,
  anticipatory_stuck_detection: true,
  operator_model_enabled: true,
  operator_model_allow_message_statistics: true,
  operator_model_allow_approval_learning: true,
  operator_model_allow_attention_tracking: true,
  operator_model_allow_implicit_feedback: true,
  collaboration_enabled: true,
  compliance_mode: "standard",
  compliance_retention_days: 30,
  compliance_sign_all_events: true,
  tool_synthesis_enabled: true,
  tool_synthesis_require_activation: true,
  tool_synthesis_max_generated_tools: 24,
  gateway_enabled: false,
  slack_token: "",
  slack_channel_filter: "",
  telegram_token: "",
  telegram_allowed_chats: "",
  discord_token: "",
  discord_channel_filter: "",
  discord_allowed_users: "",
  whatsapp_token: "",
  whatsapp_phone_id: "",
  whatsapp_allowed_contacts: "",
  gateway_command_prefix: "!zorai",
  chatFontFamily: "Cascadia Code",
  chatFontSize: 13,
  audio_stt_enabled: true,
  audio_stt_provider: "openai",
  audio_stt_model: "whisper-1",
  audio_stt_language: "",
  audio_tts_enabled: true,
  audio_tts_provider: "openai",
  audio_tts_model: "gpt-4o-mini-tts",
  audio_tts_voice: "alloy",
  audio_tts_auto_speak: false,
  image_generation_provider: "openai",
  image_generation_model: "gpt-image-1",
  semantic_embedding_enabled: false,
  semantic_embedding_provider: "openai",
  semantic_embedding_model: "text-embedding-3-small",
  semantic_embedding_dimensions: 1536,
  semantic_embedding_batch_size: 64,
  semantic_embedding_max_concurrency: 2,
  auto_thread_title: "off",
  reasoning_effort: "high",
  auto_compact_context: true,
  max_context_messages: 100,
  react_chat_history_page_size: DEFAULT_CHAT_HISTORY_PAGE_SIZE,
  tui_chat_history_page_size: DEFAULT_CHAT_HISTORY_PAGE_SIZE,
  participant_observer_restore_window_hours: 24,
  auto_refresh_interval_secs: 300,
  max_tool_loops: 0,
  max_retries: 3,
  retry_delay_ms: 2000,
  message_loop_delay_ms: 500,
  tool_call_delay_ms: 500,
  llm_stream_chunk_timeout_secs: 300,
  auto_retry: true,
  context_window_tokens: 128000,
  compact_threshold_pct: 80,
  keep_recent_on_compact: 10,
  weles_max_concurrent_reviews: 2,
  code_review_agent: "weles",
  code_review_commit_window: 5,
  compaction: {
    strategy: "heuristic",
    weles: {
      provider: "openai",
      model: "gpt-5.4-mini",
      reasoning_effort: "medium",
    },
    custom_model: {
      provider: "openai",
      base_url: "https://api.openai.com/v1",
      model: "gpt-5.4-mini",
      api_key: "",
      assistant_id: "",
      auth_source: "api_key",
      api_transport: "responses",
      context_window_tokens: 128000,
      reasoning_effort: "high",
    },
  },
  skill_recommendation: {
    enabled: true,
    background_community_search: true,
    community_preapprove_timeout_secs: 30,
    suggest_global_enable_after_approvals: 3,
  },
  mlflow_tracing: {
    enabled: false,
    tracking_uri: "http://127.0.0.1:5000",
    experiment_name: "zorai-conversations",
    experiment_id: null,
    capture_mode: "guarded",
    scopes: {
      visible_operator: true,
      gateway: true,
      goal_task: true,
      subagent: true,
      concierge: true,
      heartbeat_autonomous: false,
    },
    max_user_chars: 32768,
    max_assistant_chars: 32768,
    max_reasoning_chars: 8192,
    max_tool_argument_chars: 16384,
    max_tool_result_chars: 65536,
    batch_size: 16,
    flush_interval_ms: 1000,
    queue_capacity: 256,
    request_timeout_ms: 5000,
    max_retries: 3,
  },
  agent_backend: "daemon",
};

export const MANAGED_SECURITY_LEVELS = ["highest", "moderate", "lowest", "yolo"] as const;

export function normalizeAgentBackend(value: unknown): AgentBackend {
  void value;
  return "daemon";
}

function normalizeManagedSecurityLevel(value: unknown): AgentSettings["managed_security_level"] {
  if (typeof value === "string" && (MANAGED_SECURITY_LEVELS as readonly string[]).includes(value)) {
    return value as AgentSettings["managed_security_level"];
  }
  return DEFAULT_AGENT_SETTINGS.managed_security_level;
}

function normalizeParticipantObserverRestoreWindowHours(value: unknown): number {
  const numericValue = Number(value);
  if (!Number.isFinite(numericValue)) {
    return DEFAULT_AGENT_SETTINGS.participant_observer_restore_window_hours;
  }
  return Math.max(0, Math.min(24 * 30, Math.trunc(numericValue)));
}

function normalizeMlflowTracingSettings(
  value: DiskAgentSettings["mlflow_tracing"],
): AgentMlflowTracingSettings {
  const fallback = DEFAULT_AGENT_SETTINGS.mlflow_tracing;
  const captureMode = value?.capture_mode;
  const clamp = (candidate: unknown, minimum: number, maximum: number, defaultValue: number) => {
    const numeric = Number(candidate);
    return Number.isFinite(numeric)
      ? Math.max(minimum, Math.min(maximum, Math.trunc(numeric)))
      : defaultValue;
  };
  return {
    ...fallback,
    ...value,
    enabled: value?.enabled === true,
    tracking_uri: value?.tracking_uri?.trim() || fallback.tracking_uri,
    experiment_name: value?.experiment_name?.trim() || fallback.experiment_name,
    experiment_id: value?.experiment_id?.trim() || null,
    capture_mode: captureMode === "metadata" || captureMode === "full" ? captureMode : "guarded",
    scopes: { ...fallback.scopes, ...(value?.scopes ?? {}) },
    max_user_chars: clamp(value?.max_user_chars, 256, 1_000_000, fallback.max_user_chars),
    max_assistant_chars: clamp(value?.max_assistant_chars, 256, 1_000_000, fallback.max_assistant_chars),
    max_reasoning_chars: clamp(value?.max_reasoning_chars, 0, 1_000_000, fallback.max_reasoning_chars),
    max_tool_argument_chars: clamp(value?.max_tool_argument_chars, 256, 1_000_000, fallback.max_tool_argument_chars),
    max_tool_result_chars: clamp(value?.max_tool_result_chars, 256, 4_000_000, fallback.max_tool_result_chars),
    batch_size: clamp(value?.batch_size, 1, 256, fallback.batch_size),
    flush_interval_ms: clamp(value?.flush_interval_ms, 100, 60_000, fallback.flush_interval_ms),
    queue_capacity: clamp(value?.queue_capacity, 1, 10_000, fallback.queue_capacity),
    request_timeout_ms: clamp(value?.request_timeout_ms, 500, 120_000, fallback.request_timeout_ms),
    max_retries: clamp(value?.max_retries, 0, 10, fallback.max_retries),
  };
}

export function normalizeAutoThreadTitleMode(value: unknown): AutoThreadTitleMode {
  const raw = String(value ?? "").trim().toLowerCase();
  if (raw === "rarog" || raw === "weles") {
    return raw;
  }
  return "off";
}

export function normalizeAutoRefreshIntervalSecs(value: unknown): number {
  const numericValue = Number(value);
  if (!Number.isFinite(numericValue)) {
    return DEFAULT_AGENT_SETTINGS.auto_refresh_interval_secs;
  }
  return Math.max(0, Math.min(86_400, Math.trunc(numericValue)));
}

export function normalizeCodeReviewCommitWindow(value: unknown): number {
  const numericValue = Number(value);
  if (!Number.isFinite(numericValue)) {
    return DEFAULT_AGENT_SETTINGS.code_review_commit_window;
  }
  return Math.max(1, Math.min(50, Math.trunc(numericValue)));
}

export function loadAgentSettings(): AgentSettings {
  return { ...DEFAULT_AGENT_SETTINGS };
}

export type DiskAgentSettings = Partial<AgentSettings> & {
  provider?: string;
  base_url?: string;
  model?: string;
  api_key?: string;
  assistant_id?: string;
  auth_source?: string;
  api_transport?: string;
  reasoning_effort?: AgentSettings["reasoning_effort"] | string;
  audio?: {
    stt?: {
      enabled?: boolean;
      provider?: AgentProviderId | string;
      model?: string;
      language?: string;
    };
    tts?: {
      enabled?: boolean;
      provider?: AgentProviderId | string;
      model?: string;
      voice?: string;
      auto_speak?: boolean;
    };
  };
  image?: {
    generation?: {
      provider?: AgentProviderId | string;
      model?: string;
    };
  };
  semantic?: {
    embedding?: {
      enabled?: boolean;
      provider?: AgentProviderId | string;
      model?: string;
      dimensions?: number;
      batch_size?: number;
      max_concurrency?: number;
    };
  };
  image_generation_provider?: AgentProviderId | string;
  image_generation_model?: string;
  system_prompt?: string;
  managed_sandbox_enabled?: boolean;
  managed_security_level?: string;
  managed_execution?: {
    sandbox_enabled?: boolean;
    security_level?: string;
  };
  auto_compact_context?: boolean;
  max_context_messages?: number;
  react_chat_history_page_size?: number;
  tui_chat_history_page_size?: number;
  participant_observer_restore_window_hours?: number;
  auto_refresh_interval_secs?: number;
  max_tool_loops?: number;
  max_retries?: number;
  retry_delay_ms?: number;
  message_loop_delay_ms?: number;
  tool_call_delay_ms?: number;
  llm_stream_chunk_timeout_secs?: number;
  auto_retry?: boolean;
  context_window_tokens?: number;
  compact_threshold_pct?: number;
  keep_recent_on_compact?: number;
  weles_max_concurrent_reviews?: number;
  code_review_agent?: string;
  code_review_commit_window?: number;
  builtin_sub_agents?: {
    weles?: {
      max_concurrent_reviews?: number;
    };
  };
  compaction?: {
    strategy?: CompactionStrategy;
    weles?: Partial<AgentCompactionWelesSettings>;
    custom_model?: Partial<AgentCompactionCustomModelSettings>;
  };
  skill_recommendation?: Partial<AgentSkillRecommendationSettings>;
  mlflow_tracing?: Partial<AgentMlflowTracingSettings> & {
    scopes?: Partial<AgentMlflowTracingSettings["scopes"]>;
  };
  agent_backend?: AgentBackend | string;
  enable_honcho_memory?: boolean;
  honcho_api_key?: string;
  honcho_base_url?: string;
  honcho_workspace_id?: string;
  providers?: Record<string, Partial<AgentProviderConfig> & {
    base_url?: string;
    custom_model_name?: string;
    api_key?: string;
    assistant_id?: string;
    auth_source?: string;
    api_transport?: string;
    context_window_tokens?: number;
    openrouter_provider_order?: string[];
    openrouter_provider_ignore?: string[];
    openrouter_allow_fallbacks?: boolean | null;
  }>;
  tools?: {
    bash?: boolean;
    vision?: boolean;
    web_browse?: boolean;
    web_search?: boolean;
  };
  anticipatory?: {
    enabled?: boolean;
    morning_brief?: boolean;
    predictive_hydration?: boolean;
    stuck_detection?: boolean;
  };
  operator_model?: {
    enabled?: boolean;
    allow_message_statistics?: boolean;
    allow_approval_learning?: boolean;
    allow_attention_tracking?: boolean;
    allow_implicit_feedback?: boolean;
  };
  collaboration?: {
    enabled?: boolean;
  };
  compliance?: {
    mode?: AgentSettings["compliance_mode"];
    retention_days?: number;
    sign_all_events?: boolean;
  };
  tool_synthesis?: {
    enabled?: boolean;
    require_activation?: boolean;
    max_generated_tools?: number;
  };
  gateway?: {
    enabled?: boolean;
    slack_token?: string;
    slack_channel_filter?: string;
    telegram_token?: string;
    telegram_allowed_chats?: string;
    discord_token?: string;
    discord_channel_filter?: string;
    discord_allowed_users?: string;
    whatsapp_token?: string;
    whatsapp_phone_id?: string;
    whatsapp_allowed_contacts?: string;
    command_prefix?: string;
  };
};

function providerConfigFromRaw(
  providerId: AgentProviderId,
  source: DiskAgentSettings | null | undefined,
): AgentProviderConfig {
  const definition = getProviderDefinition(providerId);
  const fallback = (DEFAULT_AGENT_SETTINGS as Record<string, AgentProviderConfig>)[providerId] ?? {
    base_url: definition?.defaultBaseUrl ?? "",
    model: getDefaultModelForProvider(providerId),
    custom_model_name: "",
    api_key: "",
    assistant_id: "",
    api_transport: getDefaultApiTransport(providerId),
    auth_source: getDefaultAuthSource(providerId),
    context_window_tokens: null,
  };
  const providerMapValue = source?.providers?.[providerId];
  const flatValue = source?.[providerId] as Partial<AgentProviderConfig> | undefined;
  const mergedValue: Partial<AgentProviderConfig> = {
    ...(flatValue ?? {}),
    base_url: providerMapValue?.base_url ?? flatValue?.base_url,
    custom_model_name: providerMapValue?.custom_model_name ?? flatValue?.custom_model_name,
    api_key: providerMapValue?.api_key ?? flatValue?.api_key,
    assistant_id: providerMapValue?.assistant_id ?? flatValue?.assistant_id,
    auth_source: (providerMapValue?.auth_source ?? flatValue?.auth_source) as AuthSource | undefined,
    api_transport: (providerMapValue?.api_transport ?? flatValue?.api_transport) as ApiTransportMode | undefined,
    context_window_tokens:
      typeof providerMapValue?.context_window_tokens === "number"
        ? providerMapValue.context_window_tokens
        : flatValue?.context_window_tokens,
    openrouter_provider_order:
      providerMapValue?.openrouter_provider_order ?? flatValue?.openrouter_provider_order,
    openrouter_provider_ignore:
      providerMapValue?.openrouter_provider_ignore ?? flatValue?.openrouter_provider_ignore,
    openrouter_allow_fallbacks:
      providerMapValue?.openrouter_allow_fallbacks ?? flatValue?.openrouter_allow_fallbacks,
  };
  return normalizeProviderConfig(providerId, fallback, mergedValue);
}

function defaultAuxiliaryProvider(activeProvider: AgentProviderId, fallbackProvider: AgentProviderId): AgentProviderId {
  return activeProvider === "openrouter" ? activeProvider : fallbackProvider;
}

function defaultAuxiliaryModel(
  provider: AgentProviderId,
  fallbackModel: string,
  openRouterModel: string,
): string {
  return provider === "openrouter" ? openRouterModel : fallbackModel;
}

export function normalizeAgentSettingsFromSource(source: DiskAgentSettings): AgentSettings {
  const { context_budget_tokens: _legacyContextBudgetTokens, ...sourceSansLegacyBudget } = source as DiskAgentSettings & {
    context_budget_tokens?: number;
  };
  const active_provider = normalizeAgentProviderId(source.active_provider ?? source.provider);
  const activeProviderConfig = providerConfigFromRaw(active_provider, source);
  const authSource = normalizeAuthSource(active_provider, source.auth_source ?? activeProviderConfig.auth_source);
  const audio_stt_provider = normalizeAgentProviderId(
    source.audio?.stt?.provider
    ?? source.audio_stt_provider
    ?? defaultAuxiliaryProvider(active_provider, DEFAULT_AGENT_SETTINGS.audio_stt_provider),
  );
  const audio_tts_provider = normalizeAgentProviderId(
    source.audio?.tts?.provider
    ?? source.audio_tts_provider
    ?? defaultAuxiliaryProvider(active_provider, DEFAULT_AGENT_SETTINGS.audio_tts_provider),
  );
  const image_generation_provider = normalizeAgentProviderId(
    source.image?.generation?.provider
    ?? source.image_generation_provider
    ?? defaultAuxiliaryProvider(active_provider, DEFAULT_AGENT_SETTINGS.image_generation_provider),
  );
  const semantic_embedding_provider = normalizeAgentProviderId(
    source.semantic?.embedding?.provider
    ?? defaultAuxiliaryProvider(active_provider, DEFAULT_AGENT_SETTINGS.semantic_embedding_provider),
  );
  return {
    ...DEFAULT_AGENT_SETTINGS,
    ...sourceSansLegacyBudget,
    agent_name: DEFAULT_AGENT_SETTINGS.agent_name,
    active_provider,
    agent_backend: normalizeAgentBackendModeFromSource(source, active_provider, authSource),
    featherless: providerConfigFromRaw("featherless", source),
    anthropic: providerConfigFromRaw("anthropic", source),
    openai: providerConfigFromRaw("openai", source),
    gemini: providerConfigFromRaw("gemini", source),
    xai: providerConfigFromRaw("xai", source),
    "azure-openai": providerConfigFromRaw("azure-openai", source),
    "github-copilot": providerConfigFromRaw("github-copilot", source),
    "hermes-agent-api": providerConfigFromRaw("hermes-agent-api", source),
    qwen: providerConfigFromRaw("qwen", source),
    "qwen-deepinfra": providerConfigFromRaw("qwen-deepinfra", source),
    kimi: providerConfigFromRaw("kimi", source),
    "kimi-coding-plan": providerConfigFromRaw("kimi-coding-plan", source),
    "z.ai": providerConfigFromRaw("z.ai", source),
    "z.ai-coding-plan": providerConfigFromRaw("z.ai-coding-plan", source),
    arcee: providerConfigFromRaw("arcee", source),
    nvidia: providerConfigFromRaw("nvidia", source),
    "nous-portal": providerConfigFromRaw("nous-portal", source),
    openrouter: providerConfigFromRaw("openrouter", source),
    elevenlabs: providerConfigFromRaw("elevenlabs", source),
    cerebras: providerConfigFromRaw("cerebras", source),
    together: providerConfigFromRaw("together", source),
    groq: providerConfigFromRaw("groq", source),
    ollama: providerConfigFromRaw("ollama", source),
    chutes: providerConfigFromRaw("chutes", source),
    huggingface: providerConfigFromRaw("huggingface", source),
    minimax: providerConfigFromRaw("minimax", source),
    "minimax-coding-plan": providerConfigFromRaw("minimax-coding-plan", source),
    "alibaba-coding-plan": providerConfigFromRaw("alibaba-coding-plan", source),
    "alibaba-token-plan": providerConfigFromRaw("alibaba-token-plan", source),
    "xiaomi-mimo-token-plan": providerConfigFromRaw("xiaomi-mimo-token-plan", source),
    "opencode-zen": providerConfigFromRaw("opencode-zen", source),
    "opencode-go": providerConfigFromRaw("opencode-go", source),
    cursor: providerConfigFromRaw("cursor", source),
    custom: providerConfigFromRaw("custom", source),
    system_prompt: source.system_prompt ?? DEFAULT_AGENT_SETTINGS.system_prompt,
    audio_stt_enabled: source.audio?.stt?.enabled ?? source.audio_stt_enabled ?? DEFAULT_AGENT_SETTINGS.audio_stt_enabled,
    audio_stt_provider,
    audio_stt_model:
      source.audio?.stt?.model
      ?? source.audio_stt_model
      ?? defaultAuxiliaryModel(audio_stt_provider, DEFAULT_AGENT_SETTINGS.audio_stt_model, ""),
    audio_stt_language: source.audio?.stt?.language ?? source.audio_stt_language ?? DEFAULT_AGENT_SETTINGS.audio_stt_language,
    audio_tts_enabled: source.audio?.tts?.enabled ?? source.audio_tts_enabled ?? DEFAULT_AGENT_SETTINGS.audio_tts_enabled,
    audio_tts_provider,
    audio_tts_model:
      source.audio?.tts?.model
      ?? source.audio_tts_model
      ?? defaultAuxiliaryModel(audio_tts_provider, DEFAULT_AGENT_SETTINGS.audio_tts_model, ""),
    audio_tts_voice: source.audio?.tts?.voice ?? source.audio_tts_voice ?? DEFAULT_AGENT_SETTINGS.audio_tts_voice,
    audio_tts_auto_speak: source.audio?.tts?.auto_speak ?? source.audio_tts_auto_speak ?? DEFAULT_AGENT_SETTINGS.audio_tts_auto_speak,
    image_generation_provider,
    image_generation_model:
      source.image?.generation?.model
      ?? source.image_generation_model
      ?? defaultAuxiliaryModel(
        image_generation_provider,
        DEFAULT_AGENT_SETTINGS.image_generation_model,
        "openai/gpt-image-1",
      ),
    semantic_embedding_enabled:
      source.semantic?.embedding?.enabled ?? DEFAULT_AGENT_SETTINGS.semantic_embedding_enabled,
    semantic_embedding_provider,
    semantic_embedding_model:
      source.semantic?.embedding?.model
      ?? defaultAuxiliaryModel(
        semantic_embedding_provider,
        DEFAULT_AGENT_SETTINGS.semantic_embedding_model,
        "openai/text-embedding-3-small",
      ),
    semantic_embedding_dimensions:
      source.semantic?.embedding?.dimensions ?? DEFAULT_AGENT_SETTINGS.semantic_embedding_dimensions,
    semantic_embedding_batch_size:
      source.semantic?.embedding?.batch_size ?? DEFAULT_AGENT_SETTINGS.semantic_embedding_batch_size,
    semantic_embedding_max_concurrency:
      source.semantic?.embedding?.max_concurrency ?? DEFAULT_AGENT_SETTINGS.semantic_embedding_max_concurrency,
    auto_thread_title: normalizeAutoThreadTitleMode(
      source.auto_thread_title ?? DEFAULT_AGENT_SETTINGS.auto_thread_title,
    ),
    reasoning_effort: (source.reasoning_effort ?? DEFAULT_AGENT_SETTINGS.reasoning_effort) as AgentSettings["reasoning_effort"],
    auto_compact_context: source.auto_compact_context ?? DEFAULT_AGENT_SETTINGS.auto_compact_context,
    max_context_messages: source.max_context_messages ?? DEFAULT_AGENT_SETTINGS.max_context_messages,
    react_chat_history_page_size: normalizeReactChatHistoryPageSize(
      source.react_chat_history_page_size
      ?? DEFAULT_AGENT_SETTINGS.react_chat_history_page_size,
    ),
    tui_chat_history_page_size: normalizeTuiChatHistoryPageSize(
      source.tui_chat_history_page_size
      ?? DEFAULT_AGENT_SETTINGS.tui_chat_history_page_size,
    ),
    participant_observer_restore_window_hours: normalizeParticipantObserverRestoreWindowHours(
      source.participant_observer_restore_window_hours
      ?? DEFAULT_AGENT_SETTINGS.participant_observer_restore_window_hours,
    ),
    auto_refresh_interval_secs: normalizeAutoRefreshIntervalSecs(
      source.auto_refresh_interval_secs
      ?? DEFAULT_AGENT_SETTINGS.auto_refresh_interval_secs,
    ),
    max_tool_loops: source.max_tool_loops ?? DEFAULT_AGENT_SETTINGS.max_tool_loops,
    max_retries: source.max_retries ?? DEFAULT_AGENT_SETTINGS.max_retries,
    retry_delay_ms: source.retry_delay_ms ?? DEFAULT_AGENT_SETTINGS.retry_delay_ms,
    message_loop_delay_ms: source.message_loop_delay_ms ?? DEFAULT_AGENT_SETTINGS.message_loop_delay_ms,
    tool_call_delay_ms: source.tool_call_delay_ms ?? DEFAULT_AGENT_SETTINGS.tool_call_delay_ms,
    llm_stream_chunk_timeout_secs:
      source.llm_stream_chunk_timeout_secs ?? DEFAULT_AGENT_SETTINGS.llm_stream_chunk_timeout_secs,
    auto_retry: source.auto_retry ?? DEFAULT_AGENT_SETTINGS.auto_retry,
    context_window_tokens: source.context_window_tokens ?? DEFAULT_AGENT_SETTINGS.context_window_tokens,
    compact_threshold_pct: source.compact_threshold_pct ?? DEFAULT_AGENT_SETTINGS.compact_threshold_pct,
    keep_recent_on_compact: source.keep_recent_on_compact ?? DEFAULT_AGENT_SETTINGS.keep_recent_on_compact,
    weles_max_concurrent_reviews:
      source.weles_max_concurrent_reviews
      ?? source.builtin_sub_agents?.weles?.max_concurrent_reviews
      ?? DEFAULT_AGENT_SETTINGS.weles_max_concurrent_reviews,
    code_review_agent:
      typeof source.code_review_agent === "string" && source.code_review_agent.trim()
        ? source.code_review_agent.trim()
        : DEFAULT_AGENT_SETTINGS.code_review_agent,
    code_review_commit_window: normalizeCodeReviewCommitWindow(
      source.code_review_commit_window ?? DEFAULT_AGENT_SETTINGS.code_review_commit_window,
    ),
    compaction: {
      strategy: source.compaction?.strategy ?? DEFAULT_AGENT_SETTINGS.compaction.strategy,
      weles: {
        provider: normalizeAgentProviderId(
          source.compaction?.weles?.provider ?? DEFAULT_AGENT_SETTINGS.compaction.weles.provider,
        ),
        model: source.compaction?.weles?.model ?? DEFAULT_AGENT_SETTINGS.compaction.weles.model,
        reasoning_effort: (source.compaction?.weles?.reasoning_effort
          ?? DEFAULT_AGENT_SETTINGS.compaction.weles.reasoning_effort) as AgentSettings["reasoning_effort"],
        api_transport: source.compaction?.weles?.api_transport ?? undefined,
      },
      custom_model: {
        provider: normalizeAgentProviderId(
          source.compaction?.custom_model?.provider ?? DEFAULT_AGENT_SETTINGS.compaction.custom_model.provider,
        ),
        base_url: source.compaction?.custom_model?.base_url ?? DEFAULT_AGENT_SETTINGS.compaction.custom_model.base_url,
        model: source.compaction?.custom_model?.model ?? DEFAULT_AGENT_SETTINGS.compaction.custom_model.model,
        api_key: source.compaction?.custom_model?.api_key ?? DEFAULT_AGENT_SETTINGS.compaction.custom_model.api_key,
        assistant_id: source.compaction?.custom_model?.assistant_id ?? DEFAULT_AGENT_SETTINGS.compaction.custom_model.assistant_id,
        auth_source: normalizeAuthSource(
          normalizeAgentProviderId(
            source.compaction?.custom_model?.provider ?? DEFAULT_AGENT_SETTINGS.compaction.custom_model.provider,
          ),
          source.compaction?.custom_model?.auth_source ?? DEFAULT_AGENT_SETTINGS.compaction.custom_model.auth_source,
        ),
        api_transport: normalizeApiTransport(
          normalizeAgentProviderId(
            source.compaction?.custom_model?.provider ?? DEFAULT_AGENT_SETTINGS.compaction.custom_model.provider,
          ),
          source.compaction?.custom_model?.api_transport ?? DEFAULT_AGENT_SETTINGS.compaction.custom_model.api_transport,
        ),
        context_window_tokens:
          source.compaction?.custom_model?.context_window_tokens
          ?? DEFAULT_AGENT_SETTINGS.compaction.custom_model.context_window_tokens,
        reasoning_effort: (source.compaction?.custom_model?.reasoning_effort
          ?? DEFAULT_AGENT_SETTINGS.compaction.custom_model.reasoning_effort) as AgentSettings["reasoning_effort"],
      },
    },
    skill_recommendation: {
      enabled:
        source.skill_recommendation?.enabled
        ?? DEFAULT_AGENT_SETTINGS.skill_recommendation.enabled,
      background_community_search:
        source.skill_recommendation?.background_community_search
        ?? DEFAULT_AGENT_SETTINGS.skill_recommendation.background_community_search,
      community_preapprove_timeout_secs:
        source.skill_recommendation?.community_preapprove_timeout_secs
        ?? DEFAULT_AGENT_SETTINGS.skill_recommendation.community_preapprove_timeout_secs,
      suggest_global_enable_after_approvals:
        source.skill_recommendation?.suggest_global_enable_after_approvals
        ?? DEFAULT_AGENT_SETTINGS.skill_recommendation.suggest_global_enable_after_approvals,
    },
    mlflow_tracing: normalizeMlflowTracingSettings(source.mlflow_tracing),
    enable_honcho_memory: source.enable_honcho_memory ?? DEFAULT_AGENT_SETTINGS.enable_honcho_memory,
    honcho_api_key: source.honcho_api_key ?? DEFAULT_AGENT_SETTINGS.honcho_api_key,
    honcho_base_url: source.honcho_base_url ?? DEFAULT_AGENT_SETTINGS.honcho_base_url,
    honcho_workspace_id: source.honcho_workspace_id ?? DEFAULT_AGENT_SETTINGS.honcho_workspace_id,
    enable_bash_tool: source.enable_bash_tool ?? source.tools?.bash ?? DEFAULT_AGENT_SETTINGS.enable_bash_tool,
    managed_sandbox_enabled:
      source.managed_sandbox_enabled
      ?? source.managed_execution?.sandbox_enabled
      ?? DEFAULT_AGENT_SETTINGS.managed_sandbox_enabled,
    managed_security_level: normalizeManagedSecurityLevel(
      source.managed_security_level ?? source.managed_execution?.security_level,
    ),
    enable_vision_tool: source.enable_vision_tool ?? source.tools?.vision ?? DEFAULT_AGENT_SETTINGS.enable_vision_tool,
    enable_web_browsing_tool: source.enable_web_browsing_tool ?? source.tools?.web_browse ?? DEFAULT_AGENT_SETTINGS.enable_web_browsing_tool,
    enable_web_search_tool: source.enable_web_search_tool ?? source.tools?.web_search ?? DEFAULT_AGENT_SETTINGS.enable_web_search_tool,
    anticipatory_enabled: source.anticipatory?.enabled ?? source.anticipatory_enabled ?? DEFAULT_AGENT_SETTINGS.anticipatory_enabled,
    anticipatory_morning_brief: source.anticipatory?.morning_brief ?? source.anticipatory_morning_brief ?? DEFAULT_AGENT_SETTINGS.anticipatory_morning_brief,
    anticipatory_predictive_hydration: source.anticipatory?.predictive_hydration ?? source.anticipatory_predictive_hydration ?? DEFAULT_AGENT_SETTINGS.anticipatory_predictive_hydration,
    anticipatory_stuck_detection: source.anticipatory?.stuck_detection ?? source.anticipatory_stuck_detection ?? DEFAULT_AGENT_SETTINGS.anticipatory_stuck_detection,
    operator_model_enabled: source.operator_model?.enabled ?? source.operator_model_enabled ?? DEFAULT_AGENT_SETTINGS.operator_model_enabled,
    operator_model_allow_message_statistics: source.operator_model?.allow_message_statistics ?? source.operator_model_allow_message_statistics ?? DEFAULT_AGENT_SETTINGS.operator_model_allow_message_statistics,
    operator_model_allow_approval_learning: source.operator_model?.allow_approval_learning ?? source.operator_model_allow_approval_learning ?? DEFAULT_AGENT_SETTINGS.operator_model_allow_approval_learning,
    operator_model_allow_attention_tracking: source.operator_model?.allow_attention_tracking ?? source.operator_model_allow_attention_tracking ?? DEFAULT_AGENT_SETTINGS.operator_model_allow_attention_tracking,
    operator_model_allow_implicit_feedback: source.operator_model?.allow_implicit_feedback ?? source.operator_model_allow_implicit_feedback ?? DEFAULT_AGENT_SETTINGS.operator_model_allow_implicit_feedback,
    collaboration_enabled: source.collaboration?.enabled ?? source.collaboration_enabled ?? DEFAULT_AGENT_SETTINGS.collaboration_enabled,
    compliance_mode: source.compliance?.mode ?? source.compliance_mode ?? DEFAULT_AGENT_SETTINGS.compliance_mode,
    compliance_retention_days: source.compliance?.retention_days ?? source.compliance_retention_days ?? DEFAULT_AGENT_SETTINGS.compliance_retention_days,
    compliance_sign_all_events: source.compliance?.sign_all_events ?? source.compliance_sign_all_events ?? DEFAULT_AGENT_SETTINGS.compliance_sign_all_events,
    tool_synthesis_enabled: source.tool_synthesis?.enabled ?? source.tool_synthesis_enabled ?? DEFAULT_AGENT_SETTINGS.tool_synthesis_enabled,
    tool_synthesis_require_activation: source.tool_synthesis?.require_activation ?? source.tool_synthesis_require_activation ?? DEFAULT_AGENT_SETTINGS.tool_synthesis_require_activation,
    tool_synthesis_max_generated_tools: source.tool_synthesis?.max_generated_tools ?? source.tool_synthesis_max_generated_tools ?? DEFAULT_AGENT_SETTINGS.tool_synthesis_max_generated_tools,
    gateway_enabled: source.gateway?.enabled ?? DEFAULT_AGENT_SETTINGS.gateway_enabled,
    slack_token: source.gateway?.slack_token ?? DEFAULT_AGENT_SETTINGS.slack_token,
    slack_channel_filter: source.gateway?.slack_channel_filter ?? DEFAULT_AGENT_SETTINGS.slack_channel_filter,
    telegram_token: source.gateway?.telegram_token ?? DEFAULT_AGENT_SETTINGS.telegram_token,
    telegram_allowed_chats: source.gateway?.telegram_allowed_chats ?? DEFAULT_AGENT_SETTINGS.telegram_allowed_chats,
    discord_token: source.gateway?.discord_token ?? DEFAULT_AGENT_SETTINGS.discord_token,
    discord_channel_filter: source.gateway?.discord_channel_filter ?? DEFAULT_AGENT_SETTINGS.discord_channel_filter,
    discord_allowed_users: source.gateway?.discord_allowed_users ?? DEFAULT_AGENT_SETTINGS.discord_allowed_users,
    whatsapp_token: source.gateway?.whatsapp_token ?? DEFAULT_AGENT_SETTINGS.whatsapp_token,
    whatsapp_phone_id: source.gateway?.whatsapp_phone_id ?? DEFAULT_AGENT_SETTINGS.whatsapp_phone_id,
    whatsapp_allowed_contacts: source.gateway?.whatsapp_allowed_contacts ?? DEFAULT_AGENT_SETTINGS.whatsapp_allowed_contacts,
    gateway_command_prefix: source.gateway?.command_prefix ?? DEFAULT_AGENT_SETTINGS.gateway_command_prefix,
    [active_provider]: {
      ...activeProviderConfig,
      base_url: source.base_url ?? activeProviderConfig.base_url,
      model: source.model ?? activeProviderConfig.model,
      api_key: source.api_key ?? activeProviderConfig.api_key,
      assistant_id: source.assistant_id ?? activeProviderConfig.assistant_id,
      auth_source: authSource,
      api_transport: normalizeApiTransport(active_provider, source.api_transport ?? activeProviderConfig.api_transport),
    },
  };
}

function normalizeAgentBackendModeFromSource(
  source: DiskAgentSettings,
  activeProvider: AgentProviderId,
  authSource: AuthSource,
): AgentBackend {
  void source;
  void activeProvider;
  void authSource;
  return "daemon";
}

export function looksLikeDaemonAgentConfig(value: unknown): value is DiskAgentSettings {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return false;
  }
  const record = value as Record<string, unknown>;
  return Boolean(
    typeof record.provider === "string"
    || typeof record.active_provider === "string"
    || (record.providers && typeof record.providers === "object" && !Array.isArray(record.providers))
    || (record.image && typeof record.image === "object" && !Array.isArray(record.image))
    || typeof record.image_generation_provider === "string"
    || typeof record.agent_backend === "string",
  );
}
