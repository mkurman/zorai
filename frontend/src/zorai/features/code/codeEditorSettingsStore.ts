import { create } from "zustand";
import { createStore } from "zustand/vanilla";
import { createJSONStorage, persist, type StateStorage } from "zustand/middleware";

export type CodeWordWrapMode = "off" | "viewport" | "column" | "bounded";
export type CodeAutoSaveMode = "off" | "after_delay" | "editor_focus_lost" | "code_window_focus_lost";
export type CodeEditorSettings = {
  fontFamily: string; fontSize: number; lineHeight: number; tabSize: number; insertSpaces: boolean;
  wordWrap: CodeWordWrapMode; wordWrapColumn: number; formatOnPaste: boolean; formatOnType: boolean; formatOnSave: boolean;
  trimTrailingWhitespaceOnSave: boolean; finalNewlineOnSave: boolean; autoSave: CodeAutoSaveMode; autoSaveDelayMs: number;
  reopenEditors: boolean; restoreViewState: boolean; maxFileSizeMb: number; preloadMonaco: boolean; cacheDocuments: number;
  performanceLogging: boolean; minimap: boolean; stickyScroll: boolean; renderWhitespace: "none" | "boundary" | "selection" | "trailing" | "all";
  lineNumbers: "on" | "off" | "relative"; bracketGuides: boolean; smoothScrolling: boolean; glyphMargin: boolean;
  lspEnabled: boolean; diagnosticsDelayMs: number; semanticHighlighting: boolean; keybindings: Record<string, string | null>;
};

export const CODE_EDITOR_SETTINGS_STORE_NAME = "zorai-code-editor-settings";
export const CODE_EDITOR_SETTINGS_STORE_VERSION = 1;
export const CODE_EDITOR_DEFAULTS: Readonly<CodeEditorSettings> = Object.freeze({
  fontFamily: "var(--font-mono)", fontSize: 13, lineHeight: 20, tabSize: 2, insertSpaces: true,
  wordWrap: "bounded", wordWrapColumn: 120, formatOnPaste: false, formatOnType: false, formatOnSave: false,
  trimTrailingWhitespaceOnSave: false, finalNewlineOnSave: false, autoSave: "off", autoSaveDelayMs: 1000,
  reopenEditors: true, restoreViewState: true, maxFileSizeMb: 5, preloadMonaco: true, cacheDocuments: 32,
  performanceLogging: false, minimap: true, stickyScroll: true, renderWhitespace: "selection", lineNumbers: "on",
  bracketGuides: true, smoothScrolling: true, glyphMargin: true, lspEnabled: true, diagnosticsDelayMs: 250,
  semanticHighlighting: true, keybindings: {},
});

const number = (value: unknown, fallback: number, min: number, max: number) => typeof value === "number" && Number.isFinite(value) ? Math.min(max, Math.max(min, Math.round(value))) : fallback;
const boolean = (value: unknown, fallback: boolean) => typeof value === "boolean" ? value : fallback;
const choice = <T extends string>(value: unknown, choices: readonly T[], fallback: T): T => typeof value === "string" && choices.includes(value as T) ? value as T : fallback;

