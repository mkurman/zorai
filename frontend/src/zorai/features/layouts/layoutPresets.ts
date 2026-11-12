import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";
import { useSettingsStore } from "@/lib/settingsStore";
import type { ChatWidthPreference } from "@/lib/types";
import { normalizeChatWidth } from "@/lib/uiInterfacePrefs";
import { useCodeBottomPanelStore, type CodeBottomPanelTab } from "@/zorai/features/code/codeBottomPanelStore";
import { useCodeLayoutStore } from "@/zorai/features/code/codeLayoutStore";
import { LAST_ZORAI_VIEW_STORAGE_KEY, zoraiNavItems, type ZoraiViewId } from "@/zorai/shell/navigation";
import { navigateZorai } from "@/zorai/shell/zoraiNavigationEvents";
import { useShellLayoutStore } from "@/zorai/shell/shellLayoutStore";

export type LayoutSnapshot = {
  view: ZoraiViewId;
  railOpen: boolean;
  contextOpen: boolean;
  railWidths: Record<string, number>;
  contextWidths: Record<string, number>;
  codeExplorerWidth: number;
  codeAgentWidth: number;
  bottomPanel: { open: boolean; tab: CodeBottomPanelTab; height: number };
  chatWidth: ChatWidthPreference;
};

export type LayoutPreset = { id: string; name: string; layout: LayoutSnapshot };

function currentView(): ZoraiViewId {
  const stored = useShellLayoutStore.getState().lastWorkView
    ?? (typeof localStorage === "undefined" ? null : localStorage.getItem(LAST_ZORAI_VIEW_STORAGE_KEY));
  return zoraiNavItems.find((item) => item.id === stored && item.id !== "settings")?.id ?? "threads";
}

export function captureLayout(): LayoutSnapshot {
  const shell = useShellLayoutStore.getState();
  const code = useCodeLayoutStore.getState();
  const panel = useCodeBottomPanelStore.getState();
  return {
    view: currentView(),
    railOpen: shell.railOpen,
    contextOpen: shell.contextOpen,
    railWidths: { ...shell.railWidths },
    contextWidths: { ...shell.contextWidths },
    codeExplorerWidth: code.explorerPreferredWidth,
    codeAgentWidth: code.agentPreferredWidth,
    bottomPanel: { open: panel.open, tab: panel.tab, height: panel.height },
    chatWidth: normalizeChatWidth(useSettingsStore.getState().settings.chatWidth),
  };
}

const finiteWidths = (widths: unknown): Record<string, number> => Object.fromEntries(
  Object.entries(widths && typeof widths === "object" ? widths : {}).filter(([, value]) => typeof value === "number" && Number.isFinite(value)),
) as Record<string, number>;

export function applyLayout(layout: LayoutSnapshot): void {
  const shell = useShellLayoutStore.getState();
  useShellLayoutStore.setState({
    railWidths: finiteWidths(layout.railWidths),
    contextWidths: finiteWidths(layout.contextWidths),
    railOpen: layout.railOpen !== false,
    contextOpen: layout.contextOpen === true,
    codeContextOpen: layout.contextOpen === true,
    otherContextOpen: layout.contextOpen === true,
  });
  shell.markCodeContextAutoOpened();
  const code = useCodeLayoutStore.getState();
  if (Number.isFinite(layout.codeExplorerWidth)) code.setExplorerPreferredWidth(layout.codeExplorerWidth);
  if (Number.isFinite(layout.codeAgentWidth)) code.setAgentPreferredWidth(layout.codeAgentWidth);
  if (layout.bottomPanel) {
    useCodeBottomPanelStore.setState({
      open: layout.bottomPanel.open === true,
      tab: layout.bottomPanel.tab === "output" ? "output" : "terminal",
    });
    if (Number.isFinite(layout.bottomPanel.height)) useCodeBottomPanelStore.getState().setHeight(layout.bottomPanel.height);
  }
  useSettingsStore.getState().updateSetting("chatWidth", normalizeChatWidth(layout.chatWidth));
  if (zoraiNavItems.some((item) => item.id === layout.view)) navigateZorai({ view: layout.view });
}

type LayoutPresetsState = {
  presets: LayoutPreset[];
  save: (name: string, layout: LayoutSnapshot) => LayoutPreset;
  remove: (id: string) => void;
};

export const useLayoutPresetsStore = create<LayoutPresetsState>()(persist(
  (set, get) => ({
    presets: [],
    save: (name, layout) => {
      const trimmed = name.trim();
      const existing = get().presets.find((preset) => preset.name.toLowerCase() === trimmed.toLowerCase());
      const preset = { id: existing?.id ?? `layout-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`, name: trimmed, layout };
      set((state) => ({ presets: existing ? state.presets.map((entry) => (entry.id === existing.id ? preset : entry)) : [...state.presets, preset] }));
      return preset;
    },
    remove: (id) => set((state) => ({ presets: state.presets.filter((preset) => preset.id !== id) })),
  }),
  {
    name: "zorai-layout-presets",
    storage: createJSONStorage(() => (typeof localStorage !== "undefined" ? localStorage : { getItem: () => null, setItem: () => undefined, removeItem: () => undefined })),
    partialize: (state) => ({ presets: state.presets }),
  },
));
