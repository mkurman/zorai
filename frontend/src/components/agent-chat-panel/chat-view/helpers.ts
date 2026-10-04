import type { AgentMessage, AgentTodoItem } from "../../../lib/agentStore";
import { isCompactionArtifactMessage } from "./compactionArtifact";
import { mergeToolReviewMeta } from "../toolReviewPresentation";
import type { ChatDisplayItem, ToolEventAttribution, ToolEventGroup } from "./types";

const HANDOFF_EVENT_MARKER = "[[handoff_event]]";

export type HandoffSystemEvent = {
  id?: string;
  kind?: "push" | "return";
  from_agent_id?: string;
  from_agent_name?: string;
  to_agent_id?: string;
  to_agent_name?: string;
  requested_by?: "user" | "agent";
  reason?: string;
  summary?: string;
  linked_thread_id?: string | null;
  approval_id?: string | null;
  stack_depth_before?: number;
  stack_depth_after?: number;
  created_at?: number;
};

export function parseHandoffSystemEvent(content: string): HandoffSystemEvent | null {
  if (!content.startsWith(HANDOFF_EVENT_MARKER)) {
    return null;
  }
  const payloadText = content
    .slice(HANDOFF_EVENT_MARKER.length)
    .split("\n", 1)[0]
    ?.trim();
  if (!payloadText) {
    return null;
  }
  try {
    const parsed = JSON.parse(payloadText);
    return parsed && typeof parsed === "object" ? parsed as HandoffSystemEvent : null;
  } catch {
    return null;
  }
}

type BuiltDisplayItems = {
  messages: AgentMessage[];
  items: ChatDisplayItem[];
  itemStarts: number[];
  itemEnds: number[];
};

const recentDisplayBuilds: BuiltDisplayItems[] = [];

export function buildDisplayItems(messages: AgentMessage[]): ChatDisplayItem[] {
  if (messages.length === 0) return [];

  let best: BuiltDisplayItems | null = null;
  let bestShared = 0;
  for (const candidate of recentDisplayBuilds) {
    const limit = Math.min(candidate.messages.length, messages.length);
    let shared = 0;
    while (shared < limit && candidate.messages[shared] === messages[shared]) shared += 1;
    if (shared > bestShared) {
      best = candidate;
      bestShared = shared;
    }
  }

  if (best && bestShared === messages.length && bestShared === best.messages.length) {
    return best.items;
  }

  const built = best && bestShared > 0
    ? rebuildDisplayItems(messages, best, bestShared)
    : buildDisplayItemsFrom(messages, 0, emptyDisplayBuild(messages));
  recentDisplayBuilds.unshift(built);
  if (recentDisplayBuilds.length > 4) recentDisplayBuilds.length = 4;
  return built.items;
}

function emptyDisplayBuild(messages: AgentMessage[]): BuiltDisplayItems {
  return { messages, items: [], itemStarts: [], itemEnds: [] };
}

function rebuildDisplayItems(messages: AgentMessage[], previous: BuiltDisplayItems, shared: number): BuiltDisplayItems {
  let keep = 0;
  while (keep < previous.items.length && previous.itemEnds[keep] < shared) keep += 1;
  if (keep > 0 && continuesToolRun(messages, shared)) {
    let toolListIndex = keep - 1;
    while (toolListIndex >= 0 && displayItemIsToolRunNotice(previous.items[toolListIndex])) {
      toolListIndex -= 1;
    }
    const toolList = previous.items[toolListIndex];
    if (toolList?.type === "toolList" && previous.itemEnds[toolListIndex] < shared) {
      const parkedNotices = toolListIndex < keep - 1;
      if (parkedNotices || previous.itemEnds[toolListIndex] === shared - 1) {
        keep = toolListIndex;
      }
    }
  }
  const restart = keep < previous.items.length ? previous.itemStarts[keep] : shared;
  return buildDisplayItemsFrom(messages, restart, {
    messages,
    items: previous.items.slice(0, keep),
    itemStarts: previous.itemStarts.slice(0, keep),
    itemEnds: previous.itemEnds.slice(0, keep),
  });
}

