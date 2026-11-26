import { describe, expect, it } from "vitest";
import {
  CODE_COMMANDS,
  codeCommandById,
  codeCommandConflicts,
  displayCodeBinding,
  matchesCodeBinding,
  codeBindingFromEvent,
  codeBindingOwner,
  codeBindingKeys,
  shouldPassthroughCodeCommand,
 RUNNABLE_CODE_COMMANDS, UNWIRED_CODE_COMMANDS } from "./codeCommands";

const REQUIRED = [
  "file.save", "file.saveAll", "file.reload", "file.quickOpen", "search.file", "search.replace",
  "search.project", "navigation.line", "navigation.definition", "navigation.references",
  "edit.deleteLine", "edit.duplicate", "edit.moveUp", "edit.moveDown", "edit.uppercase", "edit.lowercase",
  "edit.commentLine", "edit.commentBlock", "edit.selectNext", "edit.selectAllOccurrences",
  "edit.formatDocument", "edit.formatSelection", "edit.trimWhitespace", "edit.insertLineAbove",
  "edit.insertLineBelow", "edit.joinLines", "edit.sortAscending", "edit.sortDescending",
  "view.toggleExplorer", "view.toggleAgent", "view.settings", "view.commandPalette",
] as const;

describe("Code command registry", () => {
  it("contains unique ids and the approved command inventory", () => {
    expect(new Set(CODE_COMMANDS.map((command) => command.id)).size).toBe(CODE_COMMANDS.length);
    for (const id of REQUIRED) expect(codeCommandById(id)).toBeDefined();
  });

  it("normalizes CtrlCmd display per platform", () => {
    expect(displayCodeBinding("CtrlCmd+Shift+P", "darwin")).toBe("Cmd+Shift+P");
    expect(displayCodeBinding("CtrlCmd+Shift+P", "linux")).toBe("Ctrl+Shift+P");
  });

  it("matches Code bindings from keyboard events", () => {
    expect(matchesCodeBinding("CtrlCmd+S", { key: "s", ctrlKey: true, metaKey: false, altKey: false, shiftKey: false })).toBe(true);
    expect(matchesCodeBinding("Shift+Alt+ArrowDown", { key: "ArrowDown", ctrlKey: false, metaKey: false, altKey: true, shiftKey: true })).toBe(true);
  });

  it("reports duplicate active Code bindings", () => {
    expect(codeCommandConflicts({ "file.save": "CtrlCmd+S", "file.reload": "CtrlCmd+S" })).toEqual([
      { binding: "CtrlCmd+S", commandIds: ["file.reload", "file.save"] },
    ]);
  });

  it("lets native clipboard shortcuts reach the Code agent composer", () => {
    const composer = { tagName: "TEXTAREA", readOnly: false, disabled: false } as EventTarget;
    const search = { tagName: "INPUT", type: "text", readOnly: false, disabled: false } as EventTarget;
    const monacoInput = {
      tagName: "TEXTAREA",
      closest: (selector: string) => selector === ".monaco-editor" ? {} : null,
    } as EventTarget;

    expect(shouldPassthroughCodeCommand("edit.copy", composer)).toBe(true);
    expect(shouldPassthroughCodeCommand("edit.paste", composer)).toBe(true);
    expect(shouldPassthroughCodeCommand("edit.cut", search)).toBe(true);
    expect(shouldPassthroughCodeCommand("edit.selectAll", monacoInput)).toBe(true);
    expect(shouldPassthroughCodeCommand("file.save", composer)).toBe(false);
    expect(shouldPassthroughCodeCommand("edit.copy", { tagName: "DIV" } as EventTarget)).toBe(false);
    expect(shouldPassthroughCodeCommand("edit.copy", null)).toBe(false);
  });
});

describe("runnable code commands", () => {
  it("runs every declared command, so no palette entry or shortcut is silently dead", () => {
    const ids = RUNNABLE_CODE_COMMANDS.map((command) => command.id);
    expect(UNWIRED_CODE_COMMANDS.size).toBe(0);
    expect(ids).toEqual(expect.arrayContaining(["view.toggleExplorer", "view.toggleAgent", "file.reopenClosed", "file.close", "edit.copy"]));
  });

  it("leaves Ctrl+Shift+T to the app's Reopen closed thread; Reopen Closed Editor is unbound by default", () => {
    expect(codeCommandById("file.reopenClosed")?.defaultKeybinding).toBeNull();
    expect(codeCommandById("view.toggleExplorer")?.defaultKeybinding).toBe("CtrlCmd+B");
  });
});

describe("shortcut matching", () => {
  const key = (keyName: string, mods: { ctrlKey?: boolean; altKey?: boolean; shiftKey?: boolean; metaKey?: boolean }) => ({ key: keyName, ctrlKey: false, altKey: false, shiftKey: false, metaKey: false, ...mods });

  it("does not steal Ctrl+Alt+Left (focus left pane) for the Alt+Left Go Back shortcut", () => {
    expect(matchesCodeBinding("Alt+ArrowLeft", key("ArrowLeft", { altKey: true }))).toBe(true);
    expect(matchesCodeBinding("Alt+ArrowLeft", key("ArrowLeft", { altKey: true, ctrlKey: true }))).toBe(false);
    expect(matchesCodeBinding("CtrlCmd+S", key("s", { metaKey: true }))).toBe(true);
  });

  it("records pressed keys as a binding that the dispatcher will match", () => {
    const pressed = { key: "k", ctrlKey: true, metaKey: false, altKey: true, shiftKey: true };
    const binding = codeBindingFromEvent(pressed);
    expect(binding).toBe("CtrlCmd+Shift+Alt+K");
    expect(matchesCodeBinding(binding!, pressed)).toBe(true);
    expect(matchesCodeBinding(binding!, { ...pressed, altKey: false })).toBe(false);
    expect(codeBindingFromEvent({ key: "F5", ctrlKey: false, metaKey: false, altKey: false, shiftKey: false })).toBe("F5");
    expect(codeBindingFromEvent({ key: "Control", ctrlKey: true, metaKey: false, altKey: false, shiftKey: false })).toBeNull();
  });

  it("finds which command already owns a shortcut so reassigning can warn before stealing it", () => {
    expect(codeBindingOwner("CtrlCmd+S", {}, "file.close")?.id).toBe("file.save");
    expect(codeBindingOwner("ctrlcmd+s", {}, "file.save")).toBeUndefined();
    expect(codeBindingOwner("CtrlCmd+S", { "file.save": null }, "file.close")).toBeUndefined();
    expect(codeBindingOwner("F9", { "file.save": "F9" }, "file.close")?.id).toBe("file.save");
  });

  it("splits a binding into keycaps, keeping the plus key intact", () => {
    expect(codeBindingKeys("CtrlCmd+Shift+S", "linux")).toEqual(["Ctrl", "Shift", "S"]);
    expect(codeBindingKeys("CtrlCmd++", "darwin")).toEqual(["Cmd", "+"]);
  });
});
