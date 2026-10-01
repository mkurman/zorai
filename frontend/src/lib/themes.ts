/**
 * Terminal theme definitions matching zorai-windows, plus light companions for day mode.
 * Each theme has a 16-color ANSI palette + background/foreground/cursor/selection.
 */

export type ThemeAppearance = "dark" | "light";

export interface TerminalThemeColors {
  background: string;
  foreground: string;
  cursor: string;
  selectionBg: string;
  selectionFg?: string;
  black: string;
  red: string;
  green: string;
  yellow: string;
  blue: string;
  magenta: string;
  cyan: string;
  white: string;
  brightBlack: string;
  brightRed: string;
  brightGreen: string;
  brightYellow: string;
  brightBlue: string;
  brightMagenta: string;
  brightCyan: string;
  brightWhite: string;
}

export interface TerminalTheme {
  name: string;
  author: string;
  appearance: ThemeAppearance;
  colors: TerminalThemeColors;
}

export const DEFAULT_DARK_THEME_NAME = "Catppuccin Mocha";
export const DEFAULT_LIGHT_THEME_NAME = "Catppuccin Latte";

const THEME_APPEARANCE_PAIRS: ReadonlyArray<readonly [string, string]> = [
  ["Default Dark", "Default Light"],
  ["Solarized Dark", "Solarized Light"],
  ["One Dark", "One Light"],
  ["Catppuccin Mocha", "Catppuccin Latte"],
  ["Nord", "GitHub Light"],
  ["Tokyo Night", "GitHub Light"],
  ["Dracula", "One Light"],
  ["Monokai", "Default Light"],
];