function continuesToolRun(messages: AgentMessage[], index: number): boolean {
  for (let cursor = index; cursor < messages.length; cursor += 1) {
    const message = messages[cursor];
    if (isToolRunNotice(message)) continue;
    return message.role === "tool"
      || isAssistantToolCallEnvelope(message)
      || shouldHideAssistantDisplayMessage(message);
  }
  return false;
}

function isToolRunNotice(message: AgentMessage | undefined): boolean {
  if (!message || message.role !== "system") return false;
  const content = message.content.trimStart();
  if (
    content.startsWith("Background operation finished.")
    || content.startsWith("Background operations finished.")
  ) {
    return true;
  }
  const firstLine = content.split("\n", 1)[0]?.trim() ?? "";
  return /^meta(?:-|\s)?cogniti(?:ve|on)\b/i.test(firstLine)
    && /\b(?:warning|reflection|intervention)\b/i.test(firstLine);
}

function displayItemIsToolRunNotice(item: ChatDisplayItem | undefined): boolean {
  return item?.type === "message" && isToolRunNotice(item.message);
}

function buildDisplayItemsFrom(messages: AgentMessage[], from: number, built: BuiltDisplayItems): BuiltDisplayItems {
  let groups = new Map<string, ToolEventGroup>();
  let pendingToolList: ToolEventGroup[] | null = null;
  let pendingToolListKey: string | null = null;
  let pendingToolAttribution: ToolEventAttribution | undefined;
  let pendingToolStart = -1;
  let nextToolAttribution: ToolEventAttribution | undefined;
  let deferredNotices: AgentMessage[] = [];
  let deferredNoticeIndexes: number[] = [];

  const flushToolList = (endIndex: number) => {
    if (pendingToolList && pendingToolList.length > 0) {
      built.items.push({
        type: "toolList",
        key: pendingToolListKey ?? `tool-list:${pendingToolList[0].key}`,
        groups: pendingToolList,
        attribution: pendingToolAttribution,
      });
      built.itemStarts.push(pendingToolStart);
      built.itemEnds.push(endIndex);
    }
    for (let noticeIndex = 0; noticeIndex < deferredNotices.length; noticeIndex += 1) {
      const sourceIndex = deferredNoticeIndexes[noticeIndex];
      built.items.push({ type: "message", message: deferredNotices[noticeIndex] });
      built.itemStarts.push(sourceIndex);
      built.itemEnds.push(sourceIndex);
    }
    deferredNotices = [];
    deferredNoticeIndexes = [];
    pendingToolList = null;
    pendingToolListKey = null;
    pendingToolAttribution = undefined;
    pendingToolStart = -1;
    groups = new Map<string, ToolEventGroup>();
  };

  for (let index = from; index < messages.length; index += 1) {
    const message = messages[index];
    if (isAssistantToolCallEnvelope(message)) {
      if (!pendingToolList) {
        nextToolAttribution = {
          authorAgentName: message.authorAgentName,
          createdAt: message.createdAt,
        };
        if (pendingToolStart < 0) pendingToolStart = index;
      }
      continue;
    }
    if (shouldHideAssistantDisplayMessage(message)) {
      continue;
    }

    if (
      isToolRunNotice(message)
      && (pendingToolList !== null || pendingToolStart >= 0)
      && continuesToolRun(messages, index + 1)
    ) {
      deferredNotices.push(message);
      deferredNoticeIndexes.push(index);
      continue;
    }

    if (message.role !== "tool") {
      flushToolList(index - 1);
      nextToolAttribution = undefined;
      pendingToolStart = -1;
      built.items.push({ type: "message", message });
      built.itemStarts.push(index);
      built.itemEnds.push(index);
      continue;
    }

    const groupKey = message.toolCallId || message.id;
    const existing = groups.get(groupKey);

    if (!existing) {
      const initialGroup: ToolEventGroup = {
        key: groupKey,
        toolCallId: message.toolCallId || message.id,
        toolName: message.toolName || "tool",
        toolArguments: message.toolArguments || "",
        status: message.toolStatus || (message.content ? "done" : "requested"),
        resultContent: message.content || "",
        createdAt: message.createdAt,
        welesReview: message.welesReview,
      };
      groups.set(groupKey, initialGroup);
      if (!pendingToolList) {
        pendingToolList = [];
        pendingToolListKey = `tool-list:${message.id}`;
        pendingToolAttribution = nextToolAttribution;
        nextToolAttribution = undefined;
        if (pendingToolStart < 0) pendingToolStart = index;
      }
      pendingToolList.push(initialGroup);
      continue;
    }

    if (message.toolName) existing.toolName = message.toolName;
    if (message.toolArguments) existing.toolArguments = message.toolArguments;
    if (message.toolStatus) {
      existing.status = message.toolStatus;
    } else if (message.content) {
      existing.status = "done";
    }
    if (message.content) existing.resultContent = message.content;
    existing.welesReview = mergeToolReviewMeta(existing.welesReview, message.welesReview);
    existing.createdAt = Math.min(existing.createdAt, message.createdAt);
  }

  flushToolList(messages.length - 1);
  return built;
}

