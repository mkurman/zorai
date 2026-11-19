import { useEffect } from "react";
import { getBridge } from "@/lib/bridge";
import { useAgentStore } from "@/lib/agentStore";
import { useWorkspaceStore } from "@/lib/workspaceStore";
import { useUpdateStore } from "@/components/UpdatePrompt";
import { useThreadPanelStore, type ThreadPanelId } from "@/zorai/features/threads/threadSidePanel";
import { zoraiNavItems } from "./navigation";
import { useShellLayoutStore } from "./shellLayoutStore";
import type { ZoraiNavigateDetail } from "./zoraiNavigationEvents";
import { navigateZorai } from "./zoraiNavigationEvents";

export type ZoraiAppCommandResolution =
  | { kind: "unhandled" }
  | { kind: "palette" }
  | { kind: "time-travel" }
  | { kind: "new-thread" }
  | { kind: "check-updates" }
  | { kind: "toggle-rail" }
  | { kind: "thread-panel"; panel: ThreadPanelId }
  | { kind: "thread-command"; command: ZoraiThreadCommand }
  | { kind: "navigate"; detail: ZoraiNavigateDetail };

export const ZORAI_THREAD_COMMAND_EVENT = "zorai-thread-command";

export const ZORAI_THREAD_COMMANDS = [
  "close-thread-tab",
  "reopen-closed-thread",
  "previous-thread",
  "next-thread",
  "thread-back",
  "split-right",
  "split-down",
  "split-close-pane",
  "split-close",
] as const;

export type ZoraiThreadCommand = (typeof ZORAI_THREAD_COMMANDS)[number];

let pendingThreadCommand: ZoraiThreadCommand | null = null;

export function consumePendingThreadCommand(): ZoraiThreadCommand | null {
  const command = pendingThreadCommand;
  pendingThreadCommand = null;
  return command;
}

const THREAD_PANELS: ThreadPanelId[] = ["participants", "handoff", "gateway", "operations"];

const MENU_COMMANDS: Record<string, ZoraiAppCommandResolution> = {
  ...Object.fromEntries(zoraiNavItems.map((item) => [`go-${item.id}`, { kind: "navigate", detail: { view: item.id } }])),
  ...Object.fromEntries(THREAD_PANELS.map((panel) => [`thread-panel-${panel}`, { kind: "thread-panel", panel }])),
  ...Object.fromEntries(ZORAI_THREAD_COMMANDS.map((command) => [command, { kind: "thread-command", command }])),
  "toggle-rail": { kind: "toggle-rail" },
  "keyboard-shortcuts": { kind: "navigate", detail: { view: "settings", settingsTab: "shortcuts" } },
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
  if (resolution.kind === "toggle-rail") {
    useShellLayoutStore.getState().setRailOpen((open) => !open);
    return true;
  }
  if (resolution.kind === "thread-panel") {
    navigateZorai({ view: "threads" });
    useThreadPanelStore.getState().toggle(resolution.panel);
    return true;
  }
  if (resolution.kind === "thread-command") {
    pendingThreadCommand = resolution.command;
    navigateZorai({ view: "threads" });
    window.dispatchEvent(new CustomEvent(ZORAI_THREAD_COMMAND_EVENT));
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
