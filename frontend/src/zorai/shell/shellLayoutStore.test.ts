import { beforeEach, describe, expect, it } from "vitest";
import { SHELL_CONTEXT_MIN_WIDTH, SHELL_RAIL_MIN_WIDTH, useShellLayoutStore } from "./shellLayoutStore";

describe("shell layout store", () => {
  beforeEach(() => {
    useShellLayoutStore.setState({ railWidths: {}, contextWidths: {} });
  });

  it("keeps each view's sidebar and context widths independent so resizing one view never reshapes another", () => {
    const store = useShellLayoutStore.getState();
    store.setRailWidth("threads", 400);
    store.setRailWidth("database", 220);
    store.setContextWidth("threads", 500);

    const state = useShellLayoutStore.getState();
    expect(state.railWidths).toEqual({ threads: 400, database: 220 });
    expect(state.contextWidths).toEqual({ threads: 500 });
  });

  it("resets only the targeted view back to its default", () => {
    const store = useShellLayoutStore.getState();
    store.setRailWidth("threads", 400);
    store.setRailWidth("goals", 300);
    store.resetRailWidth("threads");

    expect(useShellLayoutStore.getState().railWidths).toEqual({ goals: 300 });
  });

  it("never persists widths below the usable minimum", () => {
    const store = useShellLayoutStore.getState();
    store.setRailWidth("threads", 10);
    store.setContextWidth("threads", 10);

    const state = useShellLayoutStore.getState();
    expect(state.railWidths.threads).toBe(SHELL_RAIL_MIN_WIDTH);
    expect(state.contextWidths.threads).toBe(SHELL_CONTEXT_MIN_WIDTH);
  });
});
