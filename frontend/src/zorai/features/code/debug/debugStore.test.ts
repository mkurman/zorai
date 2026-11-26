import { describe, expect, it } from "vitest";
import { breakpointSpecs, compoundMembers, relativeToRoot, toggleBreakpointIn } from "./debugStore";

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

describe("compound launch configurations", () => {
  const server = { name: "Server", type: "node", request: "launch" };
  const client = { name: "Client", type: "node", request: "launch" };

  it("starts the named configurations in the order the compound lists them", () => {
    const compound = { name: "Full stack", type: "compound", request: "launch", configurations: ["Client", "Server"] };
    expect(compoundMembers(compound, [server, client, compound]).map((config) => config.name)).toEqual(["Client", "Server"]);
  });

  it("refuses to start when a named configuration is missing instead of launching half the stack", () => {
    const compound = { name: "Broken", type: "compound", request: "launch", configurations: ["Server", "Worker"] };
    expect(() => compoundMembers(compound, [server, compound])).toThrow(/Worker/);
  });
});
