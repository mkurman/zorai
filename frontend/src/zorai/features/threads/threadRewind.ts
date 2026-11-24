import { create } from "zustand";
import type { AgentMessage } from "@/lib/agentStore";

export type ThreadRewind = {
  threadId: string;
  anchorId: string | null;
  messages: AgentMessage[];
  draftBefore: string;
};

export const useThreadRewindStore = create<{
  rewind: ThreadRewind | null;
  setRewind: (rewind: ThreadRewind | null) => void;
}>((set) => ({
  rewind: null,
  setRewind: (rewind) => set({ rewind }),
}));

export function messagesFromRewindPoint(messages: readonly AgentMessage[], messageId: string): AgentMessage[] {
  const index = messages.findIndex((message) => message.id === messageId);
  return index < 0 ? [] : messages.slice(index);
}

export function rewindIsUndoable(rewind: ThreadRewind | null, threadId: string | null | undefined, messages: readonly AgentMessage[]): boolean {
  if (!rewind || !threadId || rewind.threadId !== threadId) return false;
  return (messages[messages.length - 1]?.id ?? null) === rewind.anchorId;
}
