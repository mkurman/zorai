import { isCompactionArtifactMessage } from "@/components/agent-chat-panel/chat-view/compactionArtifact";
import { assistantMessageHasVisibleContent } from "@/components/agent-chat-panel/chat-view/helpers";
import type { ChatDisplayItem, ToolEventGroup } from "@/components/agent-chat-panel/chat-view/types";
import type { AgentMessage } from "@/lib/agentStore";
import { geminiThoughtSignature, visibleGeminiReasoning } from "@/lib/geminiThought";

export type TurnActivityEntry =
  | { kind: "tools"; key: string; groups: ToolEventGroup[] }
  | { kind: "reasoning"; key: string; message: AgentMessage; thoughtMs?: number }
  | { kind: "notice"; key: string; message: AgentMessage };

export type ThreadDisplayItem =
  | ChatDisplayItem
  | { type: "turnActivity"; key: string; entries: TurnActivityEntry[]; messageIds: string[] };

const withoutReasoning = new WeakMap<AgentMessage, AgentMessage>();

function withoutReasoningCopy(message: AgentMessage): AgentMessage {
  let stripped = withoutReasoning.get(message);
  if (!stripped) {
    stripped = { ...message, reasoning: undefined };
    withoutReasoning.set(message, stripped);
  }
  return stripped;
}

function isVisibleMessage(item: ChatDisplayItem, isActivity: (message: AgentMessage) => boolean): boolean {
  if (item.type !== "message") return false;
  const message = item.message;
  if (message.role === "user" || isCompactionArtifactMessage(message)) return true;
  return message.role !== "system"
    && !isActivity(message)
    && (message.role !== "assistant" || assistantMessageHasVisibleContent(message.content));
}

export function providerHidReasoning(message: AgentMessage): boolean {
  return message.role === "assistant"
    && !message.isStreaming
    && !visibleGeminiReasoning(message.reasoning)
    && (Boolean(geminiThoughtSignature(message.reasoning)) || (message.reasoningTokens ?? 0) > 0);
}

export type LiveModelThinking = { startedAt: number; reasoning: string };

export function liveModelThinking(messages: AgentMessage[]): LiveModelThinking | null {
  const last = messages[messages.length - 1];
  if (!last) return null;
  if (last.role === "user") return { startedAt: last.createdAt, reasoning: "" };
  if (last.role === "assistant") {
    const thinkingOnly = last.isStreaming && !last.content?.trim() && !(last.toolCalls?.length);
    if (!thinkingOnly) return null;
    const previous = messages[messages.length - 2];
    return {
      startedAt: previous?.createdAt ?? last.createdAt,
      reasoning: visibleGeminiReasoning(last.reasoning),
    };
  }
  if (last.role !== "tool") return null;
  let index = messages.length - 1;
  while (index >= 0 && messages[index].role === "tool") {
    const status = messages[index].toolStatus;
    if (status !== "done" && status !== "error") return null;
    index -= 1;
  }
  return { startedAt: last.createdAt, reasoning: "" };
}

export function groupTurnActivity(
  items: ChatDisplayItem[],
  isActivity: (message: AgentMessage) => boolean,
  liveThinkingStartedAt: number | null = null,
): ThreadDisplayItem[] {
  const result: ThreadDisplayItem[] = [];
  let entries: TurnActivityEntry[] = [];
  let messageIds: string[] = [];
  let previousAt = 0;
  const reasoningEntry = (message: AgentMessage): TurnActivityEntry => {
    const thoughtMs = providerHidReasoning(message) && previousAt > 0 ? message.createdAt - previousAt : 0;
    return thoughtMs > 0
      ? { kind: "reasoning", key: `${message.id}:reasoning`, message, thoughtMs }
      : { kind: "reasoning", key: `${message.id}:reasoning`, message };
  };

  const flush = () => {
    if (entries.length > 0) {
      result.push({ type: "turnActivity", key: entries[0].key, entries, messageIds });
    }
    entries = [];
    messageIds = [];
  };

  for (const item of items) {
    if (item.type === "toolList") {
      entries.push({ kind: "tools", key: item.key, groups: item.groups });
      previousAt = Math.max(previousAt, ...item.groups.map((group) => group.createdAt));
      continue;
    }
    if (item.type === "tool") {
      entries.push({ kind: "tools", key: item.group.key, groups: [item.group] });
      previousAt = Math.max(previousAt, item.group.createdAt);
      continue;
    }
    const message = item.message;
    const hasReasoning = message.role === "assistant" && (Boolean(visibleGeminiReasoning(message.reasoning)) || providerHidReasoning(message));
    if (isVisibleMessage(item, isActivity)) {
      if (hasReasoning) {
        entries.push(reasoningEntry(message));
      }
      flush();
      result.push({ type: "message", message: hasReasoning ? withoutReasoningCopy(message) : message });
      previousAt = Math.max(previousAt, message.createdAt);
      continue;
    }
    if (hasReasoning) {
      entries.push(reasoningEntry(message));
    } else if (isActivity(message)) {
      insertNoticeInTimeOrder(entries, { kind: "notice", key: message.id, message });
    } else {
      previousAt = Math.max(previousAt, message.createdAt);
      continue;
    }
    previousAt = Math.max(previousAt, message.createdAt);
    messageIds.push(message.id);
  }
  if (liveThinkingStartedAt !== null) {
    entries.push({
      kind: "reasoning",
      key: "live-thinking",
      message: { id: "live-thinking", threadId: "", role: "assistant", content: "", createdAt: liveThinkingStartedAt, isStreaming: true } as AgentMessage,
    });
  }
  flush();
  return result;
}

function insertNoticeInTimeOrder(entries: TurnActivityEntry[], notice: TurnActivityEntry): void {
  const tools = entries[entries.length - 1];
  if (notice.kind !== "notice" || tools?.kind !== "tools") {
    entries.push(notice);
    return;
  }
  const split = tools.groups.findIndex((group) => group.createdAt > notice.message.createdAt);
  if (split < 0) {
    entries.push(notice);
    return;
  }
  const later = tools.groups.slice(split);
  const laterEntry: TurnActivityEntry = { kind: "tools", key: `${tools.key}:${later[0].key}`, groups: later };
  if (split === 0) {
    entries.splice(entries.length - 1, 1, notice, { ...laterEntry, key: tools.key });
    return;
  }
  entries.splice(entries.length - 1, 1, { ...tools, groups: tools.groups.slice(0, split) }, notice, laterEntry);
}
