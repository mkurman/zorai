import type { ComposerAttachment, SendMessagePayload } from "@/components/agent-chat-panel/chat-view/types";
import type { AgentContentBlock, AgentMessage } from "@/lib/agentStore/types";
import type { AttachmentTileModel } from "./attachmentTiles";
import { splitMessageAttachments } from "./messageAttachments";

export type QueuedComposerMessage = SendMessagePayload & { id: string };

export type DaemonQueuedPromptRecord = {
  id: string;
  thread_id: string;
  content: string;
  content_blocks_json?: string | null;
};

let queuedComposerSerial = 0;

export function createQueuedComposerId(): string {
  if (typeof globalThis.crypto?.randomUUID === "function") {
    return globalThis.crypto.randomUUID();
  }
  queuedComposerSerial += 1;
  return `queued_${Date.now()}_${queuedComposerSerial}`;
}

export function createQueuedComposerMessage(payload: SendMessagePayload): QueuedComposerMessage {
  return { ...payload, id: createQueuedComposerId() };
}

export function queuedComposerLabel(payload: SendMessagePayload): string {
  const text = payload.text.trim();
  return text || "(attachment)";
}

export function queuedComposerPreview(payload: SendMessagePayload): { text: string; tiles: AttachmentTileModel[] } {
  const blocks = payload.localContentBlocks ?? queuedContentBlocks(payload.contentBlocksJson);
  const { displayText, tiles } = splitMessageAttachments(payload.text, blocks);
  return { text: displayText, tiles };
}

const QUEUED_TEXT_FILE_PATTERN = /<attached_file name="([^"]*)">\r?\n?([\s\S]*?)\r?\n?<\/attached_file>/g;
const QUEUED_PATH_FILE_PATTERN = /<attached_file name="([^"]*)" path="([^"]*)" mime_type="([^"]*)" \/>/g;

export function queuedComposerDraft(payload: QueuedComposerMessage): { text: string; attachments: ComposerAttachment[] } {
  const attachments: ComposerAttachment[] = [];
  [...payload.text.matchAll(QUEUED_TEXT_FILE_PATTERN)].forEach((match, index) => {
    attachments.push({
      id: `${payload.id}:text:${index}`,
      name: match[1],
      size: match[2].length,
      kind: "text",
      mimeType: "text/plain",
      textContent: match[2],
    });
  });
  [...payload.text.matchAll(QUEUED_PATH_FILE_PATTERN)].forEach((match, index) => {
    attachments.push({
      id: `${payload.id}:file:${index}`,
      name: match[1],
      size: 0,
      kind: "file",
      mimeType: match[3],
      path: match[2],
    });
  });
  const blocks = queuedContentBlocks(payload.contentBlocksJson) ?? payload.localContentBlocks ?? [];
  const previews = payload.localContentBlocks ?? [];
  blocks.forEach((block, index) => {
    if (block.type !== "image" && block.type !== "audio") return;
    const dataUrl = block.data_url ?? block.url;
    if (!dataUrl) return;
    const preview = previews[index];
    const previewDataUrl = preview?.type === "image" && preview.data_url !== dataUrl ? preview.data_url : undefined;
    attachments.push({
      id: `${payload.id}:${block.type}:${index}`,
      name: `${block.type}-${index + 1}`,
      size: 0,
      kind: block.type,
      mimeType: block.mime_type ?? (block.type === "image" ? "image/png" : "audio/wav"),
      dataUrl,
      previewDataUrl,
    });
  });
  const text = payload.text
    .replace(QUEUED_TEXT_FILE_PATTERN, "")
    .replace(QUEUED_PATH_FILE_PATTERN, "")
    .trim();
  return { text, attachments };
}

export function shouldApplyPromptQueueSnapshot(
  daemonEventRevisionAtRequest: number,
  currentDaemonEventRevision: number,
): boolean {
  return currentDaemonEventRevision === daemonEventRevisionAtRequest;
}

