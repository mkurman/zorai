import { describe, expect, it } from "vitest";
import { BUILTIN_THEMES, coerceThemeNameForAppearance, getThemeByName, setUserThemes, themesForAppearance } from "./themes";
import { adjustBorders, isVideoBackground, resolveFonts, resolveUserThemes, themeBackdrop, translucentColor, translucentShell, userThemeToFile } from "./userThemes";

const record = (id: string, theme: ZoraiUserThemeFile | null, extra: Partial<ZoraiUserThemeRecord> = {}): ZoraiUserThemeRecord =>
  ({ id, dir: `/themes/${id}`, theme, css: null, backgroundPath: null, error: null, ...extra });

describe("user themes", () => {
  it("lets a hand-written theme override a few values and inherit everything else from its base", () => {
    const { themes, errors } = resolveUserThemes([record("tiny", { name: "Tiny", appearance: "dark", base: "Nord", colors: { cursor: "#ff0000", bogus: "#000" }, appShell: { "--accent": "#00ff00", notAVariable: "x" } })]);
    const nord = BUILTIN_THEMES.find((theme) => theme.name === "Nord")!;
    expect(errors).toEqual([]);
    expect(themes[0].colors).toEqual({ ...nord.colors, cursor: "#ff0000" });
    expect(themes[0].appShell?.["--accent"]).toBe("#00ff00");
    expect(themes[0].appShell).not.toHaveProperty("notAVariable");
    expect(themes[0].user).toMatchObject({ id: "tiny", base: "Nord", surfaceOpacity: 1, seeThrough: false });
  });

  it("reports unreadable themes and name clashes instead of silently dropping or shadowing a built-in", () => {
    const { themes, errors } = resolveUserThemes([
      record("broken", null, { error: "Could not read theme.json" }),
      record("clash", { name: "nord", appearance: "dark" }),
      record("one", { name: "Mine", appearance: "light" }),
      record("two", { name: "MINE", appearance: "light" }),
    ]);
    expect(themes.map((theme) => theme.name)).toEqual(["Mine"]);
    expect(errors.map((error) => error.id)).toEqual(["broken", "clash", "two"]);
  });

  it("keeps out-of-range effect values within usable limits so a typo cannot make the app invisible", () => {
    const { themes } = resolveUserThemes([record("x", { name: "X", appearance: "dark", surfaceOpacity: 0, background: { blur: 500, dim: 3 } })]);
    expect(themes[0].user).toMatchObject({ surfaceOpacity: 0.2, blur: 60, dim: 0.9 });
  });

  it("makes discovered themes selectable alongside built-ins and removes them again", () => {
    const { themes } = resolveUserThemes([record("x", { name: "Discovered", appearance: "light" })]);
    setUserThemes(themes);
    expect(themesForAppearance("light").map((theme) => theme.name)).toContain("Discovered");
    expect(getThemeByName("discovered").user?.id).toBe("x");
    setUserThemes([]);
    expect(getThemeByName("Discovered").name).not.toBe("Discovered");
  });

  it("saves only what differs from the base, so the file stays small and keeps following base updates", () => {
    const { themes } = resolveUserThemes([record("x", { name: "X", appearance: "dark", base: "Nord", colors: { cursor: "#123456" }, background: { image: "background.png", blur: 6, dim: 0.2 }, surfaceOpacity: 0.7 })]);
    expect(userThemeToFile(themes[0], "background.png")).toEqual({
      name: "X", appearance: "dark", base: "Nord", colors: { cursor: "#123456" }, appShell: {},
      background: { image: "background.png", blur: 6, dim: 0.2 }, surfaceOpacity: 0.7, seeThrough: false,
    });
  });

  it("only shows a backdrop when something is actually behind the window", () => {
    const [image, seeThrough, plain] = resolveUserThemes([
      record("a", { name: "A", appearance: "dark", surfaceOpacity: 0.5 }, { backgroundPath: "/themes/a/background.png" }),
      record("b", { name: "B", appearance: "dark", seeThrough: true, surfaceOpacity: 0.5 }),
      record("c", { name: "C", appearance: "dark", surfaceOpacity: 0.5 }),
    ]).themes;
    expect(themeBackdrop(image, false)).toBe("image");
    expect(themeBackdrop(seeThrough, true)).toBe("see-through");
    expect(themeBackdrop(seeThrough, false)).toBe("none");
    expect(themeBackdrop(plain, true)).toBe("none");
    expect(themeBackdrop(BUILTIN_THEMES[0], true)).toBe("none");
  });

  it("thins panels and keeps menus mostly solid, but never the window colour, which also paints text on accent buttons", () => {
    const shell = translucentShell({ "--bg-void": "#000000", "--bg-primary": "#102030", "--bg-elevated": "rgba(10, 20, 30, 0.5)", "--text-primary": "#ffffff" }, "image", 0.5);
    expect(shell).toEqual({ "--bg-void": "#000000", "--bg-primary": "rgba(16, 32, 48, 0.5)", "--bg-elevated": "rgba(10, 20, 30, 0.4)", "--text-primary": "#ffffff" });
    expect(translucentShell({ "--bg-void": "#000000" }, "none", 0.5)).toEqual({ "--bg-void": "#000000" });
    expect(translucentColor("#abc", 1)).toBe("rgba(170, 187, 204, 1)");
    expect(translucentColor("hsl(10 20% 30%)", 0.25)).toBe("color-mix(in srgb, hsl(10 20% 30%) 25%, transparent)");
  });

  it("follows a custom theme's companion when Night Mode flips, in both directions, instead of falling back to a built-in", () => {
    const { themes } = resolveUserThemes([
      record("night", { name: "My Night", appearance: "dark", pair: "My Day" }),
      record("day", { name: "My Day", appearance: "light" }),
      record("solo", { name: "Solo", appearance: "dark" }),
    ]);
    setUserThemes(themes);
    expect(coerceThemeNameForAppearance("My Night", "light")).toBe("My Day");
    expect(coerceThemeNameForAppearance("My Day", "dark")).toBe("My Night");
    expect(coerceThemeNameForAppearance("My Night", "dark")).toBe("My Night");
    expect(coerceThemeNameForAppearance("Solo", "light")).toBe("Zorai Paper");
    setUserThemes([]);
  });

  it("gives a single typed font name a fallback stack so a missing font never drops to the browser default", () => {
    const fonts = resolveFonts(undefined, { ui: "Inter", mono: "'Fira Code'" })!;
    expect(fonts.ui).toMatch(/^"Inter", /);
    expect(fonts.display).toBe(fonts.ui);
    expect(fonts.mono).toBe('"Fira Code", ui-monospace, SFMono-Regular, Consolas, monospace');
    expect(resolveFonts(undefined, { ui: "A, B" })!.ui).toBe("A, B");
    expect(resolveFonts(undefined, {})).toBeUndefined();
  });

  it("scales borders down to invisible or up toward the text colour, leaving other colours alone", () => {
    const shell = { "--border": "#202020", "--zorai-accent": "#ff0000", "--text-primary": "#ffffff" };
    expect(adjustBorders(shell, 1)).toBe(shell);
    expect(adjustBorders(shell, 0)["--border"]).toBe("rgba(32, 32, 32, 0)");
    expect(adjustBorders(shell, 2)["--border"]).toBe("color-mix(in srgb, #202020, #ffffff 60%)");
    expect(adjustBorders(shell, 2)["--zorai-accent"]).toBe("#ff0000");
  });

  it("round-trips shape, companion and font choices through theme.json and omits defaults", () => {
    const [theme] = resolveUserThemes([record("x", { name: "X", appearance: "dark", base: "Nord", pair: "GitHub Light", corners: "square", borderStrength: 0.5, fonts: { mono: "Iosevka" } })]).themes;
    expect(userThemeToFile(theme)).toMatchObject({ pair: "GitHub Light", corners: "square", borderStrength: 0.5, fonts: { mono: "Iosevka" } });
    const [plain] = resolveUserThemes([record("y", { name: "Y", appearance: "dark", base: "Nord", corners: "round" as never, borderStrength: 1 })]).themes;
    const file = userThemeToFile(plain);
    expect(file).not.toHaveProperty("corners");
    expect(file).not.toHaveProperty("borderStrength");
    expect(file).not.toHaveProperty("fonts");
    expect(file).not.toHaveProperty("pair");
  });

  it("treats mp4 and webm backgrounds as video so they play instead of being used as a broken CSS image", () => {
    expect(isVideoBackground("/t/x/background.WEBM")).toBe(true);
    expect(isVideoBackground("/t/x/background.mp4")).toBe(true);
    expect(isVideoBackground("/t/x/background.gif")).toBe(false);
    expect(isVideoBackground(null)).toBe(false);
  });
});
