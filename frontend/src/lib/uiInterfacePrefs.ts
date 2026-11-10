import type { ThemeFonts } from "./themes";
import type { ChatWidthPreference, NightModePreference, UiFontScale } from "./types";

export const UI_FONT_SCALE_MIN = 0.75;
export const UI_FONT_SCALE_MAX = 1.5;
export const UI_FONT_SCALE_STEP = 0.025;
export const UI_FONT_SCALE_TICKS: readonly number[] = Array.from(
  { length: Math.round((UI_FONT_SCALE_MAX - UI_FONT_SCALE_MIN) / UI_FONT_SCALE_STEP) + 1 },
  (_, index) => Math.round((UI_FONT_SCALE_MIN + index * UI_FONT_SCALE_STEP) * 1000) / 1000,
);

export const NIGHT_MODE_OPTIONS: ReadonlyArray<{ id: NightModePreference; label: string }> = [
  { id: "on", label: "On" },
  { id: "off", label: "Off" },
  { id: "auto", label: "Auto (system)" },
];

export const UI_FONT_SCALE_FACTORS: Record<string, number> = {
  "tiny": 0.75,
  small: 0.875,
  medium: 1.0,
  large: 1.125,
  "very-large": 1.25,
  huge: 1.5,
};

export const DEFAULT_UI_FONT = "Cascadia Code";

const UI_FONT_FALLBACKS =
  '"JetBrains Mono", "Fira Code", "SF Mono", "Segoe UI", system-ui, sans-serif, monospace';

export type ResolvedAppearance = "dark" | "light";

export function normalizeUiFontScale(value: unknown): UiFontScale {
  const factor = typeof value === "string" ? UI_FONT_SCALE_FACTORS[value] : value;
  if (typeof factor !== "number" || !Number.isFinite(factor)) {
    return 1;
  }
  const clamped = Math.min(UI_FONT_SCALE_MAX, Math.max(UI_FONT_SCALE_MIN, factor));
  return Math.round(Math.round(clamped / UI_FONT_SCALE_STEP) * UI_FONT_SCALE_STEP * 1000) / 1000;
}

export const CHAT_WIDTH_OPTIONS: ReadonlyArray<{ id: ChatWidthPreference; label: string; maxWidth: string }> = [
  { id: "narrow", label: "Narrow (620 px)", maxWidth: "620px" },
  { id: "medium-narrow", label: "Medium narrow (800 px)", maxWidth: "800px" },
  { id: "medium", label: "Medium (960 px)", maxWidth: "960px" },
  { id: "medium-wide", label: "Medium wide (1200 px)", maxWidth: "1200px" },
  { id: "wide", label: "Wide (1440 px)", maxWidth: "1440px" },
  { id: "very-wide", label: "Very wide (1680 px)", maxWidth: "1680px" },
  { id: "extra-wide", label: "Extra wide (1920 px)", maxWidth: "1920px" },
  { id: "ultra-wide", label: "Ultra wide (2560 px)", maxWidth: "2560px" },
  { id: "full", label: "Full width", maxWidth: "100%" },
];

export function normalizeChatWidth(value: unknown): ChatWidthPreference {
  return CHAT_WIDTH_OPTIONS.some((option) => option.id === value) ? value as ChatWidthPreference : "full";
}

export function chatMaxWidth(value: unknown): string {
  const id = normalizeChatWidth(value);
  return CHAT_WIDTH_OPTIONS.find((option) => option.id === id)?.maxWidth ?? "100%";
}

export function normalizeNightMode(value: unknown): NightModePreference {
  if (value === "on" || value === "off" || value === "auto") {
    return value;
  }
  return "on";
}

export function resolveAppearance(
  nightMode: NightModePreference,
  prefersDark = true,
): ResolvedAppearance {
  if (nightMode === "on") return "dark";
  if (nightMode === "off") return "light";
  return prefersDark ? "dark" : "light";
}

export function systemPrefersDark(): boolean {
  if (typeof window === "undefined" || typeof window.matchMedia !== "function") {
    return true;
  }
  return window.matchMedia("(prefers-color-scheme: dark)").matches;
}

export function buildUiFontFamilyStack(fontFamily: string): string {
  const trimmed = fontFamily.trim();
  if (!trimmed) {
    return `"${DEFAULT_UI_FONT}", ${UI_FONT_FALLBACKS}`;
  }
  const quoted = trimmed.includes(",") ? trimmed : `"${trimmed.replace(/"/g, '\\"')}"`;
  return `${quoted}, ${UI_FONT_FALLBACKS}`;
}

export function applyUiInterfacePrefs(options: {
  uiFontFamily: string;
  uiFontScale: unknown;
  appearance: ResolvedAppearance;
  themeName?: string;
  themeFonts?: ThemeFonts;
}): void {
  if (typeof document === "undefined") {
    return;
  }

  const root = document.documentElement;
  const scale = normalizeUiFontScale(options.uiFontScale);
  const userFont = options.uiFontFamily.trim();
  if (options.themeFonts && !userFont) {
    root.style.setProperty("--font-ui", options.themeFonts.ui);
    root.style.setProperty("--font-display", options.themeFonts.display);
  } else {
    root.style.setProperty("--font-ui", buildUiFontFamilyStack(userFont));
    root.style.setProperty("--font-display", "var(--font-ui)");
  }
  if (options.themeFonts) {
    root.style.setProperty("--font-mono", options.themeFonts.mono);
  } else {
    root.style.removeProperty("--font-mono");
  }
  root.style.setProperty("--ui-font-scale", String(scale));
  root.dataset.appearance = options.appearance;
  root.dataset.theme = (options.themeName ?? "").toLowerCase().replace(/[^a-z0-9]+/g, "-");
  root.style.colorScheme = options.appearance;
}
