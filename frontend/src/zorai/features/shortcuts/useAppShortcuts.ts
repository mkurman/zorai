import { useEffect, useRef } from "react";
import { getBridge } from "@/lib/bridge";
import { useSettingsStore } from "@/lib/settingsStore";
import { handleZoraiAppCommand } from "@/zorai/shell/zoraiAppCommands";
import { navigateZorai } from "@/zorai/shell/zoraiNavigationEvents";
import { APP_SHORTCUTS, eventMatchesShortcut, menuAccelerators } from "./appShortcuts";

export const SHORTCUT_CAPTURE_ATTRIBUTE = "data-shortcut-capture";

export function useAppShortcuts({ onToggleNightMode }: { onToggleNightMode: () => void }) {
  const shortcuts = useSettingsStore((state) => state.settings.shortcuts);
  const toggleNightModeRef = useRef(onToggleNightMode);
  toggleNightModeRef.current = onToggleNightMode;

  useEffect(() => {
    void getBridge()?.appSetMenuAccelerators?.(menuAccelerators(shortcuts));
  }, [shortcuts]);

  useEffect(() => {
    const menuHandledByDesktop = Boolean(getBridge()?.appSetMenuAccelerators);
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.defaultPrevented) return;
      if (event.target instanceof Element && event.target.closest(`[${SHORTCUT_CAPTURE_ATTRIBUTE}]`)) return;
      const current = useSettingsStore.getState().settings.shortcuts;
      const shortcut = APP_SHORTCUTS.find((candidate) =>
        (candidate.view || candidate.id === "app.nightMode" || candidate.id === "app.screenshot" || (candidate.menuCommand && !menuHandledByDesktop))
        && eventMatchesShortcut(current, candidate.id, event));
      if (!shortcut) return;
      event.preventDefault();
      if (shortcut.view) navigateZorai({ view: shortcut.view });
      else if (shortcut.id === "app.nightMode") toggleNightModeRef.current();
      else if (shortcut.id === "app.screenshot") void getBridge()?.contextMenuAction?.("screenshot");
      else if (shortcut.menuCommand) handleZoraiAppCommand(shortcut.menuCommand);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);
}
