export const ZORAI_MONACO_THEME = "zorai";

export type MonacoGuiColors = {
  surface: string;
  elevated: string;
  panel: string;
  text: string;
  muted: string;
  accent: string;
  accentSecondary: string;
  border: string;
  success: string;
  danger: string;
  warning: string;
  info: string;
  agent: string;
  human: string;
  reasoning: string;
};

export type MonacoThemeData = {
  base: "vs" | "vs-dark";
  inherit: boolean;
  rules: Array<{ token: string; foreground?: string; fontStyle?: string }>;
  colors: Record<string, string>;
};

type Rgba = { r: number; g: number; b: number; a: number };

type MonacoThemeApi = {
  editor: {
    defineTheme: (name: string, theme: MonacoThemeData) => void;
    setTheme: (name: string) => void;
  };
};

const GUI_COLOR_VARS: Record<keyof MonacoGuiColors, string> = {
  surface: "--zorai-bg-surface",
  elevated: "--zorai-bg-elevated",
  panel: "--zorai-bg-panel",
  text: "--zorai-text",
  muted: "--zorai-muted",
  accent: "--zorai-accent",
  accentSecondary: "--zorai-accent-secondary",
  border: "--zorai-border",
  success: "--success",
  danger: "--danger",
  warning: "--warning",
  info: "--info",
  agent: "--agent",
  human: "--human",
  reasoning: "--reasoning",
};

export const DEFAULT_MONACO_GUI_COLORS: MonacoGuiColors = {
  surface: "#141c28",
  elevated: "#212e3e",
  panel: "#191919",
  text: "#f2f2f2",
  muted: "#9aa8ba",
  accent: "#5ee7df",
  accentSecondary: "#a78bfa",
  border: "rgba(255, 255, 255, 0.10)",
  success: "#4ade80",
  danger: "#f87171",
  warning: "#fbbf24",
  info: "#60a5fa",
  agent: "#64b5f6",
  human: "#4ade80",
  reasoning: "#c4b5fd",
};

const DEFAULT_MONO_FONT = "ui-monospace, SFMono-Regular, Menlo, Consolas, monospace";

let appliedSignature = "";

export function parseCssColor(input: string): Rgba | null {
  const value = input.trim().toLowerCase();
  if (!value) return null;
  if (value === "transparent") return { r: 0, g: 0, b: 0, a: 0 };
  if (value.startsWith("#")) return parseHexColor(value.slice(1));
  const modern = value.match(/^rgba?\(\s*([\d.]+%?)\s+([\d.]+%?)\s+([\d.]+%?)(?:\s*\/\s*([\d.]+%?))?\s*\)$/);
  const legacy = value.match(/^rgba?\(\s*([\d.]+%?)\s*,\s*([\d.]+%?)\s*,\s*([\d.]+%?)(?:\s*,\s*([\d.]+%?))?\s*\)$/);
  const match = modern ?? legacy;
  if (!match) return null;
  return {
    r: colorChannel(match[1]),
    g: colorChannel(match[2]),
    b: colorChannel(match[3]),
    a: alphaChannel(match[4]),
  };
}

