import { useWorkspaceContextStore } from "@/lib/workspaceContextStore";
import { useCodeEditorSettingsStore } from "./codeEditorSettingsStore";

export function hydrateCodeWorkspaceContexts(): Promise<void> {
  return useWorkspaceContextStore.getState().hydrate({ reopenEditors: useCodeEditorSettingsStore.getState().settings.reopenEditors });
}
