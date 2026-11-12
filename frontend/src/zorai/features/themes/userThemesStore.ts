import { create } from "zustand";
import { getBridge } from "@/lib/bridge";
import { setUserThemes, type TerminalTheme } from "@/lib/themes";
import { isVideoBackground, resolveUserThemes, translucentColor, type ThemeBackdrop } from "@/lib/userThemes";
import { mediaUrlForPath } from "@/zorai/features/threads/ThreadFilePreviewOverlay";

type UserThemesState = {
  loaded: boolean;
  dir: string | null;
  records: ZoraiUserThemeRecord[];
  themes: TerminalTheme[];
  errors: Array<{ id: string; message: string }>;
  preview: TerminalTheme | null;
  seeThroughAtLaunch: boolean;
  seeThrough: boolean;
  customCss: string | null;
  customCssPath: string | null;
  backdropVideo: { src: string; blur: number } | null;
};

export const useUserThemesStore = create<UserThemesState>(() => ({
  loaded: false,
  dir: null,
  records: [],
  themes: [],
  errors: [],
  preview: null,
  seeThroughAtLaunch: false,
  seeThrough: false,
  customCss: null,
  customCssPath: null,
  backdropVideo: null,
}));

function setBackdropVideo(next: { src: string; blur: number } | null) {
  const current = useUserThemesStore.getState().backdropVideo;
  if (current?.src === next?.src && current?.blur === next?.blur) return;
  useUserThemesStore.setState({ backdropVideo: next });
}

function register(themes: TerminalTheme[], preview: TerminalTheme | null) {
  setUserThemes(preview
    ? [...themes.filter((theme) => theme.user?.id !== preview.user?.id && theme.name !== preview.name), preview]
    : themes);
}

export async function refreshUserThemes(): Promise<void> {
  const bridge = getBridge();
  if (!bridge?.themesList) {
    useUserThemesStore.setState({ loaded: true });
    return;
  }
  try {
    const { dir, themes: records, customCss } = await bridge.themesList();
    const { themes, errors } = resolveUserThemes(records);
    register(themes, useUserThemesStore.getState().preview);
    useUserThemesStore.setState({ loaded: true, dir, records, themes, errors, customCss: customCss?.css ?? null, customCssPath: customCss?.path ?? null });
  } catch (error) {
    useUserThemesStore.setState({ loaded: true, errors: [{ id: "", message: `Could not read the themes folder: ${error instanceof Error ? error.message : String(error)}` }] });
  }
}

export function startUserThemeDiscovery(): () => void {
  const bridge = getBridge();
  void refreshUserThemes();
  void bridge?.themesWindowAppearance?.().then((appearance) => useUserThemesStore.setState(appearance));
  return bridge?.onThemesChanged?.(() => void refreshUserThemes()) ?? (() => undefined);
}

export function setThemePreview(preview: TerminalTheme | null): void {
  register(useUserThemesStore.getState().themes, preview);
  useUserThemesStore.setState({ preview });
}

export async function setSeeThroughWindow(seeThrough: boolean): Promise<void> {
  const appearance = await getBridge()?.themesSetSeeThrough?.(seeThrough);
  if (appearance) useUserThemesStore.setState(appearance);
}

const CSS_ELEMENT_ID = "zorai-user-theme-css";
const CUSTOM_CSS_ELEMENT_ID = "zorai-custom-css";

export function applyCustomCss(css: string | null): void {
  if (typeof document === "undefined") return;
  let element = document.getElementById(CUSTOM_CSS_ELEMENT_ID);
  if (!css?.trim()) {
    element?.remove();
    return;
  }
  if (!element) {
    element = document.createElement("style");
    element.id = CUSTOM_CSS_ELEMENT_ID;
  }
  if (element.textContent !== css) element.textContent = css;
  document.head.appendChild(element);
}

export function applyThemeEffects(theme: TerminalTheme, backdrop: ThemeBackdrop, shell: Record<string, string>): void {
  if (typeof document === "undefined") return;
  const root = document.documentElement;
  if (theme.user?.corners === "square") root.dataset.corners = "square";
  else delete root.dataset.corners;
  if (backdrop === "none") {
    delete root.dataset.backdrop;
    root.style.removeProperty("--zorai-backdrop-solid");
    setBackdropVideo(null);
  } else {
    root.dataset.backdrop = backdrop;
    const user = theme.user;
    const video = backdrop === "image" && isVideoBackground(user?.backgroundPath);
    root.style.setProperty("--zorai-backdrop-image", backdrop === "image" && user?.backgroundPath && !video ? `url("${mediaUrlForPath(user.backgroundPath)}")` : "none");
    setBackdropVideo(video && user?.backgroundPath ? { src: mediaUrlForPath(user.backgroundPath), blur: user.blur } : null);
    root.style.setProperty("--zorai-backdrop-blur", `${user?.blur ?? 0}px`);
    root.style.setProperty("--zorai-backdrop-tint", translucentColor(shell["--bg-void"] ?? "#000000", user?.dim ?? 0));
    root.style.setProperty("--zorai-backdrop-solid", shell["--bg-elevated"] ?? shell["--bg-primary"] ?? "#000000");
  }
  const css = theme.user?.css?.trim() ? theme.user.css : null;
  let element = document.getElementById(CSS_ELEMENT_ID);
  if (!css) {
    element?.remove();
    return;
  }
  if (!element) {
    element = document.createElement("style");
    element.id = CSS_ELEMENT_ID;
    document.head.appendChild(element);
  }
  if (element.textContent !== css) element.textContent = css;
}
