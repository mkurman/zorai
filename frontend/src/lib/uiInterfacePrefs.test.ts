import { describe, expect, it, vi } from "vitest";
import { chatMaxWidth, normalizeChatWidth,
  UI_FONT_SCALE_FACTORS,
  applyUiInterfacePrefs,
  buildUiFontFamilyStack,
  normalizeNightMode,
  normalizeUiFontScale,
  resolveAppearance,
} from "./uiInterfacePrefs";
import { coerceThemeNameForAppearance, defaultThemeNameForAppearance, getThemeByName, themesForAppearance } from "./themes";
import { DEFAULT_SETTINGS } from "./types";
import { getAppShellTheme } from "./themesAppShell";

describe("uiInterfacePrefs", () => {
  it("normalizes scale and night mode with night-on defaults", () => {
    expect(normalizeUiFontScale(1.1)).toBe(1.1);
    expect(normalizeUiFontScale("nope")).toBe(1);
    expect(normalizeNightMode("auto")).toBe("auto");
    expect(normalizeNightMode("nope")).toBe("on");
  });

  it("resolves appearance from night mode preference", () => {
    expect(resolveAppearance("on", false)).toBe("dark");
    expect(resolveAppearance("off", true)).toBe("light");
    expect(resolveAppearance("auto", true)).toBe("dark");
    expect(resolveAppearance("auto", false)).toBe("light");
  });

  it("keeps known UI font scale factors ordered", () => {
    expect(UI_FONT_SCALE_FACTORS.small).toBeLessThan(UI_FONT_SCALE_FACTORS.medium);
    expect(UI_FONT_SCALE_FACTORS.medium).toBeLessThan(UI_FONT_SCALE_FACTORS.large);
    expect(UI_FONT_SCALE_FACTORS.large).toBeLessThan(UI_FONT_SCALE_FACTORS["very-large"]);
  });

  it("keeps saved named sizes working after the switch to a numeric scale", () => {
    expect(normalizeUiFontScale("large")).toBe(UI_FONT_SCALE_FACTORS.large);
    expect(normalizeUiFontScale("small")).toBe(0.875);
  });

  it("clamps and snaps the scale so the slider can never produce an unreadable or off-tick size", () => {
    expect(normalizeUiFontScale(3)).toBe(1.5);
    expect(normalizeUiFontScale(0.1)).toBe(0.75);
    expect(normalizeUiFontScale(1.0700000001)).toBe(1.075);
    expect(normalizeUiFontScale(Number.NaN)).toBe(1);
  });

  it("quotes selected UI fonts in the CSS stack", () => {
    expect(buildUiFontFamilyStack("Inter")).toContain('"Inter"');
    expect(buildUiFontFamilyStack("")).toContain('"Cascadia Code"');
  });
});