export function normalizeCodeEditorSettings(input: unknown): CodeEditorSettings {
  const raw = input && typeof input === "object" ? input as Partial<CodeEditorSettings> : {};
  const bindings = raw.keybindings && typeof raw.keybindings === "object" ? Object.fromEntries(Object.entries(raw.keybindings).filter(([id, binding]) => /^[a-z][\w.-]+$/.test(id) && (binding === null || typeof binding === "string"))) : {};
  return {
    fontFamily: typeof raw.fontFamily === "string" && raw.fontFamily.trim() ? raw.fontFamily.trim() : CODE_EDITOR_DEFAULTS.fontFamily,
    fontSize: number(raw.fontSize, 13, 8, 40), lineHeight: number(raw.lineHeight, 20, 12, 60), tabSize: number(raw.tabSize, 2, 1, 8), insertSpaces: boolean(raw.insertSpaces, true),
    wordWrap: choice(raw.wordWrap, ["off", "viewport", "column", "bounded"], "bounded"), wordWrapColumn: number(raw.wordWrapColumn, 120, 40, 500),
    formatOnPaste: boolean(raw.formatOnPaste, false), formatOnType: boolean(raw.formatOnType, false), formatOnSave: boolean(raw.formatOnSave, false),
    trimTrailingWhitespaceOnSave: boolean(raw.trimTrailingWhitespaceOnSave, false), finalNewlineOnSave: boolean(raw.finalNewlineOnSave, false),
    autoSave: choice(raw.autoSave, ["off", "after_delay", "editor_focus_lost", "code_window_focus_lost"], "off"), autoSaveDelayMs: number(raw.autoSaveDelayMs, 1000, 100, 60000),
    reopenEditors: boolean(raw.reopenEditors, true), restoreViewState: boolean(raw.restoreViewState, true), maxFileSizeMb: number(raw.maxFileSizeMb, 5, 1, 100),
    preloadMonaco: boolean(raw.preloadMonaco, true), cacheDocuments: number(raw.cacheDocuments, 32, 1, 200), performanceLogging: boolean(raw.performanceLogging, false),
    minimap: boolean(raw.minimap, true), stickyScroll: boolean(raw.stickyScroll, true), renderWhitespace: choice(raw.renderWhitespace, ["none", "boundary", "selection", "trailing", "all"], "selection"),
    lineNumbers: choice(raw.lineNumbers, ["on", "off", "relative"], "on"), bracketGuides: boolean(raw.bracketGuides, true), smoothScrolling: boolean(raw.smoothScrolling, true), glyphMargin: boolean(raw.glyphMargin, true),
    lspEnabled: boolean(raw.lspEnabled, true), diagnosticsDelayMs: number(raw.diagnosticsDelayMs, 250, 0, 5000), semanticHighlighting: boolean(raw.semanticHighlighting, true), keybindings: bindings,
  };
}

type Store = { settings: CodeEditorSettings; updateSetting: <K extends keyof CodeEditorSettings>(key: K, value: CodeEditorSettings[K]) => void; resetSettings: () => void };
const initial = (): CodeEditorSettings => ({ ...CODE_EDITOR_DEFAULTS, keybindings: {} });
const memoryStorage = (): StateStorage => { const values = new Map<string, string>(); return { getItem: (key) => values.get(key) ?? null, setItem: (key, value) => void values.set(key, value), removeItem: (key) => void values.delete(key) }; };
const creator = (set: (partial: Partial<Store> | ((state: Store) => Partial<Store>)) => void): Store => ({ settings: initial(), updateSetting: (key, value) => set((state: Store) => ({ settings: normalizeCodeEditorSettings({ ...state.settings, [key]: value }) })), resetSettings: () => set({ settings: initial() }) });
const config = (storage: StateStorage) => ({ name: CODE_EDITOR_SETTINGS_STORE_NAME, version: CODE_EDITOR_SETTINGS_STORE_VERSION, storage: createJSONStorage<{ settings: CodeEditorSettings }>(() => storage), partialize: (state: Store) => ({ settings: state.settings }), merge: (persisted: unknown, current: Store) => ({ ...current, settings: normalizeCodeEditorSettings((persisted as { settings?: unknown } | null)?.settings) }), migrate: (persisted: unknown) => ({ settings: normalizeCodeEditorSettings((persisted as { settings?: unknown } | null)?.settings) }) });
export const createCodeEditorSettingsStore = (storage: StateStorage = memoryStorage()) => createStore<Store>()(persist(creator, config(storage)));
export const useCodeEditorSettingsStore = create<Store>()(persist(creator, config(typeof localStorage !== "undefined" ? localStorage : memoryStorage())));

export function codeSettingsToggleState(id: string, settings: CodeEditorSettings): boolean | null {
  if (id === "view.toggleMinimap") return settings.minimap;
  if (id === "view.toggleWrap") return settings.wordWrap !== "off";
  if (id === "view.toggleWhitespace") return settings.renderWhitespace !== "none";
  if (id === "view.toggleStickyScroll") return settings.stickyScroll;
  return null;
}

export function codeSettingsCommandPatch(id: string, settings: CodeEditorSettings): Partial<CodeEditorSettings> | null {
  if (id === "view.toggleMinimap") return { minimap: !settings.minimap };
  if (id === "view.toggleWrap") return { wordWrap: settings.wordWrap === "off" ? CODE_EDITOR_DEFAULTS.wordWrap : "off" };
  if (id === "view.toggleWhitespace") return { renderWhitespace: settings.renderWhitespace === "none" ? "all" : "none" };
  if (id === "view.toggleStickyScroll") return { stickyScroll: !settings.stickyScroll };
  if (id === "view.fontIncrease") return { fontSize: settings.fontSize + 1 };
  if (id === "view.fontDecrease") return { fontSize: settings.fontSize - 1 };
  if (id === "view.fontReset") return { fontSize: CODE_EDITOR_DEFAULTS.fontSize };
  return null;
}