export function buildMonacoGuiTheme(colors: MonacoGuiColors): MonacoThemeData {
  const surface = paint(colors.surface, DEFAULT_MONACO_GUI_COLORS.surface);
  const elevated = paint(colors.elevated, DEFAULT_MONACO_GUI_COLORS.elevated);
  const text = paint(colors.text, DEFAULT_MONACO_GUI_COLORS.text);
  const muted = paint(composite(colors.muted, colors.surface) ?? colors.muted, DEFAULT_MONACO_GUI_COLORS.muted);
  const accent = paint(colors.accent, DEFAULT_MONACO_GUI_COLORS.accent);
  const accentSecondary = paint(colors.accentSecondary, DEFAULT_MONACO_GUI_COLORS.accentSecondary);
  const success = paint(colors.success, DEFAULT_MONACO_GUI_COLORS.success);
  const danger = paint(colors.danger, DEFAULT_MONACO_GUI_COLORS.danger);
  const warning = paint(colors.warning, DEFAULT_MONACO_GUI_COLORS.warning);
  const info = paint(colors.info, DEFAULT_MONACO_GUI_COLORS.info);
  const agent = paint(colors.agent, DEFAULT_MONACO_GUI_COLORS.agent);
  const human = paint(colors.human, DEFAULT_MONACO_GUI_COLORS.human);
  const reasoning = paint(colors.reasoning, DEFAULT_MONACO_GUI_COLORS.reasoning);
  const border = tint(colors.border, 1, "#ffffff1a");
  const light = relativeLuminance(surface) > 0.55;
  const token = (color: string) => color.slice(1);
  const rule = (name: string, color: string) => ({ token: name, foreground: token(color) });

  return {
    base: light ? "vs" : "vs-dark",
    inherit: false,
    rules: [
      rule("", text),
      rule("comment", muted),
      rule("string", human),
      rule("string.key", accent),
      rule("string.key.json", accent),
      rule("string.value.json", human),
      rule("keyword", accent),
      rule("keyword.json", reasoning),
      rule("number", warning),
      rule("regexp", warning),
      rule("type", agent),
      rule("class", agent),
      rule("interface", agent),
      rule("namespace", agent),
      rule("function", accentSecondary),
      rule("variable", text),
      rule("identifier", text),
      rule("constant", warning),
      rule("delimiter", muted),
      rule("operator", accent),
      rule("tag", agent),
      rule("attribute.name", warning),
      rule("attribute.value", human),
      rule("key", accent),
      rule("annotation", reasoning),
      rule("invalid", danger),
      rule("diff-add", success),
      rule("diff-remove", danger),
      rule("diff-hunk", accent),
      rule("diff-meta", muted),
    ],
    colors: {
      "editor.background": surface,
      "editor.foreground": text,
      "editorCursor.foreground": accent,
      "editor.selectionBackground": tint(accent, 0.28, `${accent}47`),
      "editor.inactiveSelectionBackground": tint(accent, 0.16, `${accent}29`),
      "editor.selectionHighlightBackground": tint(accent, 0.18, `${accent}2e`),
      "editor.wordHighlightBackground": tint(accent, 0.14, `${accent}24`),
      "editor.lineHighlightBackground": tint(text, light ? 0.06 : 0.05, "#ffffff0d"),
      "editorLineNumber.foreground": muted,
      "editorLineNumber.activeForeground": text,
      "editorGutter.background": surface,
      "editorIndentGuide.background1": border,
      "editorIndentGuide.activeBackground1": tint(accent, 0.45, `${accent}73`),
      "editorWhitespace.foreground": tint(muted, 0.45, `${muted}73`),
      "editorBracketMatch.background": tint(accent, 0.18, `${accent}2e`),
      "editorBracketMatch.border": accent,
      "editorWidget.background": elevated,
      "editorWidget.foreground": text,
      "editorWidget.border": border,
      "editorHoverWidget.background": elevated,
      "editorHoverWidget.border": border,
      "editorSuggestWidget.background": elevated,
      "editorSuggestWidget.foreground": text,
      "editorSuggestWidget.border": border,
      "editorSuggestWidget.selectedBackground": tint(accent, 0.22, `${accent}38`),
      "editorSuggestWidget.highlightForeground": accent,
      "editorError.foreground": danger,
      "editorWarning.foreground": warning,
      "editorInfo.foreground": info,
      "editorOverviewRuler.border": border,
      "editor.findMatchBackground": tint(warning, 0.35, `${warning}59`),
      "editor.findMatchHighlightBackground": tint(warning, 0.18, `${warning}2e`),
      "diffEditor.insertedTextBackground": tint(success, 0.16, `${success}29`),
      "diffEditor.insertedLineBackground": tint(success, 0.1, `${success}1a`),
      "diffEditor.removedTextBackground": tint(danger, 0.16, `${danger}29`),
      "diffEditor.removedLineBackground": tint(danger, 0.1, `${danger}1a`),
      "peekView.border": accent,
      "peekViewEditor.background": surface,
      "peekViewResult.background": elevated,
      "input.background": elevated,
      "input.foreground": text,
      "input.border": border,
      "input.placeholderForeground": muted,
      "focusBorder": accent,
      "list.hoverBackground": tint(text, 0.08, "#ffffff14"),
      "list.activeSelectionBackground": tint(accent, 0.22, `${accent}38`),
      "list.activeSelectionForeground": text,
      "list.highlightForeground": accent,
      "menu.background": elevated,
      "menu.foreground": text,
      "menu.selectionBackground": tint(accent, 0.22, `${accent}38`),
      "menu.selectionForeground": text,
      "menu.separatorBackground": border,
      "scrollbarSlider.background": tint(muted, 0.35, `${muted}59`),
      "scrollbarSlider.hoverBackground": tint(muted, 0.55, `${muted}8c`),
      "scrollbarSlider.activeBackground": tint(accent, 0.55, `${accent}8c`),
      "minimap.background": surface,
      "minimapSlider.background": tint(muted, 0.28, `${muted}47`),
      "minimapSlider.hoverBackground": tint(muted, 0.4, `${muted}66`),
      "minimapSlider.activeBackground": tint(accent, 0.4, `${accent}66`),
      "badge.background": accent,
      "badge.foreground": light ? surface : paint(DEFAULT_MONACO_GUI_COLORS.panel, "#191919"),
      foreground: text,
      descriptionForeground: muted,
      "icon.foreground": muted,
      errorForeground: danger,
    },
  };
}

