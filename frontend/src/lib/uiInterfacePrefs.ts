import type { NightModePreference, UiFontScale } from "./types";

export const UI_FONT_SCALE_OPTIONS: ReadonlyArray<{ id: UiFontScale; label: string }> = [
  { id: "tiny", label: "Tiny" },
  { id: "small", label: "Small" },
  { id: "medium", label: "Medium" },
  { id: "large", label: "Large" },
  { id: "very-large", label: "Very large" },
  { id: "huge", label: "Huge" },
];

export const NIGHT_MODE_OPTIONS: ReadonlyArray<{ id: NightModePreference; label: string }> = [
  { id: "on", label: "On" },
  { id: "off", label: "Off" },
  { id: "auto", label: "Auto (system)" },
];

export const UI_FONT_SCALE_FACTORS: Record<UiFontScale, number> = {
  "tiny": 0.75,
  small: 0.875,
  medium: 1.0,
  large: 1.125,
  "very-large": 1.25,
  huge: 1.5,
};

const UI_FONT_FALLBACKS =
  '"JetBrains Mono", "Fira Code", "SF Mono", "Segoe UI", system-ui, sans-serif, monospace';

export type ResolvedAppearance = "dark" | "light";

export function normalizeUiFontScale(value: unknown): UiFontScale {
  if (value === "tiny" || value === "small" || value === "medium" || value === "large" || value === "very-large" || value === "huge") {
    return value;
  }
  return "medium";
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
    return `"Cascadia Code", ${UI_FONT_FALLBACKS}`;
  }
  const quoted = trimmed.includes(",") ? trimmed : `"${trimmed.replace(/"/g, '\\"')}"`;
  return `${quoted}, ${UI_FONT_FALLBACKS}`;
}

export function applyUiInterfacePrefs(options: {
  uiFontFamily: string;
  uiFontScale: UiFontScale;
  appearance: ResolvedAppearance;
}): void {
  if (typeof document === "undefined") {
    return;
  }

  const root = document.documentElement;
  const scale = UI_FONT_SCALE_FACTORS[normalizeUiFontScale(options.uiFontScale)];
  root.style.setProperty("--font-ui", buildUiFontFamilyStack(options.uiFontFamily));
  root.style.setProperty("--font-display", "var(--font-ui)");
  root.style.setProperty("--ui-font-scale", String(scale));
  root.dataset.appearance = options.appearance;
  root.style.colorScheme = options.appearance;
}
