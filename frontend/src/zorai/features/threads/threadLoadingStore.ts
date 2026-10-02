import { create } from "zustand";

type ThreadLoadingState = {
  pending: number;
  // Per-thread in-flight counts, keyed by whatever id the caller knows
  // (local id and/or daemon id). Lets the thread rail show a loader on the
  // exact row that was clicked while a big thread page is being fetched.
  byThreadId: Record<string, number>;
  begin: (threadId?: string | null) => void;
  end: (threadId?: string | null) => void;
};

export const useThreadLoadingStore = create<ThreadLoadingState>((set) => ({
  pending: 0,
  byThreadId: {},
  begin: (threadId) => set((state) => ({
    pending: state.pending + 1,
    byThreadId: threadId
      ? { ...state.byThreadId, [threadId]: (state.byThreadId[threadId] ?? 0) + 1 }
      : state.byThreadId,
  })),
  end: (threadId) => set((state) => {
    if (!threadId || !state.byThreadId[threadId]) {
      return { pending: Math.max(0, state.pending - 1), byThreadId: state.byThreadId };
    }
    const nextCount = (state.byThreadId[threadId] ?? 0) - 1;
    const byThreadId = { ...state.byThreadId };
    if (nextCount <= 0) {
      delete byThreadId[threadId];
    } else {
      byThreadId[threadId] = nextCount;
    }
    return { pending: Math.max(0, state.pending - 1), byThreadId };
  }),
}));

export function beginThreadLoadingFor(threadIds: Array<string | null | undefined>): () => void {
  const finishers = [...new Set(threadIds.map((id) => id?.trim()).filter((id): id is string => Boolean(id)))]
    .map((id) => beginThreadLoading(id));
  return () => {
    for (const finish of finishers) finish();
  };
}

export function beginThreadLoading(threadId?: string | null): () => void {
  useThreadLoadingStore.getState().begin(threadId);
  let finished = false;
  return () => {
    if (finished) return;
    finished = true;
    useThreadLoadingStore.getState().end(threadId);
  };
}

export function isThreadLoading(
  byThreadId: Record<string, number>,
  localId: string,
  daemonThreadId?: string | null,
): boolean {
  return (byThreadId[localId] ?? 0) > 0
    || Boolean(daemonThreadId && (byThreadId[daemonThreadId] ?? 0) > 0);
}

export function shouldShowConversationSkeleton(input: {
  pending: number;
  activeThreadLoading?: boolean;
  hasActiveThread: boolean;
  loadedMessageCount: number;
  knownHistory: boolean;
}): boolean {
  if (input.loadedMessageCount > 0) return false;
  if (input.activeThreadLoading) return true;
  if (input.pending > 0) return true;
  if (!input.hasActiveThread) return false;
  return input.knownHistory;
}
