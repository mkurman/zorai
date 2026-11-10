import { describe, expect, it } from "vitest";
import { CODE_PANEL_MIN_HEIGHT, clampCodePanelHeight, nextCodePanelState } from "./codeBottomPanelStore";
import { shouldLeaveKeyToTerminal } from "./codeCommands";

describe("code bottom panel", () => {
  it("toggles like VS Code: the same tab closes the panel, another tab switches to it", () => {
    expect(nextCodePanelState({ open: false, tab: "output" }, "terminal")).toEqual({ open: true, tab: "terminal" });
    expect(nextCodePanelState({ open: true, tab: "terminal" }, "terminal")).toEqual({ open: false, tab: "terminal" });
    expect(nextCodePanelState({ open: true, tab: "terminal" }, "output")).toEqual({ open: true, tab: "output" });
    expect(nextCodePanelState({ open: true, tab: "output" })).toEqual({ open: false, tab: "output" });
  });

  it("never lets the panel squeeze the editor away or collapse below a usable height", () => {
    expect(clampCodePanelHeight(20, 800)).toBe(CODE_PANEL_MIN_HEIGHT);
    expect(clampCodePanelHeight(2000, 800)).toBe(800 - CODE_PANEL_MIN_HEIGHT);
    expect(clampCodePanelHeight(300, 800)).toBe(300);
  });

  it("leaves shell shortcuts such as Ctrl+R and Ctrl+D to the terminal, but keeps the panel toggles working", () => {
    const inTerminal = { closest: (selector: string) => (selector.includes("zorai-code-panel__terminal") ? {} : null) };
    const inEditor = { closest: () => null };
    expect(shouldLeaveKeyToTerminal("file.reload", inTerminal)).toBe(true);
    expect(shouldLeaveKeyToTerminal("edit.selectNext", inTerminal)).toBe(true);
    expect(shouldLeaveKeyToTerminal("view.toggleTerminal", inTerminal)).toBe(false);
    expect(shouldLeaveKeyToTerminal("file.reload", inEditor)).toBe(false);
  });
});
