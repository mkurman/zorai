import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";

export type PluginViewMode = "grid" | "list";

type PluginPrefsState = {
  favorites: string[];
  pinned: string[];
  viewMode: PluginViewMode;
  conversations: Record<string, string[]>;
  currentConversation: Record<string, string | null>;
  recordConversation: (plugin: string, threadId: string) => void;
  setCurrentConversation: (plugin: string, threadId: string | null) => void;
  toggleFavorite: (name: string) => void;
  togglePinned: (name: string) => void;
  setViewMode: (mode: PluginViewMode) => void;
};

const toggle = (list: string[], name: string) => (list.includes(name) ? list.filter((entry) => entry !== name) : [...list, name]);

export const usePluginPrefsStore = create<PluginPrefsState>()(persist(
  (set) => ({
    favorites: [],
    pinned: [],
    viewMode: "grid",
    conversations: {},
    currentConversation: {},
    recordConversation: (plugin, threadId) => set((state) => {
      const known = state.conversations[plugin] ?? [];
      return known.includes(threadId) ? {} : { conversations: { ...state.conversations, [plugin]: [...known, threadId].slice(-50) } };
    }),
    setCurrentConversation: (plugin, threadId) => set((state) => ({ currentConversation: { ...state.currentConversation, [plugin]: threadId } })),
    toggleFavorite: (name) => set((state) => ({ favorites: toggle(state.favorites, name) })),
    togglePinned: (name) => set((state) => ({ pinned: toggle(state.pinned, name) })),
    setViewMode: (viewMode) => set({ viewMode }),
  }),
  {
    name: "zorai-plugin-prefs",
    storage: createJSONStorage(() => (typeof localStorage !== "undefined" ? localStorage : { getItem: () => null, setItem: () => undefined, removeItem: () => undefined })),
    partialize: (state) => ({ favorites: state.favorites, pinned: state.pinned, viewMode: state.viewMode, conversations: state.conversations, currentConversation: state.currentConversation }),
  },
));

const PLUGIN_RAIL_PREFIX = "plugin:";

export function pinnedRailIds(pinned: string[]): string[] {
  return pinned.map((name) => `${PLUGIN_RAIL_PREFIX}${name}`);
}

export function pluginNameFromRailId(id: string): string | null {
  return id.startsWith(PLUGIN_RAIL_PREFIX) ? id.slice(PLUGIN_RAIL_PREFIX.length) : null;
}

export function sortPluginsByFavorite<T extends { name: string }>(plugins: T[], favorites: string[]): T[] {
  const favorite = new Set(favorites);
  return [...plugins].sort((a, b) => Number(favorite.has(b.name)) - Number(favorite.has(a.name)) || a.name.localeCompare(b.name));
}

export function matchesPluginSearch(plugin: { name: string; description?: string; panels?: Array<{ title: string }>; commands?: Array<{ title: string }> }, query: string): boolean {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (!words.length) return true;
  const haystack = [plugin.name, plugin.description ?? "", ...(plugin.panels ?? []).map((panel) => panel.title), ...(plugin.commands ?? []).map((command) => command.title)].join(" ").toLowerCase();
  return words.every((word) => haystack.includes(word));
}

export function monogram(name: string): { letters: string; hue: number } {
  const parts = name.split(/[-_\s]+/).filter(Boolean);
  const letters = (parts.length > 1 ? parts[0][0] + parts[1][0] : name.slice(0, 2)).toUpperCase();
  let hash = 0;
  for (const char of name) hash = (hash * 31 + char.charCodeAt(0)) >>> 0;
  return { letters, hue: hash % 360 };
}
