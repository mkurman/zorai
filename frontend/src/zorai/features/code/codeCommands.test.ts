import { describe, expect, it } from "vitest";
import {
  CODE_COMMANDS,
  codeCommandById,
  codeCommandConflicts,
  displayCodeBinding,
  matchesCodeBinding,
  shouldPassthroughCodeCommand,
 RUNNABLE_CODE_COMMANDS } from "./codeCommands";

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
  it("keeps commands with no handler out of the palette and key handling, so Ctrl+B is not swallowed", () => {
    const ids = RUNNABLE_CODE_COMMANDS.map((command) => command.id);
    expect(ids).not.toContain("view.toggleExplorer");
    expect(ids).not.toContain("file.reopenClosed");
    expect(ids).toContain("file.close");
    expect(ids).toContain("edit.copy");
  });
});
