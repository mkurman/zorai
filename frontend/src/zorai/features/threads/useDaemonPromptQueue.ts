import { useEffect, useRef, useState } from "react";
import type { SendMessagePayload } from "@/components/agent-chat-panel/chat-view/types";
import { getBridge } from "@/lib/bridge";
import { pushToast } from "@/lib/toastStore";
import {
  createQueuedComposerId,
  queuedPromptsFromDaemon,
  readPromptQueueResponse,
  reconcileSentQueuedPromptMessages,
  shouldApplyPromptQueueSnapshot,
  type QueuedComposerMessage,
} from "./composerQueue";
import { usePromptQueueStore, selectThreadPromptQueue } from "./promptQueueStore";
import { useAgentStore } from "@/lib/agentStore";
import { waitForThreadStopBarrier } from "@/components/agent-chat-panel/runtime/threadStopBarrier";

function applyQueueResponse(
  threadId: string,
  raw: unknown,
  expectedId?: string,
  applySnapshot = true,
): { accepted: boolean; parsed: ReturnType<typeof readPromptQueueResponse> } {
  const parsed = readPromptQueueResponse(raw);
  if (parsed.error) {
    // Surface the daemon's real message (QUEUE FULL, validation, or timeout
    // mapped in agent-ipc-handlers). Don't fall through to the generic string.
    pushToast(parsed.error);
    return { accepted: false, parsed };
  }
  if (expectedId && !parsed.prompts.some((item) => item.id === expectedId)) {
    // This is an unexpected serialization mismatch (IPC timeout mapped badly,
    // race, etc.). Surface it distinctly so it doesn't look like a capacity
    // rejection. The queue will self-heal via the next prompt_queue_update event.
    pushToast("Follow-up not echoed back — it may still be queued (syncing…)");
    return { accepted: false, parsed };
  }
  if (applySnapshot) {
    usePromptQueueStore.getState().setQueue(threadId, parsed.prompts);
  }
  return { accepted: true, parsed };
}

function applyQueueResponseForced(threadId: string, raw: unknown, applySnapshot = true): boolean {
  const parsed = readPromptQueueResponse(raw);
  if (parsed.error) {
    pushToast(parsed.error);
    return false;
  }
  if (applySnapshot) {
    usePromptQueueStore.getState().setQueue(threadId, parsed.prompts);
  }
  return true;
}

function reconcileSentQueuedPrompt(
  localThreadId: string,
  prompt: QueuedComposerMessage,
  insertionBoundary: number,
  messageCountBeforeSend: number,
): void {
  useAgentStore.setState((state) => {
    const localThread = state.threads.find((thread) => thread.id === localThreadId);
    if (!localThread) return state;
    const currentMessages = state.messages[localThread.id] ?? [];
    const nextMessages = reconcileSentQueuedPromptMessages(
      currentMessages,
      localThread.id,
      prompt,
      Date.now(),
      insertionBoundary,
    );
    if (nextMessages === currentMessages) return state;
    return {
      messages: { ...state.messages, [localThread.id]: nextMessages },
      threads: state.threads.map((thread) => thread.id === localThread.id ? {
        ...thread,
        messageCount: Math.max(thread.messageCount, messageCountBeforeSend + 1),
        updatedAt: Date.now(),
        lastMessagePreview: prompt.text.slice(0, 100),
      } : thread),
    };
  });
}

