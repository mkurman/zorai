import { BUILTIN_THEMES, type TerminalTheme, type TerminalThemeColors, type ThemeAppearance, type ThemeFonts } from "@/lib/themes";
import { accentShades } from "@/lib/themesAppShell";
import { resolveFonts, userThemeToFile } from "@/lib/userThemes";

export type ThemeDraft = {
  id: string | null;
  name: string;
  author: string;
  appearance: ThemeAppearance;
  base: string;
  colors: TerminalThemeColors;
  appShell: Record<string, string>;
  blur: number;
  dim: number;
  surfaceOpacity: number;
  seeThrough: boolean;
  pair: string | null;
  corners: "default" | "square";
  borderStrength: number;
  fonts: Partial<ThemeFonts>;
  css: string;
  backgroundPath: string | null;
  backgroundSource: string | null | undefined;
};

export function draftFromTheme(theme: TerminalTheme, takenNames: string[]): ThemeDraft {
  const editing = Boolean(theme.user);
  let name = theme.name;
  if (!editing) {
    const taken = new Set(takenNames.map((candidate) => candidate.toLowerCase()));
    name = `${theme.name} Custom`;
    for (let index = 2; taken.has(name.toLowerCase()); index += 1) name = `${theme.name} Custom ${index}`;
  }
  return {
    id: theme.user?.id ?? null,
    name,
    author: theme.author && editing ? theme.author : "",
    appearance: theme.appearance,
    base: theme.user?.base ?? theme.name,
    colors: { ...theme.colors },
    appShell: { ...theme.appShell },
    blur: theme.user?.blur ?? 12,
    dim: theme.user?.dim ?? 0.3,
    surfaceOpacity: theme.user?.surfaceOpacity ?? 1,
    seeThrough: theme.user?.seeThrough ?? false,
    pair: theme.user?.pair ?? null,
    corners: theme.user?.corners ?? "default",
    borderStrength: theme.user?.borderStrength ?? 1,
    fonts: { ...theme.user?.fontOverrides },
    css: theme.user?.css ?? "",
    backgroundPath: theme.user?.backgroundPath ?? null,
    backgroundSource: undefined,
  };
}

export function rebaseDraft(draft: ThemeDraft, baseName: string): ThemeDraft {
  const base = BUILTIN_THEMES.find((theme) => theme.name === baseName);
  if (!base) return draft;
  return { ...draft, base: base.name, appearance: base.appearance, colors: { ...base.colors }, appShell: { ...base.appShell } };
}

export function themeFromDraft(draft: ThemeDraft): TerminalTheme {
  const base = BUILTIN_THEMES.find((theme) => theme.name === draft.base);
  return {
    name: draft.name.trim() || "Untitled theme",
    author: draft.author.trim(),
    appearance: draft.appearance,
    colors: draft.colors,
    appShell: draft.appShell,
    fonts: resolveFonts(base?.fonts, Object.fromEntries(Object.entries(draft.fonts).filter(([, value]) => value?.trim()))),
    user: {
      id: draft.id ?? "__draft__",
      base: draft.base,
      css: draft.css,
      backgroundPath: draft.backgroundPath,
      blur: draft.blur,
      dim: draft.dim,
      surfaceOpacity: draft.surfaceOpacity,
      seeThrough: draft.seeThrough,
      pair: draft.pair,
      corners: draft.corners,
      borderStrength: draft.borderStrength,
      fontOverrides: Object.fromEntries(Object.entries(draft.fonts).filter(([, value]) => value?.trim())),
    },
  };
}

export function withAccent(draft: ThemeDraft, accent: string): ThemeDraft {
  return { ...draft, appShell: { ...draft.appShell, "--accent": accent, ...accentShades(accent, draft.appearance) } };
}

export function draftSavePayload(draft: ThemeDraft) {
  const keptImage = draft.backgroundSource === undefined && draft.backgroundPath ? draft.backgroundPath.split(/[\\/]/).pop() : undefined;
  return {
    id: draft.id,
    theme: userThemeToFile(themeFromDraft(draft), keptImage),
    css: draft.css,
    backgroundSource: draft.backgroundSource,
  };
}

export function draftNameProblem(draft: ThemeDraft, themes: TerminalTheme[]): string | null {
  const name = draft.name.trim();
  if (!name) return "Give the theme a name.";
  const clash = themes.find((theme) => theme.name.toLowerCase() === name.toLowerCase() && (!theme.user || theme.user.id !== draft.id));
  return clash ? `"${clash.name}" already exists. Pick another name.` : null;
}

export function colorInputValue(value: string | undefined): string {
  const text = (value ?? "").trim();
  const hex = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(text)?.[1];
  if (hex) return `#${hex.length === 3 ? hex.split("").map((char) => char + char).join("") : hex}`.toLowerCase();
  const rgb = /^rgba?\(\s*([\d.]+)[\s,]+([\d.]+)[\s,]+([\d.]+)/i.exec(text);
  if (rgb) return `#${rgb.slice(1, 4).map((channel) => Math.round(Math.min(255, Number(channel))).toString(16).padStart(2, "0")).join("")}`;
  return "#000000";
}
