import { create } from "zustand";

type NextReplyState = {
  byThreadId: Record<string, string>;
  setSuggestion: (threadId: string, content: string) => void;
  clearSuggestion: (threadId: string) => void;
};

export const useNextReplyStore = create<NextReplyState>((set) => ({
  byThreadId: {},
  setSuggestion: (threadId, content) => set((state) => ({ byThreadId: { ...state.byThreadId, [threadId]: content } })),
  clearSuggestion: (threadId) => set((state) => {
    if (!(threadId in state.byThreadId)) return state;
    const { [threadId]: _removed, ...rest } = state.byThreadId;
    return { byThreadId: rest };
  }),
}));