export const BUILTIN_THEMES: TerminalTheme[] = [
  {
    name: "Default Dark",
    author: "zorai",
    appearance: "dark",
    colors: {
      background: "#191919",
      foreground: "#d6d6d6",
      cursor: "#d6d6d6",
      selectionBg: "#264f78",
      black: "#000000",
      red: "#cd3131",
      green: "#0dbc79",
      yellow: "#e5e510",
      blue: "#2472c8",
      magenta: "#bc3fbc",
      cyan: "#11a8cd",
      white: "#d6d6d6",
      brightBlack: "#666666",
      brightRed: "#f14c4c",
      brightGreen: "#23d18b",
      brightYellow: "#f5f543",
      brightBlue: "#3b8eea",
      brightMagenta: "#d670d6",
      brightCyan: "#29b8db",
      brightWhite: "#d6d6d6",
    },
  },
  {
    name: "Dracula",
    author: "Zeno Rocha",
    appearance: "dark",
    colors: {
      background: "#282a36",
      foreground: "#f8f8f2",
      cursor: "#f8f8f2",
      selectionBg: "#44475a",
      black: "#21222c",
      red: "#ff5555",
      green: "#50fa7b",
      yellow: "#f1fa8c",
      blue: "#bd93f9",
      magenta: "#ff79c6",
      cyan: "#8be9fd",
      white: "#f8f8f2",
      brightBlack: "#6272a4",
      brightRed: "#ff6e6e",
      brightGreen: "#69ff94",
      brightYellow: "#ffffa5",
      brightBlue: "#d6acff",
      brightMagenta: "#ff92df",
      brightCyan: "#a4ffff",
      brightWhite: "#ffffff",
    },
  },
  {
    name: "Nord",
    author: "Arctic Ice Studio",
    appearance: "dark",
    colors: {
      background: "#2e3440",
      foreground: "#d8dee9",
      cursor: "#d8dee9",
      selectionBg: "#434c5e",
      black: "#3b4252",
      red: "#bf616a",
      green: "#a3be8c",
      yellow: "#ebcb8b",
      blue: "#81a1c1",
      magenta: "#b48ead",
      cyan: "#88c0d0",
      white: "#e5e9f0",
      brightBlack: "#4c566a",
      brightRed: "#bf616a",
      brightGreen: "#a3be8c",
      brightYellow: "#ebcb8b",
      brightBlue: "#81a1c1",
      brightMagenta: "#b48ead",
      brightCyan: "#8fbcbb",
      brightWhite: "#eceff4",
    },
  },
  {
    name: "Solarized Dark",
    author: "Ethan Schoonover",
    appearance: "dark",
    colors: {
      background: "#002b36",
      foreground: "#839496",
      cursor: "#839496",
      selectionBg: "#073642",
      black: "#073642",
      red: "#dc322f",
      green: "#859900",
      yellow: "#b58900",
      blue: "#268bd2",
      magenta: "#d33682",
      cyan: "#2aa198",
      white: "#eee8d5",
      brightBlack: "#586e75",
      brightRed: "#cb4b16",
      brightGreen: "#586e75",
      brightYellow: "#657b83",
      brightBlue: "#839496",
      brightMagenta: "#6c71c4",
      brightCyan: "#93a1a1",
      brightWhite: "#fdf6e3",
    },
  },
  {
    name: "One Dark",
    author: "Atom",
    appearance: "dark",
    colors: {
      background: "#282c34",
      foreground: "#abb2bf",
      cursor: "#528bff",
      selectionBg: "#3e4451",
      black: "#282c34",
      red: "#e06c75",
      green: "#98c379",
      yellow: "#e5c07b",
      blue: "#61afef",
      magenta: "#c678dd",
      cyan: "#56b6c2",
      white: "#abb2bf",
      brightBlack: "#5c6370",
      brightRed: "#e06c75",
      brightGreen: "#98c379",
      brightYellow: "#e5c07b",
      brightBlue: "#61afef",
      brightMagenta: "#c678dd",
      brightCyan: "#56b6c2",
      brightWhite: "#ffffff",
    },
  },
  {
    name: "Monokai",
    author: "Wimer Hazenberg",
    appearance: "dark",
    colors: {
      background: "#272822",
      foreground: "#f8f8f2",
      cursor: "#f8f8f0",
      selectionBg: "#49483e",
      black: "#272822",
      red: "#f92672",
      green: "#a6e22e",
      yellow: "#f4bf75",
      blue: "#66d9ef",
      magenta: "#ae81ff",
      cyan: "#a1efe4",
      white: "#f8f8f2",
      brightBlack: "#75715e",
      brightRed: "#f92672",
      brightGreen: "#a6e22e",
      brightYellow: "#f4bf75",
      brightBlue: "#66d9ef",
      brightMagenta: "#ae81ff",
      brightCyan: "#a1efe4",
      brightWhite: "#f9f8f5",
    },
  },
  {
    name: "Tokyo Night",
    author: "enkia",
    appearance: "dark",
    colors: {
      background: "#1a1b26",
      foreground: "#c0caf5",
      cursor: "#c0caf5",
      selectionBg: "#33467c",
      black: "#15161e",
      red: "#f7768e",
      green: "#9ece6a",
      yellow: "#e0af68",
      blue: "#7aa2f7",
      magenta: "#bb9af7",
      cyan: "#7dcfff",
      white: "#a9b1d6",
      brightBlack: "#414868",
      brightRed: "#f7768e",
      brightGreen: "#9ece6a",
      brightYellow: "#e0af68",
      brightBlue: "#7aa2f7",
      brightMagenta: "#bb9af7",
      brightCyan: "#7dcfff",
      brightWhite: "#c0caf5",
    },
  },
  {
    name: "Catppuccin Mocha",
    author: "Catppuccin",
    appearance: "dark",
    colors: {
      background: "#1e1e2e",
      foreground: "#cdd6f4",
      cursor: "#f5e0dc",
      selectionBg: "#45475a",
      black: "#45475a",
      red: "#f38ba8",
      green: "#a6e3a1",
      yellow: "#f9e2af",
      blue: "#89b4fa",
      magenta: "#f5c2e7",
      cyan: "#94e2d5",
      white: "#bac2de",
      brightBlack: "#585b70",
      brightRed: "#f38ba8",
      brightGreen: "#a6e3a1",
      brightYellow: "#f9e2af",
      brightBlue: "#89b4fa",
      brightMagenta: "#f5c2e7",
      brightCyan: "#94e2d5",
      brightWhite: "#a6adc8",
    },
  },
  {
    name: "Default Light",
    author: "zorai",
    appearance: "light",
    colors: {
      background: "#f5f7fa",
      foreground: "#1f2937",
      cursor: "#1f2937",
      selectionBg: "#c7d2fe",
      black: "#111827",
      red: "#dc2626",
      green: "#16a34a",
      yellow: "#ca8a04",
      blue: "#2563eb",
      magenta: "#c026d3",
      cyan: "#0891b2",
      white: "#e5e7eb",
      brightBlack: "#6b7280",
      brightRed: "#ef4444",
      brightGreen: "#22c55e",
      brightYellow: "#eab308",
      brightBlue: "#3b82f6",
      brightMagenta: "#d946ef",
      brightCyan: "#06b6d4",
      brightWhite: "#111827",
    },
  },
  {
    name: "Solarized Light",
    author: "Ethan Schoonover",
    appearance: "light",
    colors: {
      background: "#fdf6e3",
      foreground: "#657b83",
      cursor: "#657b83",
      selectionBg: "#eee8d5",
      black: "#073642",
      red: "#dc322f",
      green: "#859900",
      yellow: "#b58900",
      blue: "#268bd2",
      magenta: "#d33682",
      cyan: "#2aa198",
      white: "#eee8d5",
      brightBlack: "#002b36",
      brightRed: "#cb4b16",
      brightGreen: "#586e75",
      brightYellow: "#657b83",
      brightBlue: "#839496",
      brightMagenta: "#6c71c4",
      brightCyan: "#93a1a1",
      brightWhite: "#fdf6e3",
    },
  },
  {
    name: "One Light",
    author: "Atom",
    appearance: "light",
    colors: {
      background: "#fafafa",
      foreground: "#383a42",
      cursor: "#526fff",
      selectionBg: "#e5e5e6",
      black: "#383a42",
      red: "#e45649",
      green: "#50a14f",
      yellow: "#c18401",
      blue: "#4078f2",
      magenta: "#a626a4",
      cyan: "#0184bc",
      white: "#a0a1a7",
      brightBlack: "#696c77",
      brightRed: "#e45649",
      brightGreen: "#50a14f",
      brightYellow: "#c18401",
      brightBlue: "#4078f2",
      brightMagenta: "#a626a4",
      brightCyan: "#0184bc",
      brightWhite: "#383a42",
    },
  },
  {
    name: "Catppuccin Latte",
    author: "Catppuccin",
    appearance: "light",
    colors: {
      background: "#eff1f5",
      foreground: "#4c4f69",
      cursor: "#dc8a78",
      selectionBg: "#ccd0da",
      black: "#5c5f77",
      red: "#d20f39",
      green: "#40a02b",
      yellow: "#df8e1d",
      blue: "#1e66f5",
      magenta: "#ea76cb",
      cyan: "#179299",
      white: "#acb0be",
      brightBlack: "#6c6f85",
      brightRed: "#d20f39",
      brightGreen: "#40a02b",
      brightYellow: "#df8e1d",
      brightBlue: "#1e66f5",
      brightMagenta: "#ea76cb",
      brightCyan: "#179299",
      brightWhite: "#4c4f69",
    },
  },
  {
    name: "GitHub Light",
    author: "GitHub",
    appearance: "light",
    colors: {
      background: "#ffffff",
      foreground: "#1f2328",
      cursor: "#0969da",
      selectionBg: "#ddf4ff",
      black: "#24292f",
      red: "#cf222e",
      green: "#1a7f37",
      yellow: "#9a6700",
      blue: "#0969da",
      magenta: "#8250df",
      cyan: "#1b7c83",
      white: "#6e7781",
      brightBlack: "#656d76",
      brightRed: "#a40e26",
      brightGreen: "#116329",
      brightYellow: "#7d4e00",
      brightBlue: "#0550ae",
      brightMagenta: "#6639ba",
      brightCyan: "#1b7c83",
      brightWhite: "#1f2328",
    },
  },
];