export function readMonacoGuiColors(): MonacoGuiColors | null {
  if (typeof document === "undefined") return null;
  const probe = document.createElement("span");
  probe.style.position = "absolute";
  probe.style.pointerEvents = "none";
  document.documentElement.appendChild(probe);
  const rootStyle = getComputedStyle(document.documentElement);
  const colors = {} as MonacoGuiColors;
  try {
    for (const [key, variable] of Object.entries(GUI_COLOR_VARS) as Array<[keyof MonacoGuiColors, string]>) {
      if (!rootStyle.getPropertyValue(variable).trim()) return null;
      probe.style.color = `var(${variable})`;
      const resolved = getComputedStyle(probe).color;
      if (!parseCssColor(resolved)) return null;
      colors[key] = resolved;
    }
  } finally {
    probe.remove();
  }
  return colors;
}

export function readGuiMonoFontFamily(): string {
  if (typeof document === "undefined") return DEFAULT_MONO_FONT;
  const value = getComputedStyle(document.documentElement).getPropertyValue("--font-mono").trim();
  return value || DEFAULT_MONO_FONT;
}

export function resolveMonacoFontFamily(configured: string, computedMono = readGuiMonoFontFamily()): string {
  const value = configured.trim();
  if (!value || /\bvar\s*\(/i.test(value)) return computedMono.trim() || DEFAULT_MONO_FONT;
  return value;
}

export function applyMonacoGuiTheme(monacoApi: MonacoThemeApi, colors = readMonacoGuiColors() ?? DEFAULT_MONACO_GUI_COLORS): MonacoThemeData {
  const theme = buildMonacoGuiTheme(colors);
  const signature = JSON.stringify(theme);
  if (signature !== appliedSignature) {
    appliedSignature = signature;
    monacoApi.editor.defineTheme(ZORAI_MONACO_THEME, theme);
  }
  monacoApi.editor.setTheme(ZORAI_MONACO_THEME);
  return theme;
}

export function subscribeMonacoGuiTheme(monacoApi: MonacoThemeApi): () => void {
  const apply = () => applyMonacoGuiTheme(monacoApi);
  apply();
  if (typeof document === "undefined" || typeof MutationObserver === "undefined") return () => {};
  const observer = new MutationObserver(apply);
  observer.observe(document.documentElement, { attributes: true, attributeFilter: ["style", "class"] });
  return () => observer.disconnect();
}

function parseHexColor(hex: string): Rgba | null {
  const expanded = hex.length === 3 || hex.length === 4 ? [...hex].map((char) => `${char}${char}`).join("") : hex;
  if ((expanded.length !== 6 && expanded.length !== 8) || !/^[0-9a-f]+$/.test(expanded)) return null;
  return {
    r: Number.parseInt(expanded.slice(0, 2), 16),
    g: Number.parseInt(expanded.slice(2, 4), 16),
    b: Number.parseInt(expanded.slice(4, 6), 16),
    a: expanded.length === 8 ? Number.parseInt(expanded.slice(6, 8), 16) / 255 : 1,
  };
}

function colorChannel(raw: string): number {
  const value = raw.endsWith("%") ? (Number.parseFloat(raw) / 100) * 255 : Number.parseFloat(raw);
  return clamp(Math.round(value), 0, 255);
}

function alphaChannel(raw: string | undefined): number {
  if (!raw) return 1;
  const value = raw.endsWith("%") ? Number.parseFloat(raw) / 100 : Number.parseFloat(raw);
  return clamp(value, 0, 1);
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

function hexByte(value: number): string {
  return clamp(Math.round(value), 0, 255).toString(16).padStart(2, "0");
}

function hexColor(color: Rgba, alpha = color.a): string {
  const rgb = `${hexByte(color.r)}${hexByte(color.g)}${hexByte(color.b)}`;
  if (alpha >= 0.999) return `#${rgb}`;
  return `#${rgb}${hexByte(alpha * 255)}`;
}

function paint(input: string, fallback: string): string {
  const color = parseCssColor(input) ?? parseCssColor(fallback);
  return color ? hexColor({ ...color, a: 1 }) : "#000000";
}

function tint(input: string, alpha: number, fallback: string): string {
  const color = parseCssColor(input);
  if (!color) return fallback;
  return hexColor(color, color.a * alpha);
}

function composite(foreground: string, background: string): string | null {
  const fg = parseCssColor(foreground);
  const bg = parseCssColor(background);
  if (!fg) return null;
  if (fg.a >= 0.999 || !bg) return hexColor({ ...fg, a: 1 });
  const alpha = fg.a;
  return hexColor({
    r: fg.r * alpha + bg.r * (1 - alpha),
    g: fg.g * alpha + bg.g * (1 - alpha),
    b: fg.b * alpha + bg.b * (1 - alpha),
    a: 1,
  });
}

function relativeLuminance(hex: string): number {
  const color = parseCssColor(hex);
  if (!color) return 0;
  const channels = [color.r, color.g, color.b].map((channel) => {
    const value = channel / 255;
    return value <= 0.03928 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * channels[0] + 0.7152 * channels[1] + 0.0722 * channels[2];
}
