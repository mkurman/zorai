import { describe, expect, it } from "vitest";
import { APP_SHORTCUTS, eventMatchesShortcut, isCompleteCombo, menuAccelerators, shortcutCombo, shortcutConflicts } from "./appShortcuts";

const key = (init: Partial<KeyboardEvent>) => ({ ctrlKey: false, metaKey: false, altKey: false, shiftKey: false, key: "", ...init }) as KeyboardEvent;

describe("app shortcuts", () => {
  it("uses the default until overridden, and an empty override removes the shortcut entirely", () => {
    expect(shortcutCombo({}, "thread.recordVoice")).toBe("Ctrl+M");
    expect(shortcutCombo({ "thread.recordVoice": "Alt+V" }, "thread.recordVoice")).toBe("Alt+V");
    expect(shortcutCombo({ "thread.recordVoice": "" }, "thread.recordVoice")).toBe("");
    expect(eventMatchesShortcut({ "thread.recordVoice": "" }, "thread.recordVoice", key({ ctrlKey: true, key: "m" }))).toBe(false);
  });

  it("matches the pressed keys exactly, so Ctrl+Shift+M does not also trigger Ctrl+M", () => {
    expect(eventMatchesShortcut({}, "thread.recordVoice", key({ ctrlKey: true, key: "m" }))).toBe(true);
    expect(eventMatchesShortcut({}, "thread.recordVoice", key({ ctrlKey: true, shiftKey: true, key: "M" }))).toBe(false);
    expect(eventMatchesShortcut({ "go.code": "Alt+1" }, "go.code", key({ altKey: true, key: "1" }))).toBe(true);
  });

  it("reports every shortcut sharing a key so the editor can warn before one silently shadows another", () => {
    const conflicts = shortcutConflicts({ "go.code": "Ctrl+M", "app.nightMode": "Alt+N" });
    expect(conflicts.get("Ctrl+M")?.sort()).toEqual(["go.code", "thread.recordVoice"]);
    expect(conflicts.has("Alt+N")).toBe(false);
    expect(shortcutConflicts({}).size).toBe(0);
  });

  it("waits for a real key, not just modifiers, while recording a new shortcut", () => {
    expect(isCompleteCombo("Ctrl+Shift")).toBe(false);
    expect(isCompleteCombo("Ctrl")).toBe(false);
    expect(isCompleteCombo("Ctrl+K")).toBe(true);
  });

  it("sends menu-bar commands to the desktop in Electron's format, with removed shortcuts as empty", () => {
    const accelerators = menuAccelerators({ "app.commandPalette": "Ctrl+ArrowUp", "app.newThread": "", "app.settings": "Ctrl+Esc" });
    expect(accelerators["toggle-command-palette"]).toBe("Ctrl+Up");
    expect(accelerators["new-surface"]).toBe("");
    expect(accelerators["toggle-settings"]).toBe("Ctrl+Escape");
    expect(accelerators["toggle-sidebar"]).toBe("Ctrl+B");
    expect(Object.keys(accelerators).sort()).toEqual(APP_SHORTCUTS.filter((shortcut) => shortcut.menuCommand).map((shortcut) => shortcut.menuCommand).sort());
  });

  it("gives every command a unique id and none of the defaults collide", () => {
    expect(new Set(APP_SHORTCUTS.map((shortcut) => shortcut.id)).size).toBe(APP_SHORTCUTS.length);
    expect(shortcutConflicts({}).size).toBe(0);
  });
});
