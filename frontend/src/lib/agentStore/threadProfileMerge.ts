import type { AgentThread, GenerationSettings } from "./types";

export type ThreadRuntimeOverlay = Pick<
  AgentThread,
  "profileProvider" | "profileModel" | "profileReasoningEffort" | "profileContextWindowTokens"
>;

const overlays = new Map<string, ThreadRuntimeOverlay>();
const pendingGenerationSettings = new Map<string, GenerationSettings | null>();

export function setPendingThreadGenerationSettings(threadId: string, settings: GenerationSettings | null): void {
  pendingGenerationSettings.set(threadId, settings);
}

export function pendingThreadGenerationSettings(threadId: string): GenerationSettings | null {
  return pendingGenerationSettings.get(threadId) ?? null;
}

const initialProfileTaken = new Set<string>();

export function takeInitialThreadExecutionProfile(thread: AgentThread): Record<string, unknown> | null {
  if (initialProfileTaken.has(thread.id)) return null;
  initialProfileTaken.add(thread.id);
  const generation = pendingThreadGenerationSettings(thread.id);
  pendingGenerationSettings.delete(thread.id);
  const profile = {
    provider: thread.profileProvider?.trim() || null,
    model: thread.profileModel?.trim() || null,
    reasoning_effort: thread.profileReasoningEffort?.trim() || null,
    context_window_tokens: thread.profileContextWindowTokens ?? null,
    generation_settings: generation,
  };
  return Object.values(profile).some((value) => value !== null) ? profile : null;
}

export function pinThreadRuntimeOverlay(threadId: string, overlay: ThreadRuntimeOverlay): void {
  overlays.set(threadId, overlay);
}

export function clearThreadRuntimeOverlay(threadId: string): void {
  overlays.delete(threadId);
}

export function snapshotThreadRuntimeOverlay(thread: AgentThread): ThreadRuntimeOverlay {
  return {
    profileProvider: thread.profileProvider ?? null,
    profileModel: thread.profileModel ?? null,
    profileReasoningEffort: thread.profileReasoningEffort ?? null,
    profileContextWindowTokens: thread.profileContextWindowTokens ?? null,
  };
}

export function mergeRemoteThreadProfile(
  existing: AgentThread,
  incoming: AgentThread,
): ThreadRuntimeOverlay {
  const overlay = overlays.get(existing.id);
  const incomingProvider = nonempty(incoming.profileProvider);
  const incomingModel = nonempty(incoming.profileModel);
  if (overlay) {
    const overlayProvider = nonempty(overlay.profileProvider);
    const overlayModel = nonempty(overlay.profileModel);
    const confirmed = (!overlayProvider || overlayProvider === incomingProvider)
      && (!overlayModel || overlayModel === incomingModel)
      && (incomingProvider !== null || incomingModel !== null);
    if (confirmed) {
      overlays.delete(existing.id);
      return {
        profileProvider: incoming.profileProvider ?? overlay.profileProvider ?? null,
        profileModel: incoming.profileModel ?? overlay.profileModel ?? null,
        profileReasoningEffort: incoming.profileReasoningEffort ?? overlay.profileReasoningEffort ?? null,
        profileContextWindowTokens: incoming.profileContextWindowTokens
          ?? overlay.profileContextWindowTokens
          ?? null,
      };
    }
    return {
      profileProvider: overlay.profileProvider ?? incoming.profileProvider ?? existing.profileProvider ?? null,
      profileModel: overlay.profileModel ?? incoming.profileModel ?? existing.profileModel ?? null,
      profileReasoningEffort: overlay.profileReasoningEffort
        ?? incoming.profileReasoningEffort
        ?? existing.profileReasoningEffort
        ?? null,
      profileContextWindowTokens: overlay.profileContextWindowTokens
        ?? incoming.profileContextWindowTokens
        ?? existing.profileContextWindowTokens
        ?? null,
    };
  }

  return {
    profileProvider: incomingProvider ?? existing.profileProvider ?? null,
    profileModel: incomingModel ?? existing.profileModel ?? null,
    profileReasoningEffort: nonempty(incoming.profileReasoningEffort)
      ?? existing.profileReasoningEffort
      ?? null,
    profileContextWindowTokens: incoming.profileContextWindowTokens
      ?? existing.profileContextWindowTokens
      ?? null,
  };
}

function nonempty(value: string | null | undefined): string | null {
  const trimmed = value?.trim();
  return trimmed ? trimmed : null;
}