describe("app shell appearance", () => {
  it("builds distinct light and dark shell tokens from the same palette", () => {
    const dark = getAppShellTheme("Catppuccin Mocha", false, undefined, undefined, undefined, undefined, "dark");
    const light = getAppShellTheme("Catppuccin Mocha", false, undefined, undefined, undefined, undefined, "light");
    expect(dark["--bg-primary"]).not.toBe(light["--bg-primary"]);
    expect(dark["--text-primary"]).not.toBe(light["--text-primary"]);
    expect(light["--bg-primary"]).toMatch(/^#/);
  });

  it("keeps light-mode chrome readable with solid muted text and ink borders", () => {
    const light = getAppShellTheme("Catppuccin Latte", false, undefined, undefined, undefined, undefined, "light");
    expect(light["--bg-primary"]).toMatch(/^#/);
    expect(light["--bg-deep"]).not.toBe(getAppShellTheme("Catppuccin Mocha", false, undefined, undefined, undefined, undefined, "dark")["--bg-deep"]);
    expect(light["--text-on-accent"]).toBe("#ffffff");
    expect(light["--border-color"]).toMatch(/^rgba\(/);
    expect(light["--border-color-strong"]).toMatch(/^rgba\(/);
    expect(light["--accent-glow"]).toMatch(/^rgba\(/);
  });

  it("lets light color styles diverge instead of collapsing to one paper sheet", () => {
    const latte = getAppShellTheme("Catppuccin Latte", false, undefined, undefined, undefined, undefined, "light");
    const solarized = getAppShellTheme("Solarized Light", false, undefined, undefined, undefined, undefined, "light");
    const github = getAppShellTheme("GitHub Light", false, undefined, undefined, undefined, undefined, "light");
    expect(new Set([latte["--bg-primary"], solarized["--bg-primary"], github["--bg-primary"]]).size).toBeGreaterThan(1);
  });
});

describe("ClinicalGym theme", () => {
  it("uses the site's exact paper, ink and accent rather than colors derived from the terminal palette", () => {
    const shell = getAppShellTheme("ClinicalGym", false, undefined, undefined, undefined, undefined, "light");
    expect(shell["--bg-primary"]).toBe("#f7f6f1");
    expect(shell["--text-primary"]).toBe("#222820");
    expect(shell["--accent"]).toBe("#b94220");
    expect(shell["--border"]).toBe("#dcded4");
  });

  it("lets custom terminal colors win over the fixed site palette", () => {
    const shell = getAppShellTheme("ClinicalGym", true, "#ffffff", "#000000", undefined, undefined, "light");
    expect(shell["--bg-primary"]).not.toBe("#f7f6f1");
  });

  it("only overrides tokens every theme of its appearance sets, so switching away leaves nothing stale", () => {
    const lightBase = getAppShellTheme("GitHub Light", false, undefined, undefined, undefined, undefined, "light");
    const darkBase = getAppShellTheme("Nord", false, undefined, undefined, undefined, undefined, "dark");
    const lightOverrides = Object.keys(getThemeByName("ClinicalGym").appShell ?? {});
    const darkOverrides = Object.keys(getThemeByName("ClinicalGym Dark").appShell ?? {});
    expect(lightOverrides.filter((name) => !(name in lightBase))).toEqual([]);
    expect(darkOverrides.filter((name) => !(name in darkBase))).toEqual([]);
  });

  it("gives the dark edition the same tokens as the light one so neither leaves gaps when night mode flips", () => {
    const light = Object.keys(getThemeByName("ClinicalGym").appShell ?? {}).sort();
    const dark = Object.keys(getThemeByName("ClinicalGym Dark").appShell ?? {}).sort();
    expect(dark).toEqual(light);
    expect(getAppShellTheme("ClinicalGym Dark", false, undefined, undefined, undefined, undefined, "dark")["--bg-primary"]).toBe("#1b211d");
  });

  it("flips between the light and dark editions with night mode", () => {
    expect(coerceThemeNameForAppearance("ClinicalGym", "dark")).toBe("ClinicalGym Dark");
    expect(coerceThemeNameForAppearance("ClinicalGym Dark", "light")).toBe("ClinicalGym");
  });

  it("applies the site fonts when the UI font is left on theme default and restores defaults when another theme is chosen", () => {
    const properties = new Map<string, string>();
    const root = {
      dataset: {} as Record<string, string>,
      style: {
        colorScheme: "",
        setProperty: (name: string, value: string) => properties.set(name, value),
        removeProperty: (name: string) => properties.delete(name),
        getPropertyValue: (name: string) => properties.get(name) ?? "",
      },
    };
    vi.stubGlobal("document", { documentElement: root });
    applyUiInterfacePrefs({ uiFontFamily: "", uiFontScale: 1, appearance: "light", themeName: "ClinicalGym", themeFonts: getThemeByName("ClinicalGym").fonts });
    expect(root.style.getPropertyValue("--font-ui")).toContain("Inter Variable");
    expect(root.style.getPropertyValue("--font-mono")).toContain("ui-monospace");
    expect(root.dataset.theme).toBe("clinicalgym");
    applyUiInterfacePrefs({ uiFontFamily: "Cascadia Code", uiFontScale: 1, appearance: "light", themeName: "GitHub Light", themeFonts: getThemeByName("GitHub Light").fonts });
    expect(root.style.getPropertyValue("--font-ui")).toContain('"Cascadia Code"');
    expect(root.style.getPropertyValue("--font-mono")).toBe("");
    expect(root.dataset.theme).toBe("github-light");
    vi.unstubAllGlobals();
  });
});

describe("explicit UI font", () => {
  it("wins over theme fonts so picking a font under ClinicalGym actually changes the interface", () => {
    const properties = new Map<string, string>();
    const root = {
      dataset: {} as Record<string, string>,
      style: {
        colorScheme: "",
        setProperty: (name: string, value: string) => properties.set(name, value),
        removeProperty: (name: string) => properties.delete(name),
        getPropertyValue: (name: string) => properties.get(name) ?? "",
      },
    };
    vi.stubGlobal("document", { documentElement: root });
    applyUiInterfacePrefs({ uiFontFamily: "Ubuntu Sans", uiFontScale: 1, appearance: "light", themeName: "ClinicalGym", themeFonts: getThemeByName("ClinicalGym").fonts });
    expect(root.style.getPropertyValue("--font-ui")).toContain('"Ubuntu Sans"');
    expect(root.style.getPropertyValue("--font-ui")).not.toContain("Inter Variable");
    expect(root.style.getPropertyValue("--font-display")).toBe("var(--font-ui)");
    vi.unstubAllGlobals();
  });
});

describe("theme appearance catalog", () => {
  it("exposes separate light and dark color styles", () => {
    expect(themesForAppearance("dark").every((theme) => theme.appearance === "dark")).toBe(true);
    expect(themesForAppearance("light").every((theme) => theme.appearance === "light")).toBe(true);
    expect(themesForAppearance("light").map((theme) => theme.name)).toEqual(expect.arrayContaining([
      "Catppuccin Latte",
      "Solarized Light",
      "One Light",
      "GitHub Light",
      "Default Light",
      "ClinicalGym",
    ]));
    expect(themesForAppearance("dark").map((theme) => theme.name)).toEqual(expect.arrayContaining([
      "Catppuccin Mocha",
      "Dracula",
      "One Dark",
    ]));
  });

  it("pairs dark and light styles when night mode flips", () => {
    expect(coerceThemeNameForAppearance("Catppuccin Mocha", "light")).toBe("Catppuccin Latte");
    expect(coerceThemeNameForAppearance("Catppuccin Latte", "dark")).toBe("Catppuccin Mocha");
    expect(coerceThemeNameForAppearance("Solarized Dark", "light")).toBe("Solarized Light");
    expect(coerceThemeNameForAppearance("One Dark", "light")).toBe("One Light");
  });
});

describe("chat width", () => {
  it("caps the conversation at the chosen width and keeps full width for old or unknown settings", () => {
    expect(chatMaxWidth("narrow")).toBe("620px");
    expect(chatMaxWidth("medium-narrow")).toBe("800px");
    expect(chatMaxWidth("medium")).toBe("960px");
    expect(chatMaxWidth("medium-wide")).toBe("1200px");
    expect(chatMaxWidth("wide")).toBe("1440px");
    expect(chatMaxWidth("very-wide")).toBe("1680px");
    expect(chatMaxWidth("extra-wide")).toBe("1920px");
    expect(chatMaxWidth("ultra-wide")).toBe("2560px");
    expect(chatMaxWidth("full")).toBe("100%");
    expect(normalizeChatWidth(undefined)).toBe("full");
    expect(chatMaxWidth("huge")).toBe("100%");
  });
});

describe("Zorai Paper theme", () => {
  it("uses the brand paper, ink and gold from zorai.app and the intro artwork", () => {
    const light = getAppShellTheme("Zorai Paper", false, undefined, undefined, undefined, undefined, "light");
    const dark = getAppShellTheme("Zorai Paper Dark", false, undefined, undefined, undefined, undefined, "dark");
    expect(light["--bg-canvas"]).toBe("#fcf6e4");
    expect(light["--accent"]).toBe("#89632e");
    expect(dark["--bg-canvas"]).toBe("#2b2822");
    expect(dark["--accent"]).toBe("#d9a548");
  });

  it("gives both editions the same tokens as an existing theme so switching never leaves stale or missing colors", () => {
    const reference = Object.keys(getThemeByName("ClinicalGym").appShell ?? {}).sort();
    expect(Object.keys(getThemeByName("Zorai Paper").appShell ?? {}).sort()).toEqual(reference);
    expect(Object.keys(getThemeByName("Zorai Paper Dark").appShell ?? {}).sort()).toEqual(reference);
  });

  it("is what a new user starts with, in both night modes", () => {
    expect(DEFAULT_SETTINGS.themeName).toBe("Zorai Paper Dark");
    expect(defaultThemeNameForAppearance("dark")).toBe("Zorai Paper Dark");
    expect(defaultThemeNameForAppearance("light")).toBe("Zorai Paper");
  });

  it("flips between the light and dark editions with night mode", () => {
    expect(coerceThemeNameForAppearance("Zorai Paper", "dark")).toBe("Zorai Paper Dark");
    expect(coerceThemeNameForAppearance("Zorai Paper Dark", "light")).toBe("Zorai Paper");
  });
});
