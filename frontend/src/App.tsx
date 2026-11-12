import { useEffect, useState } from "react";
import { startAgentWorkspaceReclaim } from "./lib/agentWorkspaceReclaim";
import { startAutoSave } from "./lib/sessionPersistence";
import { coerceThemeNameForAppearance, getThemeByName } from "./lib/themes";
import { applyAppShellTheme, getAppShellTheme } from "./lib/themesAppShell";
import { adjustBorders, themeBackdrop, translucentShell } from "./lib/userThemes";
import { applyCustomCss, applyThemeEffects, startUserThemeDiscovery, useUserThemesStore } from "./zorai/features/themes/userThemesStore";
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
import { ThemeBackdropVideo } from "./zorai/features/themes/ThemeBackdropVideo";

export default function App() {
  const createWorkspace = useWorkspaceStore((state) => state.createWorkspace);
  const workspaces = useWorkspaceStore((state) => state.workspaces);
  const settings = useSettingsStore((state) => state.settings);
  const updateSetting = useSettingsStore((state) => state.updateSetting);
  const [systemPrefersDarkScheme, setSystemPrefersDarkScheme] = useState(systemPrefersDark);
  const userThemesLoaded = useUserThemesStore((state) => state.loaded);
  const userThemes = useUserThemesStore((state) => state.themes);
  const themePreview = useUserThemesStore((state) => state.preview);
  const seeThroughAtLaunch = useUserThemesStore((state) => state.seeThroughAtLaunch);
  const customCss = useUserThemesStore((state) => state.customCss);

  useEffect(() => startUserThemeDiscovery(), []);

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
    const activeThemeName = themePreview?.name ?? themeName;
    const activeTheme = getThemeByName(activeThemeName);
    const shell = adjustBorders(getAppShellTheme(
      activeThemeName,
      settings.useCustomTerminalColors,
      settings.customTerminalBackground,
      settings.customTerminalForeground,
      settings.customTerminalCursor,
      settings.customTerminalSelection,
      themePreview?.appearance ?? appearance,
    ), activeTheme.user?.borderStrength ?? 1);
    const backdrop = themeBackdrop(activeTheme, seeThroughAtLaunch);
    applyAppShellTheme(translucentShell(shell, backdrop, activeTheme.user?.surfaceOpacity ?? 1));
    applyThemeEffects(activeTheme, backdrop, shell);
    applyCustomCss(customCss);
    applyUiInterfacePrefs({
      uiFontFamily: settings.uiFontFamily,
      uiFontScale: normalizeUiFontScale(settings.uiFontScale),
      appearance: themePreview?.appearance ?? appearance,
      themeName: activeThemeName,
      themeFonts: activeTheme.fonts,
    });
    if (userThemesLoaded && themeName !== settings.themeName) {
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
    userThemesLoaded,
    userThemes,
    themePreview,
    seeThroughAtLaunch,
    customCss,
  ]);

  return (
    <>
      <ThemeBackdropVideo />
      <ZoraiApp />
    </>
  );
}
