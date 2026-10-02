import { useSyncExternalStore } from "react";
import { isCompactionArtifactMessage } from "@/components/agent-chat-panel/chat-view/compactionArtifact";
import { useAgentStore } from "@/lib/agentStore";
import { getBridge } from "@/lib/bridge";
import { pushToast } from "@/lib/toastStore";

export type ThreadCompactionStatus = {
  daemonThreadId: string;
  startedAt: number;
};

const statuses = new Map<string, ThreadCompactionStatus>();
const listeners = new Set<() => void>();
let statusVersion = 0;

function emit(): void {
  statusVersion += 1;
  for (const listener of listeners) listener();
}

export function subscribeThreadCompaction(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function getThreadCompactionVersion(): number {
  return statusVersion;
}

export function getThreadCompaction(daemonThreadId: string | null | undefined): ThreadCompactionStatus | null {
  if (!daemonThreadId) return null;
  return statuses.get(daemonThreadId) ?? null;
}

export function beginThreadCompaction(daemonThreadId: string | null | undefined): void {
  const id = daemonThreadId?.trim();
  if (!id || statuses.has(id)) return;
  statuses.set(id, { daemonThreadId: id, startedAt: Date.now() });
  emit();
}

export function endThreadCompaction(daemonThreadId: string | null | undefined): void {
  const id = daemonThreadId?.trim();
  if (!id || !statuses.delete(id)) return;
  emit();
}

export function messageTimeMs(createdAt: number): number {
  if (!Number.isFinite(createdAt)) return 0;
  return createdAt < 10_000_000_000 ? createdAt * 1000 : createdAt;
}

export function compactionWorkflowPhase(kind: string, message: string): "start" | "finished" | "failed" | null {
  if (kind !== "manual-compaction" && kind !== "auto-compaction") return null;
  const lower = message.toLowerCase();
  if (
    lower.includes("fail")
    || lower.includes("skipped")
    || lower.includes("no user message")
    || lower.includes("no claude session")
  ) {
    return "failed";
  }
  if (lower.includes("applied") || lower.includes("compacted")) {
    return "finished";
  }
  return "start";
}

export function localThreadIdForDaemon(daemonThreadId: string): string | null {
  const threads = useAgentStore.getState().threads;
  return threads.find((thread) => thread.daemonThreadId === daemonThreadId)?.id
    ?? threads.find((thread) => thread.id === daemonThreadId)?.id
    ?? null;
}

function appendCompactionActivity(daemonThreadId: string, message: string): void {
  const localThreadId = localThreadIdForDaemon(daemonThreadId);
  if (!localThreadId) return;
  const body = message.trim();
  if (!body) return;
  useAgentStore.getState().addMessage(localThreadId, {
    role: "system",
    content: `Compaction\n${body}`,
    inputTokens: 0,
    outputTokens: 0,
    totalTokens: 0,
    isCompactionSummary: false,
  });
}

export function compactionArtifactFromNotice(details: unknown): {
  id: string;
  content: string;
  payload: string;
  strategy: "heuristic" | "weles" | "custom_model" | undefined;
} | null {
  const parsed = typeof details === "string"
    ? (() => {
      try {
        return JSON.parse(details) as unknown;
      } catch {
        return null;
      }
    })()
    : details;
  if (!parsed || typeof parsed !== "object") return null;
  const record = parsed as Record<string, unknown>;
  const content = typeof record.artifact_content === "string" ? record.artifact_content.trim() : "";
  if (!content) return null;
  const strategy = record.artifact_strategy === "heuristic"
    || record.artifact_strategy === "weles"
    || record.artifact_strategy === "custom_model"
    ? record.artifact_strategy
    : undefined;
  return {
    id: typeof record.artifact_id === "string" && record.artifact_id.trim()
      ? record.artifact_id.trim()
      : "compaction-artifact",
    content,
    payload: typeof record.artifact_payload === "string" ? record.artifact_payload : "",
    strategy,
  };
}

function appendCompactionArtifact(
  daemonThreadId: string,
  artifact: NonNullable<ReturnType<typeof compactionArtifactFromNotice>>,
): void {
  const localThreadId = localThreadIdForDaemon(daemonThreadId);
  if (!localThreadId) return;
  const messages = useAgentStore.getState().getThreadMessages(localThreadId);
  if (messages.some((message) => (
    isCompactionArtifactMessage(message) && message.content.trim() === artifact.content
  ))) {
    return;
  }
  useAgentStore.getState().addMessage(localThreadId, {
    role: "assistant",
    content: artifact.content,
    inputTokens: 0,
    outputTokens: 0,
    totalTokens: 0,
    isCompactionSummary: true,
    messageKind: "compaction_artifact",
    compactionPayload: artifact.payload || undefined,
    compactionStrategy: artifact.strategy,
  });
}

export function threadHasFreshCompactionArtifact(daemonThreadId: string, startedAt: number): boolean {
  const localThreadId = localThreadIdForDaemon(daemonThreadId);
  if (!localThreadId) return false;
  return useAgentStore.getState().getThreadMessages(localThreadId).some((message) => (
    isCompactionArtifactMessage(message)
    && messageTimeMs(message.createdAt) >= startedAt - 5_000
  ));
}

export async function handleCompactionWorkflowNotice(
  event: { thread_id?: unknown; kind?: unknown; message?: unknown; details?: unknown },
  refresh: (daemonThreadId: string) => Promise<unknown>,
): Promise<void> {
  const daemonThreadId = typeof event.thread_id === "string" ? event.thread_id.trim() : "";
  const kind = typeof event.kind === "string" ? event.kind : "";
  const message = typeof event.message === "string" ? event.message : "";
  const phase = compactionWorkflowPhase(kind, message);
  if (!daemonThreadId || !phase) return;

  if (phase === "start") {
    beginThreadCompaction(daemonThreadId);
    return;
  }

  if (phase === "failed") {
    appendCompactionActivity(daemonThreadId, message);
    endThreadCompaction(daemonThreadId);
    return;
  }

  beginThreadCompaction(daemonThreadId);
  const startedAt = getThreadCompaction(daemonThreadId)?.startedAt ?? Date.now();
  try {
    await refresh(daemonThreadId);
  } catch {
    // The notice still has to land in the thread if the reload fails.
  }
  if (!threadHasFreshCompactionArtifact(daemonThreadId, startedAt)) {
    const artifact = compactionArtifactFromNotice(event.details);
    if (artifact) {
      appendCompactionArtifact(daemonThreadId, artifact);
    } else {
      appendCompactionActivity(daemonThreadId, message);
    }
  }
  endThreadCompaction(daemonThreadId);
}

export async function requestManualCompaction(daemonThreadId: string | null | undefined): Promise<void> {
  const id = daemonThreadId?.trim() ?? "";
  if (!id) {
    pushToast("Compact needs a daemon-linked thread — send one message first.", "info");
    return;
  }
  beginThreadCompaction(id);
  try {
    const compact = getBridge()?.agentForceCompact;
    if (!compact) {
      endThreadCompaction(id);
      pushToast("Compaction is unavailable.", "error");
      return;
    }
    const result = await compact(id);
    if (result && typeof result === "object" && "ok" in result && (result as { ok?: boolean }).ok === false) {
      endThreadCompaction(id);
      const error = (result as { error?: string }).error;
      pushToast(error || "Could not start compaction.", "error");
    }
  } catch (error) {
    endThreadCompaction(id);
    pushToast(error instanceof Error ? error.message : "Could not start compaction.", "error");
  }
}

export function useThreadCompaction(daemonThreadId: string | null | undefined): ThreadCompactionStatus | null {
  useSyncExternalStore(subscribeThreadCompaction, getThreadCompactionVersion, getThreadCompactionVersion);
  return getThreadCompaction(daemonThreadId);
}
