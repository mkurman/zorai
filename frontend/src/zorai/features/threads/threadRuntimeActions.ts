import { getDefaultModelForProvider } from "@/lib/agentStore/providers";
import { useAgentStore } from "@/lib/agentStore";
import type { AgentProviderId, AgentThread } from "@/lib/agentStore";
import { MANAGED_SECURITY_LEVELS } from "@/lib/agentStore/settings";
import { getBridge } from "@/lib/bridge";
import {
  getAgentDbApi,
  persistDaemonThreadMap,
  serializeThread,
  shouldPersistHistory,
} from "@/lib/agentStore/history";
import {
  clearThreadRuntimeOverlay,
  pinThreadRuntimeOverlay,
  snapshotThreadRuntimeOverlay,
} from "@/lib/agentStore/threadProfileMerge";
import { RAROG_AGENT_ID } from "./threadOwner";

export const MIN_CONTEXT_WINDOW_TOKENS = 1_000;
export const MAX_CONTEXT_WINDOW_TOKENS = 2_000_000;
const REASONING_EFFORTS = ["none", "minimal", "low", "medium", "high", "xhigh", "max"] as const;

export type ThreadReasoningEffort = (typeof REASONING_EFFORTS)[number];
export type ThreadManagedSecurityLevel = (typeof MANAGED_SECURITY_LEVELS)[number];

export function clampContextWindowTokens(tokens: number): number {
  if (!Number.isFinite(tokens)) return MIN_CONTEXT_WINDOW_TOKENS;
  return Math.min(MAX_CONTEXT_WINDOW_TOKENS, Math.max(MIN_CONTEXT_WINDOW_TOKENS, Math.trunc(tokens)));
}

export function threadProviderIds(currentProvider?: string): string[] {
  const settings = useAgentStore.getState().agentSettings;
  const ids = Object.keys(settings)
    .filter((key) => {
      const value = settings[key];
      return value && typeof value === "object" && "model" in value && "base_url" in value;
    })
    .sort();
  return currentProvider && !ids.includes(currentProvider) ? [currentProvider, ...ids] : ids;
}

export function modelForThreadProviderChange(providerId: string, fallbackModel: string): string {
  const settings = useAgentStore.getState().agentSettings;
  const configured = (settings[providerId as AgentProviderId] as { model?: string } | undefined)?.model?.trim();
  if (configured) return configured;
  return getDefaultModelForProvider(providerId as AgentProviderId) || fallbackModel;
}

export function threadReasoningEfforts(): readonly ThreadReasoningEffort[] {
  return REASONING_EFFORTS;
}

export function managedSecurityLevels(): readonly ThreadManagedSecurityLevel[] {
  return MANAGED_SECURITY_LEVELS;
}

export function patchThreadProfile(
  threadId: string,
  patch: Partial<Pick<AgentThread, "profileProvider" | "profileModel" | "profileReasoningEffort" | "profileContextWindowTokens">>,
): void {
  useAgentStore.setState((state) => {
    let updatedThread = null as AgentThread | null;
    const threads = state.threads.map((thread) => {
      if (thread.id !== threadId) return thread;
      updatedThread = { ...thread, ...patch };
      return updatedThread;
    });
    if (!updatedThread) return state;
    if (shouldPersistHistory(state.agentSettings.agent_backend)) {
      persistDaemonThreadMap(threads);
      void getAgentDbApi()?.dbCreateThread?.(serializeThread(updatedThread));
    }
    return { threads };
  });
}

function daemonEffortValue(effort: string): string {
  return effort === "none" ? "" : effort;
}

function resolveDaemonThreadIdForRuntime(thread: AgentThread): string | null {
  const state = useAgentStore.getState();
  const fromStore = state.threads.find((entry) => entry.id === thread.id)?.daemonThreadId?.trim();
  if (fromStore) return fromStore;
  const fromThread = thread.daemonThreadId?.trim();
  return fromThread || null;
}

