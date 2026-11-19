import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { resolveZoraiAppCommand } from "./zoraiAppCommands";

function menuCommandsFromWindowRuntime(): string[] {
  const source = readFileSync(new URL("../../../electron/main/window-runtime.cjs", import.meta.url), "utf8");
  return [...source.matchAll(/(?:sendAppCommand\(|commandItem\('[^']+', )'([^']+)'/g)].map((match) => match[1]);
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

  it("keeps help actions together under Help: shortcuts, developer settings, updates", () => {
    expect(resolveZoraiAppCommand("keyboard-shortcuts")).toEqual({ kind: "navigate", detail: { view: "settings", settingsTab: "shortcuts" } });
    expect(resolveZoraiAppCommand("developer-tools")).toEqual({ kind: "navigate", detail: { view: "settings", settingsTab: "developer" } });
    expect(resolveZoraiAppCommand("check-updates")).toEqual({ kind: "check-updates" });
    const menus = readFileSync(new URL("../../../electron/main/window-runtime.cjs", import.meta.url), "utf8");
    const help = menus.slice(menus.indexOf("label: 'Help'"));
    expect(help).toMatch(/'keyboard-shortcuts'[\s\S]*'developer-tools'[\s\S]*'check-updates'[\s\S]*'about'/);
    expect(menus).not.toMatch(/label: 'Features'/);
  });

  it("menu thread actions reach the thread view even when it mounts after the command", async () => {
    const { consumePendingThreadCommand, handleZoraiAppCommand, ZORAI_THREAD_COMMAND_EVENT } = await import("./zoraiAppCommands");
    const events: string[] = [];
    const listener = (event: Event) => events.push(event.type);
    globalThis.window = Object.assign(new EventTarget(), { setTimeout }) as unknown as Window & typeof globalThis;
    window.addEventListener(ZORAI_THREAD_COMMAND_EVENT, listener);
    expect(handleZoraiAppCommand("split-right")).toBe(true);
    expect(events).toContain(ZORAI_THREAD_COMMAND_EVENT);
    expect(consumePendingThreadCommand()).toBe("split-right");
    expect(consumePendingThreadCommand()).toBeNull();
  });

  it("gives every view, thread panel and thread action in the Go and View menus a handler", () => {
    expect(resolveZoraiAppCommand("go-code")).toEqual({ kind: "navigate", detail: { view: "code" } });
    expect(resolveZoraiAppCommand("thread-panel-participants")).toEqual({ kind: "thread-panel", panel: "participants" });
    expect(resolveZoraiAppCommand("next-thread")).toEqual({ kind: "thread-command", command: "next-thread" });
    expect(resolveZoraiAppCommand("toggle-rail")).toEqual({ kind: "toggle-rail" });
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
