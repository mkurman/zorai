import type { AgentSettings } from "./agentStore/settings.ts";
import {
  getDefaultAuthSource,
  getEffectiveContextWindow,
  normalizeAuthSource,
  normalizeApiTransport,
} from "./agentStore/providers.ts";
import type { AgentProviderConfig } from "./agentStore/types.ts";
import { getBridge } from "./bridge.ts";

export type DaemonOwnedAuthCapability = {
  daemonOwnedAuthAvailable: boolean;
  chatgptSubscriptionAvailable: boolean;
};

type SnapshotRetentionSettings = {
  snapshotMaxCount: number;
  snapshotMaxSizeMb: number;
  snapshotAutoCleanup: boolean;
};

export function getAgentBridge() {
  return getBridge();
}

export function resolveDaemonBackend(
  backend: AgentSettings["agent_backend"],
): AgentSettings["agent_backend"] {
  void backend;
  return "daemon";
}

export function shouldUseDaemonRuntime(
  backend: AgentSettings["agent_backend"],
): boolean {
  void backend;
  return Boolean(getAgentBridge()?.agentSendMessage);
}

export function getDaemonOwnedAuthCapability(
  backend: AgentSettings["agent_backend"],
  bridge: ZoraiBridge | null = getAgentBridge(),
): DaemonOwnedAuthCapability {
  void backend;
  const daemonOwnedAuthAvailable = Boolean(bridge?.agentSendMessage);

  return {
    daemonOwnedAuthAvailable,
    chatgptSubscriptionAvailable: daemonOwnedAuthAvailable,
  };
}

export function getProviderAuthSupportOptions(
  backend: AgentSettings["agent_backend"],
  bridge: ZoraiBridge | null = getAgentBridge(),
): { daemonOwnedAuthAvailable: boolean } {
  return {
    daemonOwnedAuthAvailable: getDaemonOwnedAuthCapability(backend, bridge).daemonOwnedAuthAvailable,
  };
}