async function patchDaemonThreadExecutionProfile(
  thread: AgentThread,
  patch: Record<string, unknown>,
): Promise<void> {
  const daemonThreadId = resolveDaemonThreadIdForRuntime(thread);
  if (!daemonThreadId) {
    return;
  }
  const bridge = getBridge();
  const existing = await bridge?.agentGetThreadExecutionProfile?.(daemonThreadId).catch(() => null) as
    | { profile?: Record<string, unknown> | null }
    | null
    | undefined;
  const previous = existing?.profile && typeof existing.profile === "object" ? existing.profile : {};
  const result = await bridge?.agentSetThreadExecutionProfile?.(daemonThreadId, {
    ...previous,
    ...patch,
  }) as { error?: string } | undefined;
  if (result?.error) throw new Error(result.error);
}

export async function applyThreadProviderModel(
  thread: AgentThread,
  providerId: string,
  model: string,
  contextWindowTokens?: number,
): Promise<void> {
  const nextModel = model.trim() || getDefaultModelForProvider(providerId as AgentProviderId);
  const knownContextWindow = contextWindowTokens && contextWindowTokens > 0
    ? clampContextWindowTokens(contextWindowTokens)
    : null;
  const previous = snapshotThreadRuntimeOverlay(thread);
  const overlay = {
    ...previous,
    profileProvider: providerId,
    profileModel: nextModel,
    ...(knownContextWindow ? { profileContextWindowTokens: knownContextWindow } : {}),
  };
  pinThreadRuntimeOverlay(thread.id, overlay);
  patchThreadProfile(thread.id, overlay);
  try {
    await patchDaemonThreadExecutionProfile(thread, {
      provider: providerId,
      model: nextModel,
      ...(knownContextWindow ? { context_window_tokens: knownContextWindow } : {}),
    });
  } catch (error) {
    restoreThreadRuntimeOverlay(thread.id, previous);
    throw error;
  }
}

export async function applyThreadReasoningEffort(thread: AgentThread, effort: string): Promise<void> {
  const daemonEffort = daemonEffortValue(effort);
  const previous = snapshotThreadRuntimeOverlay(thread);
  const overlay = {
    ...previous,
    profileReasoningEffort: daemonEffort || null,
  };
  pinThreadRuntimeOverlay(thread.id, overlay);
  patchThreadProfile(thread.id, overlay);
  try {
    await patchDaemonThreadExecutionProfile(thread, {
      reasoning_effort: daemonEffort || null,
    });
  } catch (error) {
    restoreThreadRuntimeOverlay(thread.id, previous);
    throw error;
  }
}

export async function applyThreadContextWindow(thread: AgentThread, tokens: number): Promise<void> {
  const clamped = clampContextWindowTokens(tokens);
  const previous = snapshotThreadRuntimeOverlay(thread);
  const overlay = {
    ...previous,
    profileContextWindowTokens: clamped,
  };
  pinThreadRuntimeOverlay(thread.id, overlay);
  patchThreadProfile(thread.id, overlay);
  try {
    await patchDaemonThreadExecutionProfile(thread, {
      context_window_tokens: clamped,
    });
  } catch (error) {
    restoreThreadRuntimeOverlay(thread.id, previous);
    throw error;
  }
}

function restoreThreadRuntimeOverlay(
  threadId: string,
  previous: ReturnType<typeof snapshotThreadRuntimeOverlay>,
): void {
  if (previous.profileProvider || previous.profileModel) {
    pinThreadRuntimeOverlay(threadId, previous);
  } else {
    clearThreadRuntimeOverlay(threadId);
  }
  patchThreadProfile(threadId, previous);
}

export async function applyManagedSecurityLevel(level: ThreadManagedSecurityLevel): Promise<void> {
  useAgentStore.getState().updateAgentSetting("managed_security_level", level);
  const bridge = getBridge();
  await bridge?.agentSetConfigItem?.("/managed_execution/security_level", level);
  await bridge?.agentSetConfigItem?.("/managed_security_level", level);
}

export async function applyRarogContextWindow(tokens: number): Promise<void> {
  const clamped = clampContextWindowTokens(tokens);
  await getBridge()?.agentSetTargetAgentContextWindow?.(RAROG_AGENT_ID, clamped);
}
