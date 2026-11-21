import { create } from "zustand";

type ComputerControlState = {
  active: boolean;
  threadId: string | null;
  apply: (event: { threadId: string; active: boolean; reason?: string | null }) => string | null;
};

export const useComputerControlStore = create<ComputerControlState>((set, get) => ({
  active: false,
  threadId: null,
  apply: ({ threadId, active, reason }) => {
    const wasActive = get().active;
    set({ active, threadId: active ? threadId : null });
    return !active && wasActive && reason ? `Computer control ended: ${reason}.` : null;
  },
}));
