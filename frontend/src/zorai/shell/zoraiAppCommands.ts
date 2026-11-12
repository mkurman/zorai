import { useEffect } from "react";
import { getBridge } from "@/lib/bridge";
import { useAgentStore } from "@/lib/agentStore";
import { useWorkspaceStore } from "@/lib/workspaceStore";
import { useUpdateStore } from "@/components/UpdatePrompt";
import type { ZoraiNavigateDetail } from "./zoraiNavigationEvents";
import { navigateZorai } from "./zoraiNavigationEvents";

export type ZoraiAppCommandResolution =
  | { kind: "unhandled" }
  | { kind: "palette" }
  | { kind: "time-travel" }
  | { kind: "new-thread" }
  | { kind: "check-updates" }
  | { kind: "navigate"; detail: ZoraiNavigateDetail };

const MENU_COMMANDS: Record<string, ZoraiAppCommandResolution> = {
  "toggle-command-palette": { kind: "palette" },
  "toggle-search": { kind: "navigate", detail: { view: "threads", focusSearch: true } },
  "toggle-settings": { kind: "navigate", detail: { view: "settings" } },
  about: { kind: "navigate", detail: { view: "settings", settingsTab: "about" } },
  "check-updates": { kind: "check-updates" },
  "developer-tools": { kind: "navigate", detail: { view: "settings", settingsTab: "developer" } },
  "new-workspace": { kind: "navigate", detail: { view: "workspaces" } },
  "new-surface": { kind: "new-thread" },
  "toggle-sidebar": { kind: "navigate", detail: { toggleContext: true } },
  "toggle-mission": { kind: "navigate", detail: { view: "threads" } },
  "toggle-file-manager": { kind: "navigate", detail: { view: "tools", tool: "files" } },
  "toggle-command-history": { kind: "navigate", detail: { view: "tools", tool: "history" } },
  "toggle-command-log": { kind: "navigate", detail: { view: "tools", tool: "history" } },
  "toggle-session-vault": { kind: "navigate", detail: { view: "tools", tool: "vault" } },
  "toggle-system-monitor": { kind: "navigate", detail: { view: "tools", tool: "system" } },
  "toggle-canvas": { kind: "navigate", detail: { view: "tools", tool: "canvas" } },
  "split-right": { kind: "navigate", detail: { view: "tools", tool: "terminal" } },
  "split-down": { kind: "navigate", detail: { view: "tools", tool: "terminal" } },
  "toggle-zoom": { kind: "navigate", detail: { view: "tools", tool: "terminal" } },
  "toggle-time-travel": { kind: "time-travel" },
};

export function resolveZoraiAppCommand(command: string): ZoraiAppCommandResolution {
  return MENU_COMMANDS[command] ?? { kind: "unhandled" };
}

export function handleZoraiAppCommand(command: string): boolean {
  const resolution = resolveZoraiAppCommand(command);
  if (resolution.kind === "unhandled") {
    return false;
  }
  if (resolution.kind === "palette") {
    useWorkspaceStore.getState().toggleCommandPalette();
    return true;
  }
  if (resolution.kind === "time-travel") {
    useWorkspaceStore.getState().toggleTimeTravel();
    return true;
  }
  if (resolution.kind === "check-updates") {
    navigateZorai({ view: "settings", settingsTab: "about" });
    void useUpdateStore.getState().check(true);
    return true;
  }
  if (resolution.kind === "new-thread") {
    useAgentStore.getState().createThread({});
    navigateZorai({ view: "threads" });
    return true;
  }
  navigateZorai(resolution.detail);
  return true;
}

export function useZoraiAppCommands() {
  useEffect(() => {
    const unsubscribe = getBridge()?.onAppCommand?.((command: string) => {
      handleZoraiAppCommand(command);
    });
    return () => {
      if (typeof unsubscribe === "function") unsubscribe();
    };
  }, []);
}
