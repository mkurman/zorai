import { getEffectiveContextWindow, normalizeAgentProviderId } from "./agentStore/providers.ts";
import type { ContextCompactionSettings } from "./agent-client/types.ts";

export const MIN_CONTEXT_TARGET_TOKENS = 1024;

function resolveWelesCompactionWindow(
  settings: ContextCompactionSettings,
  primaryWindow: number,
): number {
  const provider = settings.compaction?.weles?.provider?.trim();
  const model = settings.compaction?.weles?.model?.trim();
  if (!provider || !model) {
    return primaryWindow;
  }

  return getEffectiveContextWindow(normalizeAgentProviderId(provider), {
    model,
    custom_model_name: "",
    context_window_tokens: null,
    auth_source: "api_key",
  });
}

export function resolveCompactionTargetTokens(
  settings: ContextCompactionSettings,
): number {
  const primaryWindow = Math.max(1, Number(settings.context_window_tokens || 256000));
  if (!settings.auto_compact_context) {
    return primaryWindow;
  }

  const thresholdPercent = Math.min(
    100,
    Math.max(1, Number(settings.compact_threshold_pct || 80)),
  );
  const primaryTarget = Math.floor((primaryWindow * thresholdPercent) / 100);
  const strategy = settings.compaction?.strategy ?? "heuristic";
  const strategyCap = strategy === "weles"
    ? Math.floor((resolveWelesCompactionWindow(settings, primaryWindow) * thresholdPercent) / 100)
    : strategy === "custom_model"
      ? Math.floor(
        (
          Math.max(
            1,
            Number(settings.compaction?.custom_model?.context_window_tokens || primaryWindow),
          ) * thresholdPercent
        ) / 100,
      )
      : primaryTarget;

  return Math.max(
    MIN_CONTEXT_TARGET_TOKENS,
    Math.min(primaryTarget, strategyCap),
  );
}