function queuedContentBlocks(contentBlocksJson: string | null | undefined): AgentContentBlock[] | undefined {
  if (!contentBlocksJson) return undefined;
  try {
    const parsed = JSON.parse(contentBlocksJson);
    return Array.isArray(parsed) ? parsed as AgentContentBlock[] : undefined;
  } catch {
    return undefined;
  }
}

export function reconcileSentQueuedPromptMessages(
  messages: AgentMessage[],
  threadId: string,
  prompt: QueuedComposerMessage,
  createdAt = Date.now(),
  insertionBoundary = messages.length,
): AgentMessage[] {
  const localMessageId = `queued-prompt:${prompt.id}`;
  if (messages.some((message) => message.id === localMessageId)) return messages;

  const contentBlocks = prompt.localContentBlocks ?? queuedContentBlocks(prompt.contentBlocksJson);
  const authoritativeIndex = messages.findIndex((message) =>
    message.role === "user"
    && !message.id.startsWith("queued-prompt:")
    && message.content.trim() === prompt.text.trim()
    && Math.abs(normalizeComposerTimestamp(message.createdAt) - normalizeComposerTimestamp(createdAt)) <= 120_000
  );
  if (authoritativeIndex >= 0) {
    if (!contentBlocks?.length || messages[authoritativeIndex].contentBlocks?.length) return messages;
    const reconciled = [...messages];
    reconciled[authoritativeIndex] = {
      ...messages[authoritativeIndex],
      contentBlocks,
    };
    return reconciled;
  }

  const userMessage: AgentMessage = {
    id: localMessageId,
    threadId,
    createdAt,
    role: "user",
    content: prompt.text,
    contentBlocks,
    inputTokens: 0,
    outputTokens: 0,
    totalTokens: 0,
    isCompactionSummary: false,
  };

  // The boundary is captured when Send now is clicked. Existing messages,
  // including the assistant being interrupted, stay before the queued prompt;
  // any new stream messages that race the IPC reply stay after it.
  const insertionIndex = Math.max(0, Math.min(insertionBoundary, messages.length));
  return [
    ...messages.slice(0, insertionIndex),
    userMessage,
    ...messages.slice(insertionIndex),
  ];
}

function normalizeComposerTimestamp(timestamp: number): number {
  return timestamp < 10_000_000_000 ? timestamp * 1_000 : timestamp;
}

export const EMPTY_PROMPT_QUEUE: QueuedComposerMessage[] = [];

export function queuedPromptsFromDaemon(raw: unknown): QueuedComposerMessage[] {
  if (!Array.isArray(raw)) return EMPTY_PROMPT_QUEUE;
  const prompts = raw.flatMap((entry) => {
    if (!entry || typeof entry !== "object") return [];
    const record = entry as DaemonQueuedPromptRecord;
    if (typeof record.id !== "string" || typeof record.content !== "string") return [];
    return [{
      id: record.id,
      text: record.content,
      contentBlocksJson: record.content_blocks_json ?? null,
    }];
  });
  return prompts.length === 0 ? EMPTY_PROMPT_QUEUE : prompts;
}

export function sameQueuedPrompts(
  left: QueuedComposerMessage[] | undefined,
  right: QueuedComposerMessage[],
): boolean {
  if (left === right) return true;
  if (!left || left.length !== right.length) return false;
  return left.every((item, index) => {
    const other = right[index];
    return other != null
      && item.id === other.id
      && item.text === other.text
      && (item.contentBlocksJson ?? null) === (other.contentBlocksJson ?? null);
  });
}

export function readPromptQueueResponse(raw: unknown): {
  threadId: string | null;
  prompts: QueuedComposerMessage[];
  error?: string;
} {
  const data = raw && typeof raw === "object" ? raw as Record<string, unknown> : {};
  return {
    threadId: typeof data.thread_id === "string" ? data.thread_id : null,
    prompts: queuedPromptsFromDaemon(data.prompts),
    error: typeof data.error === "string" ? data.error : undefined,
  };
}
