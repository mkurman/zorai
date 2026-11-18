import { createContext } from "react";
import { create } from "zustand";

type ComposerDraftState = {
  input: string;
  setInput: (value: string | ((current: string) => string)) => void;
};

export const useComposerDraftStore = create<ComposerDraftState>((set) => ({
  input: "",
  setInput: (value) => set((state) => ({
    input: typeof value === "function" ? value(state.input) : value,
  })),
}));

export const ComposerDraftOverrideContext = createContext<string | null>(null);

export function readComposerDraftInput(): string {
  return useComposerDraftStore.getState().input;
}

export function writeComposerDraftInput(value: string | ((current: string) => string)): void {
  useComposerDraftStore.getState().setInput(value);
}

export function composerDraftIsImageCommand(input: string): boolean {
  const trimmed = input.trim();
  return trimmed === "/image" || trimmed.startsWith("/image ");
}

export function resetComposerDraftStoreForTest(): void {
  useComposerDraftStore.setState({ input: "" });
}
