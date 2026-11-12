import { describe, expect, it } from "vitest";
import { BUILTIN_THEMES } from "@/lib/themes";
import { resolveUserThemes } from "@/lib/userThemes";
import { colorInputValue, draftFromTheme, draftNameProblem, draftSavePayload, rebaseDraft, themeFromDraft, withAccent } from "./themeDraft";

const nord = BUILTIN_THEMES.find((theme) => theme.name === "Nord")!;
const [custom] = resolveUserThemes([{ id: "glass", dir: "/t/glass", css: "a{}", backgroundPath: "/t/glass/background.png", error: null, theme: { name: "Glass", appearance: "dark", base: "Nord", background: { image: "background.png", blur: 9, dim: 0.4 }, surfaceOpacity: 0.7 } }]).themes;

describe("theme editor drafts", () => {
  it("starting from a built-in makes a new copy with a free name instead of editing the built-in", () => {
    const draft = draftFromTheme(nord, ["Nord", "Nord Custom"]);
    expect(draft).toMatchObject({ id: null, name: "Nord Custom 2", base: "Nord", surfaceOpacity: 1, backgroundPath: null });
  });

  it("editing a custom theme keeps its folder, image and css", () => {
    const draft = draftFromTheme(custom, ["Glass"]);
    expect(draft).toMatchObject({ id: "glass", name: "Glass", css: "a{}", backgroundPath: "/t/glass/background.png", blur: 9, dim: 0.4, surfaceOpacity: 0.7 });
    expect(draftNameProblem(draft, [nord, custom])).toBeNull();
    expect(draftNameProblem({ ...draft, name: "nord" }, [nord, custom])).toMatch(/already exists/);
    expect(draftNameProblem({ ...draft, name: "  " }, [nord])).toMatch(/name/);
    const fresh = draftFromTheme(nord, ["Nord"]);
    expect(draftNameProblem(fresh, [nord, custom, themeFromDraft(fresh)])).toMatch(/already exists/);
    expect(draftNameProblem(fresh, [nord, custom])).toBeNull();
  });

  it("keeps the existing image when untouched, sends a newly chosen one, and asks to delete a removed one", () => {
    const draft = draftFromTheme(custom, []);
    expect(draftSavePayload(draft)).toMatchObject({ id: "glass", backgroundSource: undefined, theme: { background: { image: "background.png", blur: 9, dim: 0.4 } } });
    expect(draftSavePayload({ ...draft, backgroundPath: "/home/me/pic.jpg", backgroundSource: "/home/me/pic.jpg" }).backgroundSource).toBe("/home/me/pic.jpg");
    const removed = draftSavePayload({ ...draft, backgroundPath: null, backgroundSource: null });
    expect(removed.backgroundSource).toBeNull();
    expect(removed.theme.background).not.toHaveProperty("image");
  });

  it("switching the starting theme replaces colours and mode but keeps the background settings", () => {
    const draft = rebaseDraft(draftFromTheme(custom, []), "GitHub Light");
    const light = BUILTIN_THEMES.find((theme) => theme.name === "GitHub Light")!;
    expect(draft).toMatchObject({ base: "GitHub Light", appearance: "light", colors: light.colors, blur: 9, surfaceOpacity: 0.7 });
  });

  it("previews the draft as a real theme so the whole app reflects unsaved edits", () => {
    const theme = themeFromDraft({ ...draftFromTheme(nord, []), name: "", appShell: { ...nord.appShell, "--accent": "#ff0000" } });
    expect(theme.name).toBe("Untitled theme");
    expect(theme.appShell?.["--accent"]).toBe("#ff0000");
    expect(theme.user?.id).toBe("__draft__");
  });

  it("feeds colour pickers a hex value even when the theme uses rgba or named colours", () => {
    expect(colorInputValue("#ABC")).toBe("#aabbcc");
    expect(colorInputValue("rgba(255, 0, 16, 0.5)")).toBe("#ff0010");
    expect(colorInputValue("tomato")).toBe("#000000");
  });

  it("changing the accent also recolours its hover, soft and glow shades so no stale base colour is left behind", () => {
    const draft = withAccent(draftFromTheme(nord, []), "#ff0000");
    expect(draft.appShell["--accent"]).toBe("#ff0000");
    expect(draft.appShell["--accent-soft"]).toBe("rgba(255, 0, 0, 0.12)");
    expect(draft.appShell["--accent-glow"]).toBe("rgba(255, 0, 0, 0.2)");
    const typed = withAccent(draftFromTheme(nord, []), "tomato");
    expect(typed.appShell["--accent-soft"]).toBe(nord.appShell?.["--accent-soft"] ?? draftFromTheme(nord, []).appShell["--accent-soft"]);
  });
});