export function defaultThemeNameForAppearance(appearance: ThemeAppearance): string {
  return appearance === "light" ? DEFAULT_LIGHT_THEME_NAME : DEFAULT_DARK_THEME_NAME;
}

export function themesForAppearance(appearance: ThemeAppearance): TerminalTheme[] {
  return BUILTIN_THEMES.filter((theme) => theme.appearance === appearance);
}

export function getThemeAppearance(themeName: string): ThemeAppearance {
  return getThemeByName(themeName).appearance;
}

export function pairedThemeName(themeName: string, appearance: ThemeAppearance): string | null {
  const normalized = themeName.trim().toLowerCase();
  for (const [darkName, lightName] of THEME_APPEARANCE_PAIRS) {
    if (darkName.toLowerCase() === normalized || lightName.toLowerCase() === normalized) {
      return appearance === "light" ? lightName : darkName;
    }
  }
  return null;
}

export function coerceThemeNameForAppearance(themeName: string, appearance: ThemeAppearance): string {
  const current = BUILTIN_THEMES.find((theme) => theme.name.toLowerCase() === themeName.trim().toLowerCase());
  if (current?.appearance === appearance) {
    return current.name;
  }
  const paired = pairedThemeName(themeName, appearance);
  if (paired && BUILTIN_THEMES.some((theme) => theme.name === paired)) {
    return paired;
  }
  return defaultThemeNameForAppearance(appearance);
}

/** Get a theme by name (case-insensitive). Falls back to the dark default. */
export function getThemeByName(name: string): TerminalTheme {
  return (
    BUILTIN_THEMES.find(
      (t) => t.name.toLowerCase() === name.toLowerCase()
    ) ?? BUILTIN_THEMES.find((theme) => theme.name === DEFAULT_DARK_THEME_NAME)!
  );
}

/** Build effective theme colors, merging custom overrides on top of the base theme. */
export function getEffectiveTheme(
  themeName: string,
  useCustomColors: boolean,
  customBg?: string,
  customFg?: string,
  customCursor?: string,
  customSelection?: string
): TerminalThemeColors {
  const base = getThemeByName(themeName).colors;
  if (!useCustomColors) return base;
  return {
    ...base,
    ...(customBg ? { background: customBg } : {}),
    ...(customFg ? { foreground: customFg } : {}),
    ...(customCursor ? { cursor: customCursor } : {}),
    ...(customSelection ? { selectionBg: customSelection } : {}),
  };
}
