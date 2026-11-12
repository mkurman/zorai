import { beforeEach, describe, expect, it } from "vitest";
import { useThreadPanelStore } from "./threadSidePanel";

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
