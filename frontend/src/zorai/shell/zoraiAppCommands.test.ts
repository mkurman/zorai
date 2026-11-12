import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { resolveZoraiAppCommand } from "./zoraiAppCommands";

function menuCommandsFromWindowRuntime(): string[] {
  const source = readFileSync(new URL("../../../electron/main/window-runtime.cjs", import.meta.url), "utf8");
  return [...source.matchAll(/sendAppCommand\('([^']+)'\)/g)].map((match) => match[1]);
}

describe("Zorai window-frame app commands", () => {
  it("opens the command palette instead of the old overlay-only toggle", () => {
    expect(resolveZoraiAppCommand("toggle-command-palette")).toEqual({ kind: "palette" });
  });

  it("sends Search to the thread list instead of terminal buffer find", () => {
    expect(resolveZoraiAppCommand("toggle-search")).toEqual({
      kind: "navigate",
      detail: { view: "threads", focusSearch: true },
    });
  });

  it("maps leftover in-flight menu actions onto current Zorai surfaces", () => {
    expect(resolveZoraiAppCommand("toggle-settings")).toEqual({
      kind: "navigate",
      detail: { view: "settings" },
    });
    expect(resolveZoraiAppCommand("about")).toEqual({
      kind: "navigate",
      detail: { view: "settings", settingsTab: "about" },
    });
    expect(resolveZoraiAppCommand("new-surface")).toEqual({ kind: "new-thread" });
    expect(resolveZoraiAppCommand("toggle-file-manager")).toEqual({
      kind: "navigate",
      detail: { view: "tools", tool: "files" },
    });
    expect(resolveZoraiAppCommand("toggle-sidebar")).toEqual({
      kind: "navigate",
      detail: { toggleContext: true },
    });
  });

  it("opens Developer Tools from the Features menu and checks for updates from Help", () => {
    expect(resolveZoraiAppCommand("developer-tools")).toEqual({ kind: "navigate", detail: { view: "settings", settingsTab: "developer" } });
    expect(resolveZoraiAppCommand("check-updates")).toEqual({ kind: "check-updates" });
    const menus = readFileSync(new URL("../../../electron/main/window-runtime.cjs", import.meta.url), "utf8");
    expect(menus).toMatch(/label: 'Features'[\s\S]*sendAppCommand\('developer-tools'\)[\s\S]*label: 'Window'/);
    expect(menus).toMatch(/label: 'Help'[\s\S]*sendAppCommand\('check-updates'\)/);
  });

  it("leaves terminal edit commands to the terminal listener", () => {
    expect(resolveZoraiAppCommand("copy")).toEqual({ kind: "unhandled" });
    expect(resolveZoraiAppCommand("paste")).toEqual({ kind: "unhandled" });
    expect(resolveZoraiAppCommand("select-all")).toEqual({ kind: "unhandled" });
  });

  it("covers every Electron menu app-command except clipboard roles", () => {
    const passthrough = new Set(["copy", "paste", "select-all"]);
    for (const command of menuCommandsFromWindowRuntime()) {
      const resolution = resolveZoraiAppCommand(command);
      if (passthrough.has(command)) {
        expect(resolution.kind, command).toBe("unhandled");
        continue;
      }
      expect(resolution.kind, command).not.toBe("unhandled");
    }
  });
});
