import { beforeEach, describe, expect, it } from "vitest";
import { useSettingsStore } from "@/lib/settingsStore";
import { useCodeBottomPanelStore } from "@/zorai/features/code/codeBottomPanelStore";
import { useCodeLayoutStore } from "@/zorai/features/code/codeLayoutStore";
import { useShellLayoutStore } from "@/zorai/shell/shellLayoutStore";
import { ZORAI_NAVIGATE_EVENT } from "@/zorai/shell/zoraiNavigationEvents";
import { applyLayout, captureLayout, useLayoutPresetsStore, type LayoutSnapshot } from "./layoutPresets";

const navigations: unknown[] = [];
const fakeWindow = Object.assign(new EventTarget(), { setTimeout, clearTimeout });
fakeWindow.addEventListener(ZORAI_NAVIGATE_EVENT, (event) => navigations.push((event as CustomEvent).detail));
(globalThis as { window?: unknown }).window = fakeWindow;

describe("saved layouts", () => {
  beforeEach(() => {
    navigations.length = 0;
    useLayoutPresetsStore.setState({ presets: [] });
    useShellLayoutStore.setState({ railOpen: true, contextOpen: false, codeContextAutoOpened: false, lastWorkView: null, railWidths: {}, contextWidths: {} });
    useCodeBottomPanelStore.setState({ open: false, tab: "terminal", height: 260 });
  });

  it("captures the view you were working in, not Settings where the layout is saved from", () => {
    useShellLayoutStore.setState({ lastWorkView: "code", railOpen: false, contextOpen: true, railWidths: { code: 300 } });
    useCodeBottomPanelStore.setState({ open: true, tab: "output", height: 333 });
    const layout = captureLayout();
    expect(layout).toMatchObject({ view: "code", railOpen: false, contextOpen: true, railWidths: { code: 300 }, bottomPanel: { open: true, tab: "output", height: 333 } });
    useShellLayoutStore.setState({ lastWorkView: "settings" });
    expect(captureLayout().view).toBe("threads");
  });

  it("restores every part of a captured layout and stops the Code view from reopening a panel the layout closed", () => {
    useShellLayoutStore.setState({ lastWorkView: "code", railOpen: false, contextOpen: false, railWidths: { threads: 250 }, contextWidths: { goals: 400 } });
    useCodeBottomPanelStore.setState({ open: true, tab: "output", height: 300 });
    useSettingsStore.getState().updateSetting("chatWidth", "narrow");
    const saved = captureLayout();

    useShellLayoutStore.setState({ railOpen: true, contextOpen: true, railWidths: {}, contextWidths: {} });
    useCodeBottomPanelStore.setState({ open: false, tab: "terminal", height: 200 });
    useSettingsStore.getState().updateSetting("chatWidth", "full");
    applyLayout(saved);

    expect(useShellLayoutStore.getState()).toMatchObject({ railOpen: false, contextOpen: false, railWidths: { threads: 250 }, contextWidths: { goals: 400 }, codeContextAutoOpened: true });
    expect(useCodeBottomPanelStore.getState()).toMatchObject({ open: true, tab: "output", height: 300 });
    expect(useSettingsStore.getState().settings.chatWidth).toBe("narrow");
    expect(useCodeLayoutStore.getState().explorerPreferredWidth).toBe(saved.codeExplorerWidth);
    expect(navigations).toEqual([{ view: "code" }]);
  });

  it("survives a hand-edited or older saved layout instead of breaking the shell", () => {
    expect(() => applyLayout({ view: "nowhere", railWidths: { threads: "wide", code: 280 }, bottomPanel: { open: "yes", tab: "weird", height: "tall" }, chatWidth: "huge" } as unknown as LayoutSnapshot)).not.toThrow();
    expect(useShellLayoutStore.getState()).toMatchObject({ railOpen: true, contextOpen: false, railWidths: { code: 280 } });
    expect(useCodeBottomPanelStore.getState()).toMatchObject({ open: false, tab: "terminal" });
    expect(navigations).toEqual([]);
  });

  it("saving under an existing name replaces that layout instead of adding a duplicate, and delete removes it", () => {
    const { save, remove } = useLayoutPresetsStore.getState();
    const first = save("Coding", captureLayout());
    useShellLayoutStore.setState({ railOpen: false });
    const second = save("  coding ", captureLayout());
    expect(second.id).toBe(first.id);
    expect(useLayoutPresetsStore.getState().presets).toHaveLength(1);
    expect(useLayoutPresetsStore.getState().presets[0].layout.railOpen).toBe(false);
    save("Review", captureLayout());
    remove(first.id);
    expect(useLayoutPresetsStore.getState().presets.map((preset) => preset.name)).toEqual(["Review"]);
  });
});
