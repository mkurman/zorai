import { BUILTIN_THEMES, defaultThemeNameForAppearance, type TerminalTheme, type TerminalThemeColors } from "./themes";
import type { AppShellTheme } from "./themesAppShell";
import { buildUiFontFamilyStack } from "./uiInterfacePrefs";

const DEFAULT_MONO_STACK = "ui-monospace, SFMono-Regular, Consolas, monospace";
const BORDER_VARIABLES = ["--border", "--border-color", "--border-strong", "--border-color-strong", "--border-subtle", "--glass-border", "--glass-border-light"];

export const USER_THEME_FORMAT_VERSION = 1;

export const THEME_EDITOR_SHELL_VARIABLES = [
  { key: "--bg-void", label: "Window background" },
  { key: "--bg-primary", label: "Panels" },
  { key: "--bg-secondary", label: "Selected rows" },
  { key: "--bg-tertiary", label: "Inputs and cards" },
  { key: "--bg-elevated", label: "Menus and dialogs" },
  { key: "--text-primary", label: "Text" },
  { key: "--text-secondary", label: "Secondary text" },
  { key: "--text-muted", label: "Muted text" },
  { key: "--accent", label: "Accent" },
  { key: "--border", label: "Borders" },
] as const;

export const THEME_EDITOR_TERMINAL_COLORS: Array<keyof TerminalThemeColors> = [
  "background", "foreground", "cursor", "selectionBg",
  "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white",
  "brightBlack", "brightRed", "brightGreen", "brightYellow", "brightBlue", "brightMagenta", "brightCyan", "brightWhite",
];

const PANEL_SURFACES = ["--bg-primary", "--bg-secondary"];
const RAISED_SURFACES = ["--bg-tertiary", "--bg-surface", "--bg-elevated", "--bg-overlay"];

function clampNumber(value: unknown, min: number, max: number, fallback: number): number {
  const number = typeof value === "number" && Number.isFinite(value) ? value : fallback;
  return Math.min(max, Math.max(min, number));
}

function stringEntries(value: unknown, accept: (key: string) => boolean): Record<string, string> {
  if (!value || typeof value !== "object") return {};
  return Object.fromEntries(Object.entries(value).filter(([key, entry]) => accept(key) && typeof entry === "string" && entry.trim()));
}

export type ResolvedUserThemes = { themes: TerminalTheme[]; errors: Array<{ id: string; message: string }> };

export function resolveUserThemes(records: ZoraiUserThemeRecord[], builtins: TerminalTheme[] = BUILTIN_THEMES): ResolvedUserThemes {
  const taken = new Set(builtins.map((theme) => theme.name.toLowerCase()));
  const themes: TerminalTheme[] = [];
  const errors: ResolvedUserThemes["errors"] = [];
  for (const record of records) {
    const file = record.theme;
    if (!file) {
      errors.push({ id: record.id, message: record.error ?? "Unreadable theme." });
      continue;
    }
    const name = file.name.trim();
    if (taken.has(name.toLowerCase())) {
      errors.push({ id: record.id, message: `Another theme is already named "${name}". Rename one of them.` });
      continue;
    }
    taken.add(name.toLowerCase());
    if (record.error) errors.push({ id: record.id, message: record.error });
    const fontOverrides = stringEntries(file.fonts, (key) => ["ui", "mono", "display"].includes(key));
    const baseName = file.base ?? defaultThemeNameForAppearance(file.appearance);
    const base = builtins.find((theme) => theme.name.toLowerCase() === baseName.toLowerCase())
      ?? builtins.find((theme) => theme.name === defaultThemeNameForAppearance(file.appearance))!;
    themes.push({
      name,
      author: file.author ?? "",
      appearance: file.appearance,
      colors: { ...base.colors, ...stringEntries(file.colors, (key) => key in base.colors || key === "selectionFg") } as TerminalThemeColors,
      appShell: { ...base.appShell, ...stringEntries(file.appShell, (key) => key.startsWith("--")) },
      fonts: resolveFonts(base.fonts, fontOverrides),
      user: {
        id: record.id,
        base: base.name,
        css: record.css,
        backgroundPath: record.backgroundPath,
        blur: clampNumber(file.background?.blur, 0, 60, 0),
        dim: clampNumber(file.background?.dim, 0, 0.9, 0),
        surfaceOpacity: clampNumber(file.surfaceOpacity, 0.2, 1, 1),
        seeThrough: file.seeThrough === true,
        pair: typeof file.pair === "string" && file.pair.trim() ? file.pair.trim() : null,
        corners: file.corners === "square" ? "square" : "default",
        borderStrength: clampNumber(file.borderStrength, 0, 2, 1),
        fontOverrides,
      },
    });
  }
  return { themes, errors };
}

