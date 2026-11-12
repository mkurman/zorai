import { formatKeyboardEvent } from "@/lib/keybindStore";
import { useSettingsStore } from "@/lib/settingsStore";
import { zoraiNavItems, type ZoraiViewId } from "@/zorai/shell/navigation";

export type AppShortcutCategory = "General" | "Navigation" | "Threads";

export type AppShortcut = {
  id: string;
  label: string;
  category: AppShortcutCategory;
  defaultCombo: string;
  menuCommand?: string;
  view?: ZoraiViewId;
};

export const APP_SHORTCUTS: AppShortcut[] = [
  { id: "app.commandPalette", label: "Command palette", category: "General", defaultCombo: "Ctrl+Shift+P", menuCommand: "toggle-command-palette" },
  { id: "app.settings", label: "Settings", category: "General", defaultCombo: "Ctrl+,", menuCommand: "toggle-settings" },
  { id: "app.toggleContext", label: "Toggle context panel", category: "General", defaultCombo: "Ctrl+B", menuCommand: "toggle-sidebar" },
  { id: "app.nightMode", label: "Toggle night mode", category: "General", defaultCombo: "" },
  { id: "app.screenshot", label: "Take screenshot", category: "General", defaultCombo: "" },
  ...zoraiNavItems.map((item): AppShortcut => ({ id: `go.${item.id}`, label: `Go to ${item.label}`, category: "Navigation", defaultCombo: "", view: item.id })),
  { id: "app.files", label: "Open Files", category: "Navigation", defaultCombo: "Ctrl+Shift+E", menuCommand: "toggle-file-manager" },
  { id: "app.newWorkspace", label: "Open Workspaces board", category: "Navigation", defaultCombo: "Ctrl+Shift+N", menuCommand: "new-workspace" },
  { id: "app.newThread", label: "New thread", category: "Threads", defaultCombo: "Ctrl+T", menuCommand: "new-surface" },
  { id: "app.searchThreads", label: "Search threads", category: "Threads", defaultCombo: "Ctrl+Shift+F", menuCommand: "toggle-search" },
  { id: "thread.speakLatest", label: "Read the latest reply aloud", category: "Threads", defaultCombo: "Ctrl+L" },
  { id: "thread.recordVoice", label: "Record a voice message", category: "Threads", defaultCombo: "Ctrl+M" },
];

export const APP_SHORTCUT_CATEGORIES: AppShortcutCategory[] = ["General", "Navigation", "Threads"];

export type ShortcutOverrides = Record<string, string>;

export function shortcutCombo(overrides: ShortcutOverrides | undefined, id: string): string {
  const override = overrides?.[id];
  if (override !== undefined) return override;
  return APP_SHORTCUTS.find((shortcut) => shortcut.id === id)?.defaultCombo ?? "";
}

export function eventMatchesShortcut(overrides: ShortcutOverrides | undefined, id: string, event: KeyboardEvent): boolean {
  const combo = shortcutCombo(overrides, id);
  return Boolean(combo) && formatKeyboardEvent(event) === combo;
}

export function shortcutMatches(id: string, event: KeyboardEvent): boolean {
  return eventMatchesShortcut(useSettingsStore.getState().settings.shortcuts, id, event);
}

export function shortcutConflicts(overrides: ShortcutOverrides | undefined): Map<string, string[]> {
  const byCombo = new Map<string, string[]>();
  for (const shortcut of APP_SHORTCUTS) {
    const combo = shortcutCombo(overrides, shortcut.id);
    if (!combo) continue;
    byCombo.set(combo, [...(byCombo.get(combo) ?? []), shortcut.id]);
  }
  return new Map([...byCombo].filter(([, ids]) => ids.length > 1));
}

export function isCompleteCombo(combo: string): boolean {
  const parts = combo.split("+");
  const key = parts[parts.length - 1];
  return Boolean(key) && !["Ctrl", "Alt", "Shift"].includes(key);
}

function toAccelerator(combo: string): string {
  return combo.replace(/Arrow(Up|Down|Left|Right)$/, "$1").replace(/Esc$/, "Escape");
}

export function menuAccelerators(overrides: ShortcutOverrides | undefined): Record<string, string> {
  return Object.fromEntries(APP_SHORTCUTS
    .filter((shortcut) => shortcut.menuCommand)
    .map((shortcut) => [shortcut.menuCommand!, toAccelerator(shortcutCombo(overrides, shortcut.id))]));
}
