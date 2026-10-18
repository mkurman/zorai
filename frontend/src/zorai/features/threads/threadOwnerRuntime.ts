import type { ConciergeConfig } from "@/lib/agentStore";
import type { AgentSettings } from "@/lib/agentStore/settings";
import type { AgentProviderId, AgentThread, SubAgentDefinition } from "@/lib/agentStore";
import { getDefaultModelForProvider } from "@/lib/agentStore/providers";
import { isSvarogOwner, RAROG_AGENT_ID, resolveThreadOwnerAgentId } from "./threadOwner";

export type ThreadOwnerRuntimeProfile = {
  ownerId: string;
  provider: string;
  model: string;
  effort: string;
  contextWindowTokens: number;
};

type ProviderRuntimeConfig = {
  model?: string;
  custom_model_name?: string;
  context_window_tokens?: number | null;
};

export function draftThreadForOwnerSnapshot(
  owner: { agentId?: string | null; agentName?: string | null },
  fallbackAgentName: string,
): AgentThread {
  return {
    id: "draft",
    daemonThreadId: null,
    workspaceId: null,
    surfaceId: null,
    paneId: null,
    agent_name: owner.agentName?.trim() || fallbackAgentName,
    targetAgentId: owner.agentId?.trim() || null,
    title: "New Conversation",
    createdAt: 0,
    updatedAt: 0,
    messageCount: 0,
    totalInputTokens: 0,
    totalOutputTokens: 0,
    totalTokens: 0,
    compactionCount: 0,
    lastMessagePreview: "",
  };
}

export function snapshotThreadOwnerRuntimeProfile(
  thread: AgentThread,
  subAgents: SubAgentDefinition[],
  agentSettings: AgentSettings,
  conciergeConfig: Pick<ConciergeConfig, "provider" | "model" | "reasoning_effort">,
): Pick<AgentThread, "profileProvider" | "profileModel" | "profileReasoningEffort" | "profileContextWindowTokens"> {
  const profile = resolveThreadOwnerRuntimeProfile(thread, subAgents, agentSettings, conciergeConfig);
  return {
    profileProvider: profile.provider.trim() || null,
    profileModel: profile.model.trim() || null,
    profileReasoningEffort: profile.effort.trim() || null,
    profileContextWindowTokens: profile.contextWindowTokens > 0 ? Math.trunc(profile.contextWindowTokens) : null,
  };
}

export function resolveThreadOwnerRuntimeProfile(
  thread: AgentThread,
  subAgents: SubAgentDefinition[],
  agentSettings: AgentSettings,
  conciergeConfig: Pick<ConciergeConfig, "provider" | "model" | "reasoning_effort">,
): ThreadOwnerRuntimeProfile {
  const ownerId = resolveThreadOwnerAgentId(thread, subAgents);
  const inherited = svarogRuntimeProfile(agentSettings, ownerId);
  const configured = configuredOwnerRuntimeProfile(ownerId, subAgents, agentSettings, conciergeConfig)
    ?? inherited;
  const upstreamProvider = typeof thread.upstreamProvider === "string" ? thread.upstreamProvider.trim() : "";
  const upstreamModel = typeof thread.upstreamModel === "string" ? thread.upstreamModel.trim() : "";
  return {
    ownerId,
    provider: thread.profileProvider?.trim() || upstreamProvider || configured.provider,
    model: thread.profileModel?.trim() || upstreamModel || configured.model,
    effort: thread.profileReasoningEffort?.trim() || configured.effort,
    contextWindowTokens: typeof thread.profileContextWindowTokens === "number" && thread.profileContextWindowTokens > 0
      ? Math.trunc(thread.profileContextWindowTokens)
      : configured.contextWindowTokens,
  };
}

function configuredOwnerRuntimeProfile(
  ownerId: string,
  subAgents: SubAgentDefinition[],
  agentSettings: AgentSettings,
  conciergeConfig: Pick<ConciergeConfig, "provider" | "model" | "reasoning_effort">,
): ThreadOwnerRuntimeProfile | null {
  if (isSvarogOwner(ownerId)) {
    return svarogRuntimeProfile(agentSettings, ownerId);
  }
  if (ownerId === RAROG_AGENT_ID) {
    const provider = conciergeConfig.provider?.trim();
    const model = conciergeConfig.model?.trim();
    if (provider && model) {
      return {
        ownerId,
        provider,
        model,
        effort: conciergeConfig.reasoning_effort?.trim() || "none",
        contextWindowTokens: contextWindowForProvider(agentSettings, provider),
      };
    }
    return svarogRuntimeProfile(agentSettings, ownerId);
  }

  const entry = findSubAgent(ownerId, subAgents);
  if (entry) {
    const provider = entry.provider?.trim();
    const model = entry.model?.trim();
    if (provider && model) {
      return {
        ownerId,
        provider,
        model,
        effort: entry.reasoning_effort?.trim() || "none",
        contextWindowTokens: entry.context_window_tokens && entry.context_window_tokens > 0
          ? Math.trunc(entry.context_window_tokens)
          : contextWindowForProvider(agentSettings, provider),
      };
    }
  }

  if (ownerId === "weles") {
    const weles = agentSettings.compaction.weles;
    if (weles.provider && weles.model) {
      return {
        ownerId,
        provider: weles.provider,
        model: weles.model,
        effort: weles.reasoning_effort || "none",
        contextWindowTokens: contextWindowForProvider(agentSettings, weles.provider),
      };
    }
  }

  return null;
}

function svarogRuntimeProfile(agentSettings: AgentSettings, ownerId: string): ThreadOwnerRuntimeProfile {
  const provider = String(agentSettings.active_provider || "").trim();
  const config = providerConfig(agentSettings, provider);
  const model = (config?.custom_model_name || config?.model || "").trim()
    || (provider ? getDefaultModelForProvider(provider as AgentProviderId) : "");
  return {
    ownerId,
    provider,
    model,
    effort: agentSettings.reasoning_effort || "none",
    contextWindowTokens: contextWindowForProvider(agentSettings, provider),
  };
}

function findSubAgent(ownerId: string, subAgents: SubAgentDefinition[]): SubAgentDefinition | undefined {
  const wanted = ownerId.trim().toLowerCase().replace(/_builtin$/, "");
  return subAgents.find((entry) => {
    const entryId = (entry.id ?? "").trim().toLowerCase().replace(/_builtin$/, "");
    const entryName = (entry.name ?? "").trim().toLowerCase();
    return entryId === wanted || entryName === wanted;
  });
}

function providerConfig(agentSettings: AgentSettings, providerId: string): ProviderRuntimeConfig | undefined {
  if (!providerId) return undefined;
  const value = agentSettings[providerId as AgentProviderId];
  return value && typeof value === "object" ? value as ProviderRuntimeConfig : undefined;
}

function contextWindowForProvider(agentSettings: AgentSettings, providerId: string): number {
  const providerWindow = providerConfig(agentSettings, providerId)?.context_window_tokens;
  if (typeof providerWindow === "number" && providerWindow > 0) {
    return Math.trunc(providerWindow);
  }
  return Math.max(1, Math.trunc(agentSettings.context_window_tokens || 256000));
}
