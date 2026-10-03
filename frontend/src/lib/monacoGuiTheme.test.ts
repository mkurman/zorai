import { describe, expect, it } from "vitest";
import {
  applyMonacoGuiTheme,
  buildMonacoGuiTheme,
  DEFAULT_MONACO_GUI_COLORS,
  parseCssColor,
  resolveMonacoFontFamily,
  ZORAI_MONACO_THEME,
} from "./monacoGuiTheme";

describe("monaco gui theme", () => {
  it("parses the hex and rgb colors the shell theme writes", () => {
    expect(parseCssColor("#5ee7df")).toEqual({ r: 94, g: 231, b: 223, a: 1 });
    expect(parseCssColor("rgba(255, 255, 255, 0.10)")?.a).toBeCloseTo(0.1);
    expect(parseCssColor("rgb(20 28 40 / 50%)")?.a).toBeCloseTo(0.5);
  });

  it("paints the editor chrome and syntax from gui tokens instead of vs-dark defaults", () => {
    const theme = buildMonacoGuiTheme(DEFAULT_MONACO_GUI_COLORS);

    expect(theme.base).toBe("vs-dark");
    expect(theme.inherit).toBe(false);
    expect(theme.colors["editor.background"]).toBe("#141c28");
    expect(theme.colors["editor.foreground"]).toBe("#f2f2f2");
    expect(theme.colors["editorCursor.foreground"]).toBe("#5ee7df");
    expect(theme.colors["editorWidget.background"]).toBe("#212e3e");
    expect(theme.colors["editorLineNumber.foreground"]).toBe("#9aa8ba");
    expect(foreground(theme, "string")).toBe("4ade80");
    expect(foreground(theme, "string.key.json")).toBe("5ee7df");
    expect(foreground(theme, "keyword")).toBe("5ee7df");
    expect(foreground(theme, "number")).toBe("fbbf24");
    expect(foreground(theme, "comment")).toBe("9aa8ba");
    expect(foreground(theme, "diff-add")).toBe("4ade80");
    expect(foreground(theme, "diff-remove")).toBe("f87171");
    expect(foreground(theme, "string")).not.toBe("ce9178");
  });

  it("uses the light monaco base when the gui surface is light", () => {
    const theme = buildMonacoGuiTheme({
      ...DEFAULT_MONACO_GUI_COLORS,
      surface: "#f7f4ef",
      text: "#1c2430",
      muted: "#5c6b80",
    });

    expect(theme.base).toBe("vs");
    expect(theme.colors["editor.background"]).toBe("#f7f4ef");
    expect(theme.colors["editor.foreground"]).toBe("#1c2430");
  });

  it("composites translucent muted text onto the editor surface", () => {
    const theme = buildMonacoGuiTheme({
      ...DEFAULT_MONACO_GUI_COLORS,
      surface: "#141c28",
      muted: "rgba(242, 242, 242, 0.5)",
    });

    expect(foreground(theme, "comment")).toBe("83878d");
  });

  it("resolves the gui mono stack when the editor font is still a css variable", () => {
    expect(resolveMonacoFontFamily("var(--font-mono)", '"IBM Plex Mono", monospace')).toBe('"IBM Plex Mono", monospace');
    expect(resolveMonacoFontFamily("JetBrains Mono", '"IBM Plex Mono", monospace')).toBe("JetBrains Mono");
  });

  it("registers one gui theme and refreshes it when the shell colors change", () => {
    const defined: string[] = [];
    const activated: string[] = [];
    const monaco = {
      editor: {
        defineTheme: (name: string) => defined.push(name),
        setTheme: (name: string) => activated.push(name),
      },
    };
    const first = { ...DEFAULT_MONACO_GUI_COLORS, accent: "#11aaee" };
    const second = { ...DEFAULT_MONACO_GUI_COLORS, accent: "#22bbff" };

    applyMonacoGuiTheme(monaco, first);
    applyMonacoGuiTheme(monaco, first);
    applyMonacoGuiTheme(monaco, second);

    expect(defined).toEqual([ZORAI_MONACO_THEME, ZORAI_MONACO_THEME]);
    expect(activated).toEqual([ZORAI_MONACO_THEME, ZORAI_MONACO_THEME, ZORAI_MONACO_THEME]);
  });
});

function foreground(theme: ReturnType<typeof buildMonacoGuiTheme>, token: string): string | undefined {
  return theme.rules.find((rule) => rule.token === token)?.foreground;
}
