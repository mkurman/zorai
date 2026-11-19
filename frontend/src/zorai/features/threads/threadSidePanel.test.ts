import { beforeEach, describe, expect, it } from "vitest";
import { keepsThreadPanelOpen, THREAD_PANEL_KEEP_OPEN_SELECTOR, useThreadPanelStore } from "./threadSidePanel";

describe("thread side panels", () => {
  beforeEach(() => useThreadPanelStore.setState({ open: null }));

  it("keeps only one panel open, so a new button replaces the previous panel instead of stacking", () => {
    const { toggle } = useThreadPanelStore.getState();
    toggle("handoff");
    toggle("participants");
    expect(useThreadPanelStore.getState().open).toBe("participants");
    toggle("participants");
    expect(useThreadPanelStore.getState().open).toBeNull();
  });

  it("closing a specific panel leaves a different open panel alone", () => {
    const { toggle, close } = useThreadPanelStore.getState();
    toggle("gateway");
    close("handoff");
    expect(useThreadPanelStore.getState().open).toBe("gateway");
    close("gateway");
    expect(useThreadPanelStore.getState().open).toBeNull();
  });
});

describe("closing thread panels on outside clicks", () => {
  const element = (matches: boolean) => ({ closest: (selector: string) => (matches && selector === THREAD_PANEL_KEEP_OPEN_SELECTOR ? ({} as Element) : null) });

  it("keeps the panel open for header toggles and popups opened from it, closes it for anything else", () => {
    expect(THREAD_PANEL_KEEP_OPEN_SELECTOR).toContain(".zorai-thread-header button");
    expect(THREAD_PANEL_KEEP_OPEN_SELECTOR).toContain('[role="menu"]');
    expect(keepsThreadPanelOpen(element(true))).toBe(true);
    expect(keepsThreadPanelOpen(element(false))).toBe(false);
  });
});
