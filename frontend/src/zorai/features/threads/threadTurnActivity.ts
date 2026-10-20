import { isCompactionArtifactMessage } from "@/components/agent-chat-panel/chat-view/compactionArtifact";
import { assistantMessageHasVisibleContent } from "@/components/agent-chat-panel/chat-view/helpers";
import type { ChatDisplayItem, ToolEventGroup } from "@/components/agent-chat-panel/chat-view/types";
import type { AgentMessage } from "@/lib/agentStore";
import { visibleGeminiReasoning } from "@/lib/geminiThought";

export type TurnActivityEntry =
  | { kind: "tools"; key: string; groups: ToolEventGroup[] }
  | { kind: "reasoning"; key: string; message: AgentMessage }
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

export function groupTurnActivity(
  items: ChatDisplayItem[],
  isActivity: (message: AgentMessage) => boolean,
): ThreadDisplayItem[] {
  const result: ThreadDisplayItem[] = [];
  let entries: TurnActivityEntry[] = [];
  let messageIds: string[] = [];

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
      continue;
    }
    if (item.type === "tool") {
      entries.push({ kind: "tools", key: item.group.key, groups: [item.group] });
      continue;
    }
    const message = item.message;
    const hasReasoning = message.role === "assistant" && Boolean(visibleGeminiReasoning(message.reasoning));
    if (isVisibleMessage(item, isActivity)) {
      if (hasReasoning) {
        entries.push({ kind: "reasoning", key: `${message.id}:reasoning`, message });
      }
      flush();
      result.push({ type: "message", message: hasReasoning ? withoutReasoningCopy(message) : message });
      continue;
    }
    if (hasReasoning) {
      entries.push({ kind: "reasoning", key: `${message.id}:reasoning`, message });
    } else if (isActivity(message)) {
      insertNoticeInTimeOrder(entries, { kind: "notice", key: message.id, message });
    } else {
      continue;
    }
    messageIds.push(message.id);
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
