import { create } from "zustand";

type ToolRevealState = {
  key: string | null;
  nonce: number;
  reveal: (key: string) => void;
  clear: (key: string) => void;
};

export const useToolRevealStore = create<ToolRevealState>((set) => ({
  key: null,
  nonce: 0,
  reveal: (key) => set((state) => ({ key, nonce: state.nonce + 1 })),
  clear: (key) => set((state) => (state.key === key ? { key: null } : state)),
}));
