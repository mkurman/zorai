import { useSyncExternalStore } from "react";

export type ThreadContextBreakdown = {
  context_window_tokens: number;
  compaction_threshold_tokens: number;
  messages_tokens: number;
  compaction_summary_tokens: number;
  system_prompt_tokens: number;
  memory_tokens: number;
  skills_tokens: number;
  session_state_tokens: number;
  tools_tokens: number;
  tools_count: number;
  deferred_tools_count: number;
  input_tokens: number;
  output_tokens: number;
  cached_input_tokens: number;
};

export type ContextBreakdownRow = {
  key: string;
  label: string;
  tokens: number;
  share: number;
  filled: boolean;
};

const breakdowns = new Map<string, ThreadContextBreakdown>();
const listeners = new Set<() => void>();

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function handleContextWindowUpdateEvent(event: { thread_id?: unknown; context_breakdown?: unknown }): void {
  const daemonThreadId = typeof event.thread_id === "string" ? event.thread_id : "";
  if (!daemonThreadId) return;
  const breakdown = event.context_breakdown as ThreadContextBreakdown | null | undefined;
  if (breakdown && typeof breakdown === "object") {
    breakdowns.set(daemonThreadId, breakdown);
  } else if (!breakdowns.delete(daemonThreadId)) {
    return;
  }
  for (const listener of listeners) listener();
}

export function getThreadContextBreakdown(daemonThreadId: string | null | undefined): ThreadContextBreakdown | null {
  return daemonThreadId ? breakdowns.get(daemonThreadId) ?? null : null;
}

export function useThreadContextBreakdown(daemonThreadId: string | null | undefined): ThreadContextBreakdown | null {
  return useSyncExternalStore(
    subscribe,
    () => getThreadContextBreakdown(daemonThreadId),
    () => null,
  );
}

export function contextBreakdownUsedTokens(breakdown: ThreadContextBreakdown): number {
  return breakdown.messages_tokens
    + breakdown.compaction_summary_tokens
    + breakdown.system_prompt_tokens
    + breakdown.memory_tokens
    + breakdown.skills_tokens
    + breakdown.session_state_tokens
    + breakdown.tools_tokens;
}

export function contextBreakdownRows(breakdown: ThreadContextBreakdown): ContextBreakdownRow[] {
  const window = Math.max(1, breakdown.context_window_tokens);
  const threshold = Math.min(window, Math.max(0, breakdown.compaction_threshold_tokens));
  const used = contextBreakdownUsedTokens(breakdown);
  const row = (key: string, label: string, tokens: number, filled = true): ContextBreakdownRow => ({
    key,
    label,
    tokens,
    share: tokens / window,
    filled,
  });
  const rows = [
    row("messages", "Messages", breakdown.messages_tokens),
    row("summary", "Compaction summary", breakdown.compaction_summary_tokens),
    row("tools", `Tools · ${breakdown.tools_count}`, breakdown.tools_tokens),
    row("session", "Session state", breakdown.session_state_tokens),
    row("memory", "Memories", breakdown.memory_tokens),
    row("system", "System prompt", breakdown.system_prompt_tokens),
    row("skills", "Skills", breakdown.skills_tokens),
  ].filter((entry) => entry.key === "messages" || entry.tokens > 0);
  rows.push(row("free", "Free space", Math.max(0, threshold - used), false));
  if (threshold < window) {
    rows.push(row("buffer", "Compaction buffer", Math.max(0, window - Math.max(threshold, used)), false));
  }
  return rows;
}
