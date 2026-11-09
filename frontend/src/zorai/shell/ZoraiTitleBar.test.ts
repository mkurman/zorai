import { describe, expect, it } from "vitest";
import { formatMenuAccelerator, menuForMnemonic, stepMenuIndex } from "./ZoraiTitleBar";

describe("formatMenuAccelerator", () => {
  it("shows the platform modifier the operator actually presses instead of Electron's accelerator names", () => {
    expect(formatMenuAccelerator("CommandOrControl+Shift+I", false)).toBe("Ctrl+Shift+I");
    expect(formatMenuAccelerator("CommandOrControl+Shift+I", true)).toBe("Cmd+Shift+I");
    expect(formatMenuAccelerator("Control+Plus", false)).toBe("Ctrl++");
    expect(formatMenuAccelerator("Ctrl+B", false)).toBe("Ctrl+B");
  });
});

describe("title bar menu keyboard helpers", () => {
  it("wraps arrow navigation so the operator never gets stuck at the first or last item", () => {
    expect(stepMenuIndex(-1, 4, 1)).toBe(0);
    expect(stepMenuIndex(-1, 4, -1)).toBe(3);
    expect(stepMenuIndex(3, 4, 1)).toBe(0);
    expect(stepMenuIndex(0, 4, -1)).toBe(3);
    expect(stepMenuIndex(0, 0, 1)).toBe(-1);
  });

  it("opens the menu whose first letter matches Alt+key, like a native menu bar", () => {
    const labels = ["File", "Edit", "View", "Features", "Window", "Help"];
    expect(menuForMnemonic(labels, "e")).toBe("Edit");
    expect(menuForMnemonic(labels, "W")).toBe("Window");
    expect(menuForMnemonic(labels, "f")).toBe("File");
    expect(menuForMnemonic(labels, "z")).toBeNull();
  });
});
