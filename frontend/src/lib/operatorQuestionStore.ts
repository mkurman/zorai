import { create } from "zustand";

export type OperatorQuestion = {
  question_id: string;
  content: string;
  options: string[];
  threadId: string | null;
};

type OperatorQuestionStoreState = {
  question: OperatorQuestion | null;
  inlineHosts: number;
  setQuestion: (question: OperatorQuestion | null) => void;
  resolveQuestion: (questionId: string) => void;
  registerInlineHost: () => () => void;
};

export const useOperatorQuestionStore = create<OperatorQuestionStoreState>((set) => ({
  question: null,
  inlineHosts: 0,
  setQuestion: (question) => set({ question }),
  registerInlineHost: () => {
    set((state) => ({ inlineHosts: state.inlineHosts + 1 }));
    return () => set((state) => ({ inlineHosts: Math.max(0, state.inlineHosts - 1) }));
  },
  resolveQuestion: (questionId) =>
    set((state) =>
      state.question && state.question.question_id === questionId
        ? { question: null }
        : state,
    ),
}));
