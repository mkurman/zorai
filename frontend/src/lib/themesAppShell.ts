import type { TerminalThemeColors } from "./themes";
import { getEffectiveTheme, getThemeByName } from "./themes";

export type AppShellTheme = Record<string, string>;

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

function normalizeHex(hex: string): string {
  const value = hex.trim().replace(/^#/, "");
  if (value.length === 3) {
    return value
      .split("")
      .map((char) => `${char}${char}`)
      .join("");
  }
  return value.padEnd(6, "0").slice(0, 6);
}

function parseHex(hex: string): [number, number, number] {
  const normalized = normalizeHex(hex);
  return [
    Number.parseInt(normalized.slice(0, 2), 16),
    Number.parseInt(normalized.slice(2, 4), 16),
    Number.parseInt(normalized.slice(4, 6), 16),
  ];
}

function toHex(red: number, green: number, blue: number): string {
  return `#${[red, green, blue]
    .map((value) => clamp(Math.round(value), 0, 255).toString(16).padStart(2, "0"))
    .join("")}`;
}

function mix(colorA: string, colorB: string, amount: number): string {
  const [redA, greenA, blueA] = parseHex(colorA);
  const [redB, greenB, blueB] = parseHex(colorB);
  const ratio = clamp(amount, 0, 1);
  return toHex(
    redA + (redB - redA) * ratio,
    greenA + (greenB - greenA) * ratio,
    blueA + (blueB - blueA) * ratio
  );
}

function withAlpha(color: string, alpha: number): string {
  const [red, green, blue] = parseHex(color);
  return `rgba(${red}, ${green}, ${blue}, ${clamp(alpha, 0, 1)})`;
}

export function getAppShellTheme(
  themeName: string,
  useCustomColors: boolean,
  customBg?: string,
  customFg?: string,
  customCursor?: string,
  customSelection?: string,
  appearance: "dark" | "light" = "dark",
): AppShellTheme {
  const colors = getEffectiveTheme(
    themeName,
    useCustomColors,
    customBg,
    customFg,
    customCursor,
    customSelection
  );

  const appShell = useCustomColors ? undefined : getThemeByName(themeName).appShell;
  if (appearance === "light") {
    return { ...buildLightAppShellTheme(colors), ...appShell };
  }
  return { ...buildDarkAppShellTheme(colors), ...appShell };
}

function buildDarkAppShellTheme(colors: TerminalThemeColors): AppShellTheme {
  // New design system color mapping - consistent structure across all themes
  const bgVoid = mix(colors.background, "#000000", 0.5);
  const bgDeep = mix(colors.background, "#000000", 0.3);
  const bgPrimary = colors.background;
  const bgSecondary = mix(colors.background, colors.black, 0.38);
  const bgTertiary = mix(colors.background, colors.black, 0.2);
  const bgSurface = mix(colors.background, colors.white, 0.1);
  const bgElevated = mix(colors.background, colors.white, 0.16);

  // Agent-centric accent colors based on theme
  const accent = colors.cyan; // Cyan/teal for primary accent
  const agent = colors.blue;
  const human = colors.green;
  const approval = colors.yellow;
  const reasoning = colors.magenta;
  const mission = colors.cyan;
  const timeline = colors.brightMagenta;

  return {
    // Core backgrounds
    "--bg-void": bgVoid,
    "--bg-deep": bgDeep,
    "--bg-primary": bgPrimary,
    "--bg-secondary": bgSecondary,
    "--bg-tertiary": bgTertiary,
    "--bg-surface": bgSurface,
    "--bg-elevated": bgElevated,
    "--bg-canvas": mix(colors.background, "#000000", 0.6),
    "--bg-overlay": withAlpha(mix(colors.background, "#000000", 0.4), 0.85),

    // Text colors
    "--text-primary": colors.foreground,
    "--text-secondary": mix(colors.foreground, colors.background, 0.28),
    "--text-muted": withAlpha(colors.foreground, 0.55),
    "--text-disabled": withAlpha(colors.foreground, 0.35),

    // Primary accent
    "--accent": accent,
    "--accent-hover": mix(accent, "#ffffff", 0.2),
    "--accent-soft": withAlpha(accent, 0.12),
    "--accent-dim": withAlpha(accent, 0.06),

    // Agent lane colors
    "--agent": agent,
    "--agent-soft": withAlpha(agent, 0.14),
    "--agent-glow": withAlpha(agent, 0.25),
    "--human": human,
    "--human-soft": withAlpha(human, 0.14),
    "--human-glow": withAlpha(human, 0.25),
    "--approval": approval,
    "--approval-soft": withAlpha(approval, 0.14),
    "--approval-glow": withAlpha(approval, 0.3),
    "--reasoning": reasoning,
    "--reasoning-soft": withAlpha(reasoning, 0.14),
    "--reasoning-glow": withAlpha(reasoning, 0.25),
    "--mission": mission,
    "--mission-soft": withAlpha(mission, 0.14),
    "--mission-glow": withAlpha(mission, 0.25),
    "--timeline": timeline,
    "--timeline-soft": withAlpha(timeline, 0.14),

    // Status colors
    "--success": colors.green,
    "--success-soft": withAlpha(colors.green, 0.12),
    "--warning": colors.yellow,
    "--warning-soft": withAlpha(colors.yellow, 0.12),
    "--danger": colors.red,
    "--danger-soft": withAlpha(colors.red, 0.12),
    "--info": colors.blue,
    "--info-soft": withAlpha(colors.blue, 0.12),

    // Risk levels
    "--risk-low": withAlpha(colors.green, 0.1),
    "--risk-medium": withAlpha(colors.yellow, 0.12),
    "--risk-high": withAlpha(colors.red, 0.12),
    "--risk-critical": withAlpha(colors.red, 0.18),

    // Borders
    "--border": withAlpha(colors.white, 0.06),
    "--border-color": withAlpha(colors.white, 0.12),
    "--border-strong": withAlpha(colors.white, 0.1),
    "--border-color-strong": withAlpha(colors.white, 0.15),
    "--border-subtle": withAlpha(colors.white, 0.06),
    "--border-focus": withAlpha(accent, 0.4),
    "--glass-border": withAlpha(colors.white, 0.05),
    "--glass-border-light": withAlpha(colors.white, 0.1),

    "--accent-secondary": colors.magenta,
    "--accent-glow": withAlpha(accent, 0.2),
    "--text-on-accent": mix(colors.background, "#000000", 0.35),
    "--text-on-agent": mix(colors.background, "#000000", 0.35),
    "--text-on-human": mix(colors.background, "#000000", 0.35),
    "--text-inverse": mix(colors.background, "#000000", 0.35),

    // Shadows
    "--shadow-sm": `0 2px 8px ${withAlpha(colors.black, 0.3)}`,
    "--shadow-md": `0 4px 16px ${withAlpha(colors.black, 0.4)}`,
    "--shadow-lg": `0 8px 32px ${withAlpha(colors.black, 0.5)}`,
    "--shadow-xl": `0 16px 48px ${withAlpha(colors.black, 0.6)}`,
    "--shadow-glow-sm": `0 0 20px ${withAlpha(agent, 0.3)}`,
    "--shadow-glow-md": `0 0 40px ${withAlpha(agent, 0.3)}`,

    // Blur
    "--blur-sm": "8px",
    "--blur-md": "16px",
    "--blur-lg": "24px",
    "--blur-xl": "32px",
    "--panel-blur": "20px",

    // Legacy compatibility
    "--shadow-color": withAlpha(colors.black, 0.45),
    "--shade": "#000000",
  };
}

function relativeLuminance(hex: string): number {
  const [red, green, blue] = parseHex(hex).map((channel) => {
    const value = channel / 255;
    return value <= 0.03928 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * red + 0.7152 * green + 0.0722 * blue;
}

function ensureLightSurface(hex: string): string {
  return relativeLuminance(hex) < 0.72 ? mix(hex, "#ffffff", 0.82) : hex;
}

function ensureDarkInk(hex: string, fallback = "#152033"): string {
  return relativeLuminance(hex) > 0.45 ? fallback : hex;
}

function buildLightAppShellTheme(colors: TerminalThemeColors): AppShellTheme {
  // Drive chrome from the selected light palette (not a fixed paper sheet),
  // and lift toward white for depth so Color Style actually changes the shell.
  const bgPrimary = ensureLightSurface(colors.background);
  const ink = ensureDarkInk(colors.foreground);
  const mutedInk = mix(ink, bgPrimary, 0.22);
  const readableMuted = relativeLuminance(mutedInk) > 0.55 ? "#4a5a72" : mutedInk;
  const bgVoid = mix(bgPrimary, "#ffffff", 0.28);
  const bgDeep = mix(bgPrimary, "#ffffff", 0.12);
  const bgSecondary = mix(bgPrimary, ink, 0.04);
  const bgTertiary = mix(bgPrimary, ink, 0.07);
  const bgSurface = mix(bgPrimary, "#ffffff", 0.45);
  const bgElevated = mix(bgPrimary, "#ffffff", 0.72);
  const onAccent = "#ffffff";

  const accent = mix(colors.cyan, "#0f766e", 0.28);
  const accentSecondary = mix(colors.magenta, "#7e22ce", 0.22);
  const agent = mix(colors.blue, "#1d4ed8", 0.22);
  const human = mix(colors.green, "#15803d", 0.22);
  const approval = mix(colors.yellow, "#b45309", 0.28);
  const reasoning = mix(colors.magenta, "#7e22ce", 0.22);
  const mission = mix(colors.cyan, "#0e7490", 0.22);
  const timeline = mix(colors.brightMagenta, "#9d174d", 0.22);

  return {
    "--bg-void": bgVoid,
    "--bg-deep": bgDeep,
    "--bg-primary": bgPrimary,
    "--bg-secondary": bgSecondary,
    "--bg-tertiary": bgTertiary,
    "--bg-surface": bgSurface,
    "--bg-elevated": bgElevated,
    "--bg-canvas": mix(bgPrimary, "#ffffff", 0.55),
    "--bg-overlay": withAlpha(bgElevated, 0.92),

    "--text-primary": ink,
    "--text-secondary": readableMuted,
    "--text-muted": readableMuted,
    "--text-disabled": withAlpha(ink, 0.42),
    "--text-inverse": bgElevated,
    "--text-on-accent": onAccent,
    "--text-on-agent": onAccent,
    "--text-on-human": onAccent,

    "--accent": accent,
    "--accent-secondary": accentSecondary,
    "--accent-hover": mix(accent, "#000000", 0.12),
    "--accent-soft": withAlpha(accent, 0.14),
    "--accent-dim": withAlpha(accent, 0.07),
    "--accent-glow": withAlpha(accent, 0.22),

    "--agent": agent,
    "--agent-soft": withAlpha(agent, 0.12),
    "--agent-glow": withAlpha(agent, 0.18),
    "--human": human,
    "--human-soft": withAlpha(human, 0.12),
    "--human-glow": withAlpha(human, 0.18),
    "--approval": approval,
    "--approval-soft": withAlpha(approval, 0.12),
    "--approval-glow": withAlpha(approval, 0.2),
    "--reasoning": reasoning,
    "--reasoning-soft": withAlpha(reasoning, 0.12),
    "--reasoning-glow": withAlpha(reasoning, 0.18),
    "--mission": mission,
    "--mission-soft": withAlpha(mission, 0.12),
    "--mission-glow": withAlpha(mission, 0.18),
    "--timeline": timeline,
    "--timeline-soft": withAlpha(timeline, 0.12),

    "--success": mix(colors.green, "#15803d", 0.22),
    "--success-soft": withAlpha(colors.green, 0.12),
    "--warning": mix(colors.yellow, "#b45309", 0.28),
    "--warning-soft": withAlpha(colors.yellow, 0.12),
    "--danger": mix(colors.red, "#b91c1c", 0.22),
    "--danger-soft": withAlpha(colors.red, 0.12),
    "--info": mix(colors.blue, "#1d4ed8", 0.22),
    "--info-soft": withAlpha(colors.blue, 0.12),

    "--risk-low": withAlpha(colors.green, 0.1),
    "--risk-medium": withAlpha(colors.yellow, 0.12),
    "--risk-high": withAlpha(colors.red, 0.12),
    "--risk-critical": withAlpha(colors.red, 0.16),

    "--border": withAlpha(ink, 0.14),
    "--border-color": withAlpha(ink, 0.16),
    "--border-strong": withAlpha(ink, 0.22),
    "--border-color-strong": withAlpha(ink, 0.22),
    "--border-subtle": withAlpha(ink, 0.08),
    "--border-focus": withAlpha(accent, 0.5),
    "--glass-border": withAlpha(ink, 0.1),
    "--glass-border-light": withAlpha(ink, 0.12),

    "--shadow-sm": `0 2px 8px ${withAlpha(ink, 0.08)}`,
    "--shadow-md": `0 4px 16px ${withAlpha(ink, 0.1)}`,
    "--shadow-lg": `0 8px 32px ${withAlpha(ink, 0.12)}`,
    "--shadow-xl": `0 16px 48px ${withAlpha(ink, 0.14)}`,
    "--shadow-glow-sm": `0 0 20px ${withAlpha(agent, 0.16)}`,
    "--shadow-glow-md": `0 0 40px ${withAlpha(agent, 0.16)}`,

    "--blur-sm": "8px",
    "--blur-md": "16px",
    "--blur-lg": "24px",
    "--blur-xl": "32px",
    "--panel-blur": "20px",

    "--shadow-color": withAlpha(ink, 0.18),
    "--shade": ink,
  };
}

export function applyAppShellTheme(theme: AppShellTheme): void {
  if (typeof document === "undefined") {
    return;
  }

  const root = document.documentElement;
  for (const [name, value] of Object.entries(theme)) {
    root.style.setProperty(name, value);
  }
}
