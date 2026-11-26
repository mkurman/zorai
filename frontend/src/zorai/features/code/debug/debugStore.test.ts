import { describe, expect, it } from "vitest";
import { breakpointSpecs, relativeToRoot, toggleBreakpointIn } from "./debugStore";

describe("debug breakpoints", () => {
  it("toggles a breakpoint on and off and keeps them in line order for the gutter and the adapter", () => {
    let list = toggleBreakpointIn([], 9);
    list = toggleBreakpointIn(list, 3);
    expect(list.map((entry) => entry.line)).toEqual([3, 9]);
    expect(toggleBreakpointIn(list, 9).map((entry) => entry.line)).toEqual([3]);
  });

  it("sends only enabled breakpoints with their conditions, so a disabled one never pauses the program", () => {
    expect(breakpointSpecs([{ line: 2, enabled: false }, { line: 5, enabled: true, condition: "i > 3" }])).toEqual([{ line: 5, condition: "i > 3", logMessage: undefined, hitCondition: undefined }]);
  });

  it("maps adapter frame paths back into the workspace, ignoring library frames outside it", () => {
    expect(relativeToRoot("/repo/", "/repo/src/main.c")).toBe("src/main.c");
    expect(relativeToRoot("C:\\repo", "C:\\repo\\a.py")).toBe("a.py");
    expect(relativeToRoot("/repo", "/usr/lib/libc.so")).toBeNull();
  });
});
