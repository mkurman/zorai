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
      entries.push({ kind: "notice", key: message.id, message });
    } else {
      continue;
    }
    messageIds.push(message.id);
  }
  flush();
  return result;
}
