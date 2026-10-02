import { useEffect, useState } from "react";
import { startAgentWorkspaceReclaim } from "./lib/agentWorkspaceReclaim";
import { startAutoSave } from "./lib/sessionPersistence";
import { coerceThemeNameForAppearance } from "./lib/themes";
import { applyAppShellTheme, getAppShellTheme } from "./lib/themesAppShell";
import {
  applyUiInterfacePrefs,
  normalizeNightMode,
  normalizeUiFontScale,
  resolveAppearance,
  systemPrefersDark,
} from "./lib/uiInterfacePrefs";
import { useSettingsStore } from "./lib/settingsStore";
import { useWorkspaceStore } from "./lib/workspaceStore";
import { ZoraiApp } from "./zorai/ZoraiApp";

export default function App() {
  const createWorkspace = useWorkspaceStore((state) => state.createWorkspace);
  const workspaces = useWorkspaceStore((state) => state.workspaces);
  const settings = useSettingsStore((state) => state.settings);
  const updateSetting = useSettingsStore((state) => state.updateSetting);
  const [systemPrefersDarkScheme, setSystemPrefersDarkScheme] = useState(systemPrefersDark);

  useEffect(() => {
    if (workspaces.length === 0) {
      createWorkspace("Default");
    }
  }, [createWorkspace, workspaces.length]);

  useEffect(() => startAutoSave(30_000), []);
  useEffect(() => startAgentWorkspaceReclaim(), []);

  useEffect(() => {
    if (typeof window === "undefined" || typeof window.matchMedia !== "function") {
      return;
    }
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const sync = () => setSystemPrefersDarkScheme(media.matches);
    sync();
    media.addEventListener("change", sync);
    return () => media.removeEventListener("change", sync);
  }, []);

  useEffect(() => {
    const appearance = resolveAppearance(
      normalizeNightMode(settings.nightMode),
      systemPrefersDarkScheme,
    );
    const themeName = coerceThemeNameForAppearance(settings.themeName, appearance);
    applyAppShellTheme(
      getAppShellTheme(
        themeName,
        settings.useCustomTerminalColors,
        settings.customTerminalBackground,
        settings.customTerminalForeground,
        settings.customTerminalCursor,
        settings.customTerminalSelection,
        appearance,
      ),
    );
    applyUiInterfacePrefs({
      uiFontFamily: settings.uiFontFamily,
      uiFontScale: normalizeUiFontScale(settings.uiFontScale),
      appearance,
    });
    if (themeName !== settings.themeName) {
      updateSetting("themeName", themeName);
    }
  }, [
    settings.themeName,
    settings.useCustomTerminalColors,
    settings.customTerminalBackground,
    settings.customTerminalForeground,
    settings.customTerminalCursor,
    settings.customTerminalSelection,
    settings.uiFontFamily,
    settings.uiFontScale,
    settings.nightMode,
    systemPrefersDarkScheme,
    updateSetting,
  ]);

  return <ZoraiApp />;
}
