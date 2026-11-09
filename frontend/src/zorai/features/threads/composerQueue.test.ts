import { describe, expect, it } from "vitest";
import {
  EMPTY_PROMPT_QUEUE,
  createQueuedComposerMessage,
  queuedComposerDraft,
  queuedComposerLabel,
  queuedComposerPreview,
  queuedPromptsFromDaemon,
  readPromptQueueResponse,
  reconcileSentQueuedPromptMessages,
  sameQueuedPrompts,
  shouldApplyPromptQueueSnapshot,
} from "./composerQueue";
import { buildAttachmentSendPayload } from "@/components/agent-chat-panel/chat-view/composerMedia";
import { MAX_CACHED_PROMPT_QUEUES, selectThreadPromptQueue, usePromptQueueStore } from "./promptQueueStore";

describe("queued composer follow-ups", () => {
  it("keeps media blocks on a queued payload instead of flattening to text", () => {
    const queued = createQueuedComposerMessage({
      text: "",
      contentBlocksJson: '[{"type":"image","data_url":"data:image/png;base64,abc"}]',
      localContentBlocks: [{ type: "image", data_url: "data:image/png;base64,abc", mime_type: "image/png" }],
    });

    expect(queued.contentBlocksJson).toContain("image");
    expect(queued.localContentBlocks).toHaveLength(1);
    expect(queuedComposerLabel(queued)).toBe("(attachment)");
    expect(queued.id.length).toBeGreaterThan(0);
  });

  it("previews queued attachments from local blocks or daemon JSON so the queue shows what will be sent", () => {
    const local = queuedComposerPreview({
      text: "look at this",
      localContentBlocks: [{ type: "image", data_url: "data:image/png;base64,abc", mime_type: "image/png" }],
    });
    expect(local.text).toBe("look at this");
    expect(local.tiles.map((tile) => tile.kind)).toEqual(["image"]);

    const hydrated = queuedComposerPreview({
      text: '<attached_file name="notes.md">\nhello\n</attached_file>\n\nread it',
      contentBlocksJson: '[{"type":"text","text":"read it"},{"type":"audio","url":"file:///tmp/clip.wav"}]',
    });
    expect(hydrated.text).toBe("read it");
    expect(hydrated.tiles.map((tile) => [tile.kind, tile.name])).toEqual([["audio", "clip.wav"], ["text", "notes.md"]]);
  });

  it("restores attachments when editing a queued message so saving the edit does not silently drop them", () => {
    const original = buildAttachmentSendPayload("compare these", [
      { id: "a", name: "notes.md", size: 5, kind: "text", mimeType: "text/markdown", textContent: "hello" },
      { id: "b", name: "report.pdf", size: 9, kind: "file", mimeType: "application/pdf", path: "/tmp/report.pdf" },
      { id: "c", name: "shot.png", size: 3, kind: "image", mimeType: "image/png", dataUrl: "data:image/png;base64,full", previewDataUrl: "data:image/png;base64,small" },
    ]);
    const queued = { ...original, id: "q1" };

    const draft = queuedComposerDraft(queued);
    expect(draft.text).toBe("compare these");
    expect(draft.attachments.map((attachment) => attachment.kind)).toEqual(["text", "file", "image"]);
    expect(draft.attachments[2].dataUrl).toBe("data:image/png;base64,full");

    const resaved = buildAttachmentSendPayload(draft.text, draft.attachments);
    expect(resaved.text).toBe(original.text);
    expect(resaved.contentBlocksJson).toBe(original.contentBlocksJson);
  });

  it("restores attachments from daemon-hydrated queue records that only carry block JSON", () => {
    const draft = queuedComposerDraft({
      id: "q2",
      text: "listen",
      contentBlocksJson: '[{"type":"text","text":"listen"},{"type":"audio","data_url":"data:audio/wav;base64,x","mime_type":"audio/wav"}]',
    });
    expect(draft.text).toBe("listen");
    expect(draft.attachments).toEqual([expect.objectContaining({ kind: "audio", dataUrl: "data:audio/wav;base64,x", mimeType: "audio/wav" })]);
  });

  it("hydrates daemon-owned queue records into composer chips", () => {
    const prompts = queuedPromptsFromDaemon([
      {
        id: "prompt-1",
        thread_id: "thread-a",
        content: "stay on the migration",
        content_blocks_json: '[{"type":"image"}]',
      },
    ]);
    expect(prompts).toEqual([
      {
        id: "prompt-1",
        text: "stay on the migration",
        contentBlocksJson: '[{"type":"image"}]',
      },
    ]);
  });

  it("does not let an older IPC snapshot resurrect prompts cleared by a daemon event", () => {
    expect(shouldApplyPromptQueueSnapshot(4, 4)).toBe(true);
    expect(shouldApplyPromptQueueSnapshot(4, 5)).toBe(false);
  });

  it("shows a sent queued prompt before an assistant stream that raced ahead", () => {
    const interruptedAssistant = {
      id: "assistant-interrupted",
      threadId: "local-thread",
      createdAt: 5,
      role: "assistant",
      content: "old partial answer",
      inputTokens: 0,
      outputTokens: 0,
      totalTokens: 0,
      isCompactionSummary: false,
      isStreaming: true,
    } as const;
    const streamingAssistant = {
      id: "assistant-local",
      threadId: "local-thread",
      createdAt: 20,
      role: "assistant",
      content: "partial answer",
      inputTokens: 0,
      outputTokens: 0,
      totalTokens: 0,
      isCompactionSummary: false,
      isStreaming: true,
    } as const;
    const messages = reconcileSentQueuedPromptMessages(
      [interruptedAssistant, streamingAssistant],
      "local-thread",
      {
        id: "prompt-1",
        text: "send this now",
        contentBlocksJson: '[{"type":"image","data_url":"data:image/png;base64,abc"}]',
      },
      10,
      1,
    );

    expect(messages.map((message) => message.id)).toEqual([
      "assistant-interrupted",
      "queued-prompt:prompt-1",
      "assistant-local",
    ]);
    expect(messages[1]).toMatchObject({
      id: "queued-prompt:prompt-1",
      content: "send this now",
      contentBlocks: [{ type: "image" }],
    });
  });

  it("adopts an authoritative queued user row that arrives before Send now reconciliation", () => {
    const prompt = {
      id: "prompt-1",
      text: "send this now",
      contentBlocksJson: '[{"type":"image","data_url":"data:image/png;base64,abc"}]',
    };
    const authoritative = {
      id: "daemon-user-1",
      threadId: "local-thread",
      createdAt: 20,
      role: "user",
      content: "send this now",
      inputTokens: 0,
      outputTokens: 0,
      totalTokens: 0,
      isCompactionSummary: false,
    } as const;

    const messages = reconcileSentQueuedPromptMessages(
      [authoritative],
      "local-thread",
      prompt,
      21,
      0,
    );

    expect(messages).toHaveLength(1);
    expect(messages[0]).toMatchObject({
      id: "daemon-user-1",
      contentBlocks: [{ type: "image" }],
    });
  });

  it("does not duplicate a sent queued prompt during repeated reconciliation", () => {
    const prompt = { id: "prompt-1", text: "send once" };
    const once = reconcileSentQueuedPromptMessages([], "local-thread", prompt, 10);
    const twice = reconcileSentQueuedPromptMessages(once, "local-thread", prompt, 11);
    expect(twice).toBe(once);
    expect(twice).toHaveLength(1);
  });

  it("reads a prompt-queue IPC payload without mixing other threads", () => {
    const parsed = readPromptQueueResponse({
      thread_id: "thread-a",
      prompts: [{ id: "prompt-1", thread_id: "thread-a", content: "later" }],
    });
    expect(parsed.threadId).toBe("thread-a");
    expect(parsed.prompts).toHaveLength(1);
    expect(parsed.prompts[0]?.text).toBe("later");
  });

  it("keeps an empty thread queue referentially stable so React does not loop", () => {
    usePromptQueueStore.setState({ byThreadId: {} });
    const first = selectThreadPromptQueue(usePromptQueueStore.getState(), "thread-a");
    const missing = selectThreadPromptQueue(usePromptQueueStore.getState(), undefined);
    expect(first).toBe(EMPTY_PROMPT_QUEUE);
    expect(missing).toBe(EMPTY_PROMPT_QUEUE);

    usePromptQueueStore.getState().setQueue("thread-a", []);
    expect(selectThreadPromptQueue(usePromptQueueStore.getState(), "thread-a")).toBe(EMPTY_PROMPT_QUEUE);
    expect(sameQueuedPrompts(EMPTY_PROMPT_QUEUE, queuedPromptsFromDaemon([]))).toBe(true);
  });

  it("bounds thread queues and refreshes recency when a cached thread is touched", () => {
    usePromptQueueStore.setState({ byThreadId: {} });
    for (let index = 0; index < MAX_CACHED_PROMPT_QUEUES; index += 1) {
      usePromptQueueStore.getState().setQueue(`thread-${index}`, []);
    }

    usePromptQueueStore.getState().setQueue("thread-0", []);
    usePromptQueueStore.getState().setQueue("thread-new", []);

    const cachedIds = Object.keys(usePromptQueueStore.getState().byThreadId);
    expect(cachedIds).toHaveLength(MAX_CACHED_PROMPT_QUEUES);
    expect(cachedIds).toContain("thread-0");
    expect(cachedIds).toContain("thread-new");
    expect(cachedIds).not.toContain("thread-1");
  });
});