export function useDaemonPromptQueue(daemonThreadId: string | null | undefined) {
  const queuedMessages = usePromptQueueStore((state) => selectThreadPromptQueue(state, daemonThreadId));
  const [editingId, setEditingId] = useState<string | null>(null);
  const daemonEventRevisionRef = useRef(0);

  useEffect(() => {
    setEditingId(null);
    if (!daemonThreadId) return;
    const bridge = getBridge();
    if (!bridge?.agentListPromptQueue) return;
    let cancelled = false;
    const threadIdAtEffect = daemonThreadId;
    const revisionAtRequest = daemonEventRevisionRef.current;
    void bridge.agentListPromptQueue(threadIdAtEffect).then((raw) => {
      if (cancelled) return;
      applyQueueResponseForced(
        threadIdAtEffect,
        raw,
        shouldApplyPromptQueueSnapshot(revisionAtRequest, daemonEventRevisionRef.current),
      );
    }).catch((error: unknown) => {
      if (!cancelled) {
        pushToast(error instanceof Error ? error.message : "Failed to load queued messages.");
      }
    });
    return () => {
      cancelled = true;
    };
  }, [daemonThreadId]);

  useEffect(() => {
    const threadIdAtEffect = daemonThreadId;
    if (!threadIdAtEffect) return;
    const bridge = getBridge();
    if (!bridge?.onAgentEvent) return;
    return bridge.onAgentEvent((event: { type?: string; thread_id?: string; prompts?: unknown; prompt?: unknown }) => {
      if (event?.type === "queued_prompt_delivered" && event.thread_id === threadIdAtEffect) {
        const [prompt] = queuedPromptsFromDaemon([event.prompt]);
        const state = useAgentStore.getState();
        const localThread = state.threads.find((thread) => thread.daemonThreadId === threadIdAtEffect);
        if (prompt && localThread) {
          const messageCount = (state.messages[localThread.id] ?? []).length;
          reconcileSentQueuedPrompt(localThread.id, prompt, messageCount, localThread.messageCount);
        }
        return;
      }
      if (event?.type !== "prompt_queue_update" || event.thread_id !== threadIdAtEffect) return;
      daemonEventRevisionRef.current += 1;
      usePromptQueueStore.getState().applyDaemonUpdate(threadIdAtEffect, event.prompts);
    });
  }, [daemonThreadId]);

  const enqueue = async (payload: SendMessagePayload) => {
    if (!daemonThreadId) {
      pushToast("No daemon thread to queue against.");
      return false;
    }
    const bridge = getBridge();
    if (!bridge?.agentEnqueuePrompt) {
      pushToast("Queued messages require the daemon bridge.");
      return false;
    }
    const promptId = createQueuedComposerId();
    const revisionAtRequest = daemonEventRevisionRef.current;
    try {
      const raw = await bridge.agentEnqueuePrompt({
        threadId: daemonThreadId,
        content: payload.text,
        contentBlocksJson: payload.contentBlocksJson ?? null,
        promptId,
      });
      const { accepted } = applyQueueResponse(
        daemonThreadId,
        raw,
        promptId,
        shouldApplyPromptQueueSnapshot(revisionAtRequest, daemonEventRevisionRef.current),
      );
      return accepted;
    } catch (error) {
      pushToast(error instanceof Error ? error.message : "Failed to queue message.");
      return false;
    }
  };

  const updateQueued = async (promptId: string, payload: SendMessagePayload) => {
    if (!daemonThreadId) return false;
    const bridge = getBridge();
    if (!bridge?.agentUpdateQueuedPrompt) {
      pushToast("Queued messages require the daemon bridge.");
      return false;
    }
    const revisionAtRequest = daemonEventRevisionRef.current;
    try {
      const raw = await bridge.agentUpdateQueuedPrompt({
        threadId: daemonThreadId,
        promptId,
        content: payload.text,
        contentBlocksJson: payload.contentBlocksJson ?? null,
      });
      const { accepted } = applyQueueResponse(
        daemonThreadId,
        raw,
        promptId,
        shouldApplyPromptQueueSnapshot(revisionAtRequest, daemonEventRevisionRef.current),
      );
      if (accepted) setEditingId(null);
      return accepted;
    } catch (error) {
      pushToast(error instanceof Error ? error.message : "Failed to update queued message.");
      return false;
    }
  };

  const cancelQueued = async (promptId: string) => {
    if (!daemonThreadId) return;
    const bridge = getBridge();
    if (!bridge?.agentCancelQueuedPrompt) return;
    const revisionAtRequest = daemonEventRevisionRef.current;
    try {
      const raw = await bridge.agentCancelQueuedPrompt({
        threadId: daemonThreadId,
        promptId,
      });
      applyQueueResponseForced(
        daemonThreadId,
        raw,
        shouldApplyPromptQueueSnapshot(revisionAtRequest, daemonEventRevisionRef.current),
      );
      if (editingId === promptId) setEditingId(null);
    } catch (error) {
      pushToast(error instanceof Error ? error.message : "Failed to cancel queued message.");
    }
  };

  const sendNow = async (promptId: string) => {
    if (!daemonThreadId) return;
    const bridge = getBridge();
    if (!bridge?.agentSendQueuedPromptNow) return;
    const stateBeforeBarrier = useAgentStore.getState();
    const threadBeforeBarrier = stateBeforeBarrier.threads.find(
      (thread) => thread.id === stateBeforeBarrier.activeThreadId && thread.daemonThreadId === daemonThreadId,
    ) ?? stateBeforeBarrier.threads.find((thread) => thread.daemonThreadId === daemonThreadId);
    try {
      if (threadBeforeBarrier) {
        await waitForThreadStopBarrier(threadBeforeBarrier.id);
      }

      const prompt = usePromptQueueStore.getState().byThreadId[daemonThreadId]
        ?.find((item) => item.id === promptId);
      const stateAtSend = useAgentStore.getState();
      const localThread = stateAtSend.threads.find(
        (thread) => thread.id === stateAtSend.activeThreadId && thread.daemonThreadId === daemonThreadId,
      ) ?? stateAtSend.threads.find((thread) => thread.daemonThreadId === daemonThreadId);
      const insertionBoundary = localThread
        ? (stateAtSend.messages[localThread.id] ?? []).length
        : 0;
      const messageCountBeforeSend = localThread?.messageCount ?? insertionBoundary;
      const revisionAtRequest = daemonEventRevisionRef.current;
      const raw = await bridge.agentSendQueuedPromptNow({
        threadId: daemonThreadId,
        promptId,
      });
      const accepted = applyQueueResponseForced(
        daemonThreadId,
        raw,
        shouldApplyPromptQueueSnapshot(revisionAtRequest, daemonEventRevisionRef.current),
      );
      if (accepted && prompt && localThread) {
        reconcileSentQueuedPrompt(
          localThread.id,
          prompt,
          insertionBoundary,
          messageCountBeforeSend,
        );
      }
      if (editingId === promptId) setEditingId(null);
    } catch (error) {
      pushToast(error instanceof Error ? error.message : "Failed to send queued message.");
    }
  };

  return {
    queuedMessages,
    editingId,
    startEdit: (item: QueuedComposerMessage) => setEditingId(item.id),
    cancelEdit: () => setEditingId(null),
    enqueue,
    updateQueued,
    cancelQueued,
    sendNow,
  };
}