export function assistantMessageHasVisibleContent(content: string): boolean {
  const text = content.trim();
  return text !== "" && text !== "Calling tools...";
}

function isAssistantToolCallEnvelope(message: AgentMessage): boolean {
  return message.role === "assistant"
    && Array.isArray(message.toolCalls)
    && message.toolCalls.length > 0;
}

function shouldHideAssistantDisplayMessage(message: AgentMessage): boolean {
  if (message.role !== "assistant") {
    return false;
  }

  if (isCompactionArtifactMessage(message)) {
    return false;
  }

  if (message.reasoning?.trim()) {
    return false;
  }

  return !assistantMessageHasVisibleContent(message.content);
}

export function filterDisplayItems(items: ChatDisplayItem[], searchQuery: string): ChatDisplayItem[] {
  const normalizedQuery = searchQuery.trim().toLowerCase();
  if (!normalizedQuery) {
    return items;
  }

  return items.filter((item) => {
    if (!normalizedQuery) {
      return true;
    }

    if (item.type === "message") {
      const message = item.message;
      return [
        message.role,
        message.content,
        message.reasoning ?? "",
        message.provider ?? "",
        message.model ?? "",
      ].join(" ").toLowerCase().includes(normalizedQuery);
    }

    if (item.type === "toolList") {
      return item.groups
        .map((group) => [
          group.toolName,
          group.toolArguments,
          group.resultContent,
          group.status,
        ].join(" "))
        .join(" ")
        .toLowerCase()
        .includes(normalizedQuery);
    }

    return [
      item.group.toolName,
      item.group.toolArguments,
      item.group.resultContent,
      item.group.status,
    ].join(" ").toLowerCase().includes(normalizedQuery);
  });
}

export function summarizeSessionUsage(messages: AgentMessage[]) {
  let totalCost = 0;
  let hasCost = false;
  let tpsSum = 0;
  let tpsCount = 0;

  for (const message of messages) {
    if (message.role !== "assistant") continue;
    if (typeof message.cost === "number" && Number.isFinite(message.cost)) {
      totalCost += message.cost;
      hasCost = true;
    }
    if (typeof message.tps === "number" && Number.isFinite(message.tps) && message.tps > 0) {
      tpsSum += message.tps;
      tpsCount += 1;
    }
  }

  return {
    hasCost,
    totalCost,
    avgTps: tpsCount > 0 ? (tpsSum / tpsCount) : undefined,
  };
}

export function buildTodoPreview(todos: AgentTodoItem[]): string {
  return todos
    .slice()
    .sort((a, b) => a.position - b.position)
    .slice(0, 2)
    .map((item) => item.content)
    .join(" • ");
}

export function todoStatusColor(status: AgentTodoItem["status"]): string {
  switch (status) {
    case "in_progress":
      return "var(--accent)";
    case "completed":
      return "var(--success)";
    case "blocked":
      return "var(--warning)";
    default:
      return "var(--text-muted)";
  }
}