export function resolveFonts(base: TerminalTheme["fonts"], overrides: Partial<NonNullable<TerminalTheme["fonts"]>>): TerminalTheme["fonts"] {
  if (!base && Object.keys(overrides).length === 0) return undefined;
  const uiFallback = base?.ui ?? buildUiFontFamilyStack("");
  const monoFallback = base?.mono ?? DEFAULT_MONO_STACK;
  const stack = (value: string | undefined, fallback: string) => !value ? undefined : value.includes(",") ? value : `"${value.replace(/^["']|["']$/g, "")}", ${fallback}`;
  const ui = stack(overrides.ui, uiFallback) ?? uiFallback;
  return {
    ui,
    mono: stack(overrides.mono, monoFallback) ?? monoFallback,
    display: stack(overrides.display, ui) ?? (overrides.ui ? ui : base?.display ?? ui),
  };
}

export function adjustBorders(shell: AppShellTheme, strength: number): AppShellTheme {
  if (strength === 1) return shell;
  const next = { ...shell };
  const text = shell["--text-primary"] ?? "#808080";
  for (const key of BORDER_VARIABLES) {
    if (!next[key]) continue;
    next[key] = strength < 1
      ? translucentColor(next[key], strength)
      : `color-mix(in srgb, ${next[key]}, ${text} ${Math.round((strength - 1) * 60)}%)`;
  }
  return next;
}

export function userThemeToFile(theme: TerminalTheme, existingImage?: string): ZoraiUserThemeFile {
  const base = BUILTIN_THEMES.find((candidate) => candidate.name === theme.user?.base);
  const changed = <T extends Record<string, string>>(next: T | undefined, previous: T | undefined) =>
    Object.fromEntries(Object.entries(next ?? {}).filter(([key, value]) => previous?.[key] !== value));
  const background = theme.user && (theme.user.blur || theme.user.dim || existingImage)
    ? { ...(existingImage ? { image: existingImage } : {}), blur: theme.user.blur, dim: theme.user.dim }
    : undefined;
  return {
    name: theme.name,
    ...(theme.author ? { author: theme.author } : {}),
    appearance: theme.appearance,
    ...(base ? { base: base.name } : {}),
    colors: changed(theme.colors as unknown as Record<string, string>, base?.colors as unknown as Record<string, string> | undefined),
    appShell: changed(theme.appShell, base?.appShell),
    ...(background ? { background } : {}),
    surfaceOpacity: theme.user?.surfaceOpacity ?? 1,
    seeThrough: theme.user?.seeThrough === true,
    ...(theme.user?.pair ? { pair: theme.user.pair } : {}),
    ...(theme.user?.corners === "square" ? { corners: "square" as const } : {}),
    ...(theme.user && theme.user.borderStrength !== 1 ? { borderStrength: theme.user.borderStrength } : {}),
    ...(theme.user && Object.keys(theme.user.fontOverrides).length ? { fonts: theme.user.fontOverrides } : {}),
  };
}

export function translucentColor(color: string, alpha: number): string {
  const value = color.trim();
  const hex = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(value)?.[1];
  if (hex) {
    const full = hex.length === 3 ? hex.split("").map((char) => char + char).join("") : hex;
    const [red, green, blue] = [0, 2, 4].map((offset) => Number.parseInt(full.slice(offset, offset + 2), 16));
    return `rgba(${red}, ${green}, ${blue}, ${Number(alpha.toFixed(3))})`;
  }
  const rgb = /^rgba?\(\s*([\d.]+)[\s,]+([\d.]+)[\s,]+([\d.]+)(?:[\s,/]+([\d.]+%?))?\s*\)$/i.exec(value);
  if (rgb) {
    const existing = rgb[4] === undefined ? 1 : rgb[4].endsWith("%") ? Number.parseFloat(rgb[4]) / 100 : Number(rgb[4]);
    return `rgba(${rgb[1]}, ${rgb[2]}, ${rgb[3]}, ${Number((existing * alpha).toFixed(3))})`;
  }
  return `color-mix(in srgb, ${value} ${Math.round(alpha * 100)}%, transparent)`;
}

export type ThemeBackdrop = "none" | "image" | "see-through";

export function isVideoBackground(path: string | null | undefined): boolean {
  return /\.(mp4|webm)$/i.test(path ?? "");
}

export function themeBackdrop(theme: TerminalTheme, seeThroughActive: boolean): ThemeBackdrop {
  if (!theme.user) return "none";
  if (theme.user.seeThrough && seeThroughActive) return "see-through";
  return theme.user.backgroundPath ? "image" : "none";
}

export function translucentShell(shell: AppShellTheme, backdrop: ThemeBackdrop, surfaceOpacity: number): AppShellTheme {
  if (backdrop === "none") return shell;
  const next = { ...shell };
  for (const key of PANEL_SURFACES) if (next[key]) next[key] = translucentColor(next[key], surfaceOpacity);
  for (const key of RAISED_SURFACES) if (next[key]) next[key] = translucentColor(next[key], Math.min(1, surfaceOpacity + (1 - surfaceOpacity) * 0.6));
  return next;
}
