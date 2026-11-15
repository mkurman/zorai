import { createContext, useContext } from "react";
import type { WorkContextEntry } from "@/lib/agentWorkContext";

export type ThreadFilePreviewTarget = {
  entry: WorkContextEntry;
  trail?: WorkContextEntry[];
};

export type ThreadFilePreviewContextValue = {
  previewTarget: ThreadFilePreviewTarget | null;
  openThreadFilePreview: (entry: WorkContextEntry, trail?: WorkContextEntry[]) => void;
  closeThreadFilePreview: () => void;
};

export const ThreadFilePreviewContext = createContext<ThreadFilePreviewContextValue | null>(null);

export function useThreadFilePreview() {
  const context = useContext(ThreadFilePreviewContext);
  if (!context) {
    throw new Error("useThreadFilePreview must be used inside ThreadFilePreviewProvider");
  }
  return context;
}
