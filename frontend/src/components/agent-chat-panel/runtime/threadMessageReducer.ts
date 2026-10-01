import { canonicalizeHydratedToolCalls } from "@/lib/agentStore";
import type { AgentMessage } from "@/lib/agentStore";

const EQUIVALENT_USER_WINDOW_MS = 120_000;

/**
 * Reconcile a daemon-owned page with the renderer's current timeline.
 *
 * Authoritative rows own identity and persisted fields. Existing rows retain
 * richer local media and pending/live state until an authoritative equivalent
 * arrives. The result is deterministic regardless of whether hydration starts
 * before or after an optimistic/queued send.
 */
export function reconcileThreadMessages(
  existing: readonly AgentMessage[],
  authoritative: readonly AgentMessage[],
): AgentMessage[] {
  const authoritativeById = new Map(authoritative.map((message) => [message.id, message]));
  const consumedAuthoritativeIds = new Set<string>();
  const reconciled: AgentMessage[] = [];

  existing.forEach((local) => {
    const exact = authoritativeById.get(local.id);
    if (exact) {
      consumedAuthoritativeIds.add(exact.id);
      reconciled.push(mergeMessageRichness(exact, local));
      return;
    }

    const equivalent = isOptimisticMessage(local)
      ? authoritative.find((remote) =>
        !consumedAuthoritativeIds.has(remote.id) && messagesAreEquivalent(local, remote))
      : undefined;
    if (equivalent) {
      consumedAuthoritativeIds.add(equivalent.id);
      reconciled.push(mergeMessageRichness(equivalent, local));
      return;
    }

    if (isStaleLocalTool(local, authoritative)) return;
    if (isStaleLocalAssistantShell(local)) return;
    reconciled.push(local);
  });

  authoritative.forEach((remote) => {
    if (consumedAuthoritativeIds.has(remote.id)) return;
    const insertionIndex = reconciled.findIndex((current) =>
      normalizeTimestamp(current.createdAt) > normalizeTimestamp(remote.createdAt)
    );
    if (insertionIndex < 0) {
      reconciled.push(remote);
    } else {
      reconciled.splice(insertionIndex, 0, remote);
    }
  });

  return canonicalizeHydratedToolCalls(dedupeIds(reconciled));
}

function dedupeIds(messages: AgentMessage[]): AgentMessage[] {
  const seen = new Set<string>();
  return messages.filter((message) => {
    if (seen.has(message.id)) return false;
    seen.add(message.id);
    return true;
  });
}

export function isLiveLocalThreadMessage(message: AgentMessage): boolean {
  return message.isStreaming === true || isOptimisticMessage(message);
}

export function retainLiveLocalMessages(messages: readonly AgentMessage[]): AgentMessage[] {
  return messages.filter(isLiveLocalThreadMessage);
}

function isOptimisticMessage(message: AgentMessage): boolean {
  return message.id.startsWith("queued-prompt:") || /^msg_\d+$/.test(message.id);
}

function messagesAreEquivalent(left: AgentMessage, right: AgentMessage): boolean {
  if (left.role !== right.role) return false;
  if (left.role === "user") {
    return left.content.trim() === right.content.trim()
      && Math.abs(normalizeTimestamp(left.createdAt) - normalizeTimestamp(right.createdAt))
        <= EQUIVALENT_USER_WINDOW_MS;
  }
  if (left.role === "assistant") {
    const hasIdentity = Boolean(left.content.trim() || left.reasoning?.trim());
    return hasIdentity
      && left.content === right.content
      && (left.reasoning ?? "") === (right.reasoning ?? "");
  }
  return false;
}

function mergeMessageRichness(authoritative: AgentMessage, local: AgentMessage): AgentMessage {
  const authoritativeBlocks = authoritative.contentBlocks ?? [];
  const localBlocks = local.contentBlocks ?? [];
  return {
    ...local,
    ...authoritative,
    contentBlocks: authoritativeBlocks.length > 0
      ? authoritativeBlocks
      : localBlocks.length > 0
      ? localBlocks
      : undefined,
    reasoning: authoritative.reasoning || local.reasoning,
    providerFinalResult: authoritative.providerFinalResult ?? local.providerFinalResult,
  };
}

function isStaleLocalAssistantShell(message: AgentMessage): boolean {
  return isOptimisticMessage(message)
    && message.role === "assistant"
    && !message.isStreaming
    && !message.content.trim()
    && !message.reasoning?.trim();
}

function isStaleLocalTool(message: AgentMessage, authoritative: readonly AgentMessage[]): boolean {
  if (message.role !== "tool") return false;
  const timestamp = normalizeTimestamp(message.createdAt);
  return authoritative.some((candidate) =>
    candidate.role === "user" && normalizeTimestamp(candidate.createdAt) > timestamp
  );
}

function normalizeTimestamp(timestamp: number): number {
  return timestamp < 10_000_000_000 ? timestamp * 1_000 : timestamp;
}