function escapeJsonPointerSegment(segment: string): string {
  return segment.replace(/~/g, "~0").replace(/\//g, "~1");
}

export function flattenDaemonConfigEntries(
  value: unknown,
  pointer = "",
  entries: Array<{ keyPath: string; value: unknown }> = [],
): Array<{ keyPath: string; value: unknown }> {
  if (value && typeof value === "object" && !Array.isArray(value)) {
    const record = value as Record<string, unknown>;
    const keys = Object.keys(record);
    if (keys.length > 0) {
      for (const key of keys) {
        flattenDaemonConfigEntries(
          record[key],
          `${pointer}/${escapeJsonPointerSegment(key)}`,
          entries,
        );
      }
      return entries;
    }
  }
  entries.push({ keyPath: pointer, value });
  return entries;
}

export function diffDaemonConfigEntries(
  previous: unknown,
  next: unknown,
): Array<{ keyPath: string; value: unknown }> {
  const previousMap = new Map(
    flattenDaemonConfigEntries(previous).map(({ keyPath, value }) => [
      keyPath,
      JSON.stringify(value),
    ]),
  );
  return flattenDaemonConfigEntries(next).filter(
    ({ keyPath, value }) => previousMap.get(keyPath) !== JSON.stringify(value),
  );
}

export function buildDaemonAgentConfig(
  agentSettings: AgentSettings,
  snapshotSettings?: SnapshotRetentionSettings,
) {
  const daemonBackend = resolveDaemonBackend(agentSettings.agent_backend);
  const providerKey = agentSettings.active_provider;
  const providerConfig = agentSettings[providerKey] as AgentProviderConfig | undefined;
  const authSupportOptions = getProviderAuthSupportOptions(agentSettings.agent_backend);
  const authSource = providerConfig
    ? normalizeAuthSource(providerKey, providerConfig.auth_source, authSupportOptions)
    : getDefaultAuthSource(providerKey, authSupportOptions);
  const referencedProviderIds = Array.from(new Set([
    providerKey,
    agentSettings.audio_stt_provider,
    agentSettings.audio_tts_provider,
    agentSettings.image_generation_provider,
    agentSettings.semantic_embedding_provider,
  ]));
  const providerConfigs = Object.fromEntries(
    referencedProviderIds.flatMap((referencedProviderId) => {
      const referencedConfig = agentSettings[referencedProviderId] as AgentProviderConfig | undefined;
      if (!referencedConfig) {
        return [];
      }
      const referencedAuthSource = normalizeAuthSource(
        referencedProviderId,
        referencedConfig.auth_source,
        authSupportOptions,
      );
      return [[
        referencedProviderId,
        {
          base_url: referencedConfig.base_url || "",
          model: referencedConfig.model || "",
          assistant_id: referencedConfig.assistant_id || "",
          api_transport: normalizeApiTransport(referencedProviderId, referencedConfig.api_transport),
          auth_source: referencedAuthSource,
          context_window_tokens: getEffectiveContextWindow(referencedProviderId, referencedConfig),
          reasoning_effort: agentSettings.reasoning_effort || "high",
          ...(referencedProviderId === "openrouter"
            ? {
              openrouter_provider_order: referencedConfig.openrouter_provider_order ?? [],
              openrouter_provider_ignore: referencedConfig.openrouter_provider_ignore ?? [],
              openrouter_allow_fallbacks: referencedConfig.openrouter_allow_fallbacks ?? null,
              openrouter_response_cache_enabled: referencedConfig.openrouter_response_cache_enabled === true,
            }
            : {}),
        },
      ]];
    }),
  );

  return {
    enabled: agentSettings.enabled,
    agent_backend: daemonBackend,
    provider: providerKey,
    base_url: providerConfig?.base_url || "",
    model: providerConfig?.model || "",
    assistant_id: providerConfig?.assistant_id || "",
    api_transport: normalizeApiTransport(providerKey, providerConfig?.api_transport),
    auth_source: authSource,
    reasoning_effort: agentSettings.reasoning_effort || "high",
    system_prompt: agentSettings.system_prompt,
    agent_aliases: agentSettings.agent_aliases,
    managed_sandbox_enabled: agentSettings.managed_sandbox_enabled,
    managed_security_level: agentSettings.managed_security_level,
    managed_execution: {
      sandbox_enabled: agentSettings.managed_sandbox_enabled,
      security_level: agentSettings.managed_security_level,
    },
    auto_compact_context: agentSettings.auto_compact_context,
    max_context_messages: agentSettings.max_context_messages,
    react_chat_history_page_size: agentSettings.react_chat_history_page_size,
    tui_chat_history_page_size: agentSettings.tui_chat_history_page_size,
    participant_observer_restore_window_hours:
      agentSettings.participant_observer_restore_window_hours,
    auto_refresh_interval_secs: agentSettings.auto_refresh_interval_secs,
    max_tool_loops: agentSettings.max_tool_loops,
    max_retries: agentSettings.max_retries,
    retry_delay_ms: agentSettings.retry_delay_ms,
    message_loop_delay_ms: agentSettings.message_loop_delay_ms,
    tool_call_delay_ms: agentSettings.tool_call_delay_ms,
    llm_stream_chunk_timeout_secs: agentSettings.llm_stream_chunk_timeout_secs,
    auto_retry: agentSettings.auto_retry,
    auto_thread_title: agentSettings.auto_thread_title,
    context_window_tokens: providerConfig
      ? getEffectiveContextWindow(providerKey, providerConfig)
      : 128000,
    search_provider: agentSettings.search_provider,
    duckduckgo_region: agentSettings.duckduckgo_region,
    duckduckgo_safe_search: agentSettings.duckduckgo_safe_search,
    firecrawl_api_key: agentSettings.firecrawl_api_key,
    exa_api_key: agentSettings.exa_api_key,
    tavily_api_key: agentSettings.tavily_api_key,
    search_max_results: agentSettings.search_max_results,
    search_timeout_secs: agentSettings.search_timeout_secs,
    browse_provider: agentSettings.browse_provider,
    compact_threshold_pct: agentSettings.compact_threshold_pct,
    keep_recent_on_compact: agentSettings.keep_recent_on_compact,
    builtin_sub_agents: {
      weles: {
        max_concurrent_reviews: agentSettings.weles_max_concurrent_reviews,
      },
    },
    compaction: {
      strategy: agentSettings.compaction.strategy,
      weles: {
        provider: agentSettings.compaction.weles.provider,
        model: agentSettings.compaction.weles.model,
        reasoning_effort: agentSettings.compaction.weles.reasoning_effort,
      },
      custom_model: {
        provider: agentSettings.compaction.custom_model.provider,
        base_url: agentSettings.compaction.custom_model.base_url,
        model: agentSettings.compaction.custom_model.model,
        api_key: agentSettings.compaction.custom_model.api_key,
        assistant_id: agentSettings.compaction.custom_model.assistant_id,
        auth_source: agentSettings.compaction.custom_model.auth_source,
        api_transport: normalizeApiTransport(
          agentSettings.compaction.custom_model.provider,
          agentSettings.compaction.custom_model.api_transport,
        ),
        reasoning_effort: agentSettings.compaction.custom_model.reasoning_effort,
        context_window_tokens: agentSettings.compaction.custom_model.context_window_tokens,
      },
    },
    skill_recommendation: {
      enabled: agentSettings.skill_recommendation.enabled,
      background_community_search: agentSettings.skill_recommendation.background_community_search,
      community_preapprove_timeout_secs:
        agentSettings.skill_recommendation.community_preapprove_timeout_secs,
      suggest_global_enable_after_approvals:
        agentSettings.skill_recommendation.suggest_global_enable_after_approvals,
    },
    mlflow_tracing: agentSettings.mlflow_tracing,
    enable_honcho_memory: agentSettings.enable_honcho_memory,
    honcho_api_key: agentSettings.honcho_api_key,
    honcho_base_url: agentSettings.honcho_base_url,
    honcho_workspace_id: agentSettings.honcho_workspace_id,
    providers: providerConfigs,
    audio: {
      stt: {
        enabled: agentSettings.audio_stt_enabled,
        provider: agentSettings.audio_stt_provider,
        model: agentSettings.audio_stt_model,
        language: agentSettings.audio_stt_language,
      },
      tts: {
        enabled: agentSettings.audio_tts_enabled,
        provider: agentSettings.audio_tts_provider,
        model: agentSettings.audio_tts_model,
        voice: agentSettings.audio_tts_voice,
        auto_speak: agentSettings.audio_tts_auto_speak,
      },
    },
    image: {
      generation: {
        provider: agentSettings.image_generation_provider,
        model: agentSettings.image_generation_model,
      },
    },
    semantic: {
      embedding: {
        enabled: agentSettings.semantic_embedding_enabled,
        provider: agentSettings.semantic_embedding_provider,
        model: agentSettings.semantic_embedding_model,
        dimensions: Math.max(1, Math.floor(agentSettings.semantic_embedding_dimensions)),
        batch_size: Math.max(1, Math.floor(agentSettings.semantic_embedding_batch_size)),
        max_concurrency: Math.max(1, Math.floor(agentSettings.semantic_embedding_max_concurrency)),
      },
    },
    tools: {
      bash: agentSettings.enable_bash_tool,
      web_search: agentSettings.enable_web_search_tool,
      web_browse: agentSettings.enable_web_browsing_tool,
      vision: agentSettings.enable_vision_tool,
      gateway_messaging: true,
      file_operations: true,
      system_info: true,
    },
    gateway: {
      enabled: agentSettings.gateway_enabled,
      slack_token: agentSettings.slack_token,
      slack_channel_filter: agentSettings.slack_channel_filter,
      telegram_token: agentSettings.telegram_token,
      telegram_allowed_chats: agentSettings.telegram_allowed_chats,
      discord_token: agentSettings.discord_token,
      discord_channel_filter: agentSettings.discord_channel_filter,
      discord_allowed_users: agentSettings.discord_allowed_users,
      whatsapp_token: agentSettings.whatsapp_token,
      whatsapp_phone_id: agentSettings.whatsapp_phone_id,
      whatsapp_allowed_contacts: agentSettings.whatsapp_allowed_contacts,
      command_prefix: agentSettings.gateway_command_prefix || "!zorai",
      default_agent: agentSettings.gateway_default_agent || "rarog",
      slack_default_agent: agentSettings.slack_default_agent,
      discord_default_agent: agentSettings.discord_default_agent,
      telegram_default_agent: agentSettings.telegram_default_agent,
      whatsapp_default_agent: agentSettings.whatsapp_default_agent,
    },
    snapshot_retention: snapshotSettings
      ? {
        max_snapshots: Math.max(0, Math.floor(snapshotSettings.snapshotMaxCount)),
        max_total_size_mb: Math.max(1, Math.floor(snapshotSettings.snapshotMaxSizeMb)),
        auto_cleanup: snapshotSettings.snapshotAutoCleanup,
      }
      : undefined,
    anticipatory: {
      enabled: agentSettings.anticipatory_enabled,
      morning_brief: agentSettings.anticipatory_morning_brief,
      predictive_hydration: agentSettings.anticipatory_predictive_hydration,
      stuck_detection: agentSettings.anticipatory_stuck_detection,
    },
    operator_model: {
      enabled: agentSettings.operator_model_enabled,
      allow_message_statistics: agentSettings.operator_model_allow_message_statistics,
      allow_approval_learning: agentSettings.operator_model_allow_approval_learning,
      allow_attention_tracking: agentSettings.operator_model_allow_attention_tracking,
      allow_implicit_feedback: agentSettings.operator_model_allow_implicit_feedback,
    },
    collaboration: {
      enabled: agentSettings.collaboration_enabled,
    },
    compliance: {
      mode: agentSettings.compliance_mode,
      retention_days: agentSettings.compliance_retention_days,
      sign_all_events: agentSettings.compliance_sign_all_events,
    },
    tool_synthesis: {
      enabled: agentSettings.tool_synthesis_enabled,
      require_activation: agentSettings.tool_synthesis_require_activation,
      max_generated_tools: agentSettings.tool_synthesis_max_generated_tools,
    },
  };
}
