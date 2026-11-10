import { describe, expect, it } from "vitest";
import type { StateStorage } from "zustand/middleware";
import { CODE_EDITOR_DEFAULTS, CODE_EDITOR_SETTINGS_STORE_NAME, codeSettingsCommandPatch, codeSettingsToggleState, createCodeEditorSettingsStore, normalizeCodeEditorSettings } from "./codeEditorSettingsStore";

function memoryStorage(): StateStorage {
  const values = new Map<string, string>();
  return { getItem: (key) => values.get(key) ?? null, setItem: (key, value) => void values.set(key, value), removeItem: (key) => void values.delete(key) };
}

describe("codeEditorSettingsStore", () => {
  it("starts with immutable safe defaults", () => {
    const store = createCodeEditorSettingsStore(memoryStorage());
    expect(store.getState().settings).toMatchObject({ wordWrap: "bounded", wordWrapColumn: 120, autoSave: "off", autoSaveDelayMs: 1000, maxFileSizeMb: 5 });
    expect(Object.isFrozen(CODE_EDITOR_DEFAULTS)).toBe(true);
  });

  it("normalizes malformed and out-of-range persisted values", () => {
    expect(normalizeCodeEditorSettings({ wordWrap: "wild", wordWrapColumn: 900, fontSize: 2, maxFileSizeMb: 0, autoSaveDelayMs: 999999 })).toMatchObject({
      wordWrap: "bounded", wordWrapColumn: 500, fontSize: 8, maxFileSizeMb: 1, autoSaveDelayMs: 60000,
    });
  });

  it("updates, resets, and persists only settings data", () => {
    const storage = memoryStorage();
    const store = createCodeEditorSettingsStore(storage);
    store.getState().updateSetting("fontSize", 18);
    expect(store.getState().settings.fontSize).toBe(18);
    expect(JSON.parse(storage.getItem(CODE_EDITOR_SETTINGS_STORE_NAME) as string).state.settings.fontSize).toBe(18);
    store.getState().resetSettings();
    expect(store.getState().settings).toEqual(CODE_EDITOR_DEFAULTS);
  });

  it("preserves safe unknown shortcut IDs without executing them", () => {
    const normalized = normalizeCodeEditorSettings({ keybindings: { "future.safe-command": "Ctrl+K", "bad id!": "Ctrl+X" } });
    expect(normalized.keybindings).toEqual({ "future.safe-command": "Ctrl+K" });
  });
});

describe("code settings commands", () => {
  const settings = { ...CODE_EDITOR_DEFAULTS, keybindings: {} };

  it("toggles the saved setting, so the editor menu can show whether wrap and minimap are on", () => {
    expect(codeSettingsToggleState("view.toggleWrap", settings)).toBe(true);
    expect(codeSettingsCommandPatch("view.toggleWrap", settings)).toEqual({ wordWrap: "off" });
    expect(codeSettingsCommandPatch("view.toggleWrap", { ...settings, wordWrap: "off" })).toEqual({ wordWrap: CODE_EDITOR_DEFAULTS.wordWrap });
    expect(codeSettingsToggleState("view.toggleMinimap", { ...settings, minimap: false })).toBe(false);
    expect(codeSettingsCommandPatch("view.toggleMinimap", settings)).toEqual({ minimap: false });
  });

  it("steps and resets the font size, and ignores commands that are not settings", () => {
    expect(codeSettingsCommandPatch("view.fontIncrease", settings)).toEqual({ fontSize: settings.fontSize + 1 });
    expect(codeSettingsCommandPatch("view.fontReset", { ...settings, fontSize: 20 })).toEqual({ fontSize: CODE_EDITOR_DEFAULTS.fontSize });
    expect(codeSettingsCommandPatch("file.close", settings)).toBeNull();
    expect(codeSettingsToggleState("file.close", settings)).toBeNull();
  });
});
