import type { AgentContentBlock, AgentMessage } from "./types";

export const MAX_RENDERER_TEXT_CHARS = 100_000;
export const MAX_RENDERER_THREAD_CHARS = 8_000_000;
export const MAX_RENDERER_STORE_CHARS = 24_000_000;

const TRUNCATION_MARKER = "\n[Truncated to keep the app within its memory limit]";

export function boundRendererText(value: string | null | undefined): string {
  const text = typeof value === "string" ? value : "";
  if (text.length <= MAX_RENDERER_TEXT_CHARS) return text;
  return `${text.slice(0, MAX_RENDERER_TEXT_CHARS)}${TRUNCATION_MARKER}`;
}

export function boundOptionalRendererText(value: string | null | undefined): string | undefined {
  if (typeof value !== "string") return undefined;
  return boundRendererText(value);
}

export function boundContentBlocks(
  blocks: AgentContentBlock[] | undefined,
): AgentContentBlock[] | undefined {
  if (!blocks || blocks.length === 0) return blocks;
  let changed = false;
  const next = blocks.map((block) => {
    if (block.type === "text") {
      const text = boundRendererText(block.text);
      if (text === block.text) return block;
      changed = true;
      return { ...block, text };
    }
    if (typeof block.data_url === "string" && block.data_url.length > MAX_RENDERER_TEXT_CHARS) {
      changed = true;
      return { ...block, data_url: undefined };
    }
    return block;
  });
  return changed ? next : blocks;
}

export function messagePayloadChars(message: Pick<
  AgentMessage,
  "content" | "toolArguments" | "reasoning" | "compactionPayload" | "contentBlocks"
>): number {
  let total = message.content?.length ?? 0;
  total += message.toolArguments?.length ?? 0;
  total += message.reasoning?.length ?? 0;
  total += message.compactionPayload?.length ?? 0;
  for (const block of message.contentBlocks ?? []) {
    if (block.type === "text") {
      total += block.text.length;
    } else {
      total += block.data_url?.length ?? 0;
      total += block.url?.length ?? 0;
    }
  }
  return total;
}

export function capMessagesToCharBudget<T extends AgentMessage>(messages: T[], budget: number): T[] {
  if (budget <= 0 || messages.length === 0) return messages;
  let total = 0;
  for (const message of messages) total += messagePayloadChars(message);
  if (total <= budget) return messages;
  let start = 0;
  while (start < messages.length - 1 && total > budget) {
    total -= messagePayloadChars(messages[start]);
    start += 1;
  }
  return messages.slice(start);
}

export function applyRendererMessageBudget(
  messages: Record<string, AgentMessage[]>,
  activeThreadId: string | null,
): Record<string, AgentMessage[]> {
  let changed = false;
  const next: Record<string, AgentMessage[]> = {};
  const chars = new Map<string, number>();
  let total = 0;
  for (const [threadId, list] of Object.entries(messages)) {
    const capped = capMessagesToCharBudget(list, MAX_RENDERER_THREAD_CHARS);
    if (capped !== list) changed = true;
    const size = capped.reduce((sum, message) => sum + messagePayloadChars(message), 0);
    chars.set(threadId, size);
    total += size;
    next[threadId] = capped;
  }
  if (total > MAX_RENDERER_STORE_CHARS) {
    const inactive = Object.keys(next)
      .filter((threadId) => threadId !== activeThreadId)
      .sort((left, right) => (chars.get(right) ?? 0) - (chars.get(left) ?? 0));
    for (const threadId of inactive) {
      if (total <= MAX_RENDERER_STORE_CHARS) break;
      const size = chars.get(threadId) ?? 0;
      if (size === 0) continue;
      next[threadId] = [];
      total -= size;
      changed = true;
    }
  }
  if (activeThreadId && total > MAX_RENDERER_STORE_CHARS) {
    const capped = capMessagesToCharBudget(next[activeThreadId] ?? [], MAX_RENDERER_STORE_CHARS);
    if (capped !== next[activeThreadId]) {
      next[activeThreadId] = capped;
      changed = true;
    }
  }
  return changed ? next : messages;
}
