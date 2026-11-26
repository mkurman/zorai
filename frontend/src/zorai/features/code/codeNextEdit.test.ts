import { describe, expect, it } from "vitest";
import { describeLineEdits, parseNextEdit, rememberLineEdit } from "./codeNextEdit";

describe("next edit prediction", () => {
  it("keeps typing on one line as a single edit, so a rename counts once, not per keystroke", () => {
    let edits = rememberLineEdit([], { line: 4, text: "const user" });
    edits = rememberLineEdit(edits, { line: 4, text: "const userName" });
    edits = rememberLineEdit(edits, { line: 9, text: "x" });
    expect(edits).toEqual([{ line: 4, text: "const userName" }, { line: 9, text: "x" }]);
  });

  it("remembers only the latest few edits", () => {
    let edits: Array<{ line: number; text: string }> = [];
    for (let line = 1; line <= 8; line += 1) edits = rememberLineEdit(edits, { line, text: `l${line}` });
    expect(edits.map((edit) => edit.line)).toEqual([4, 5, 6, 7, 8]);
  });

  it("numbers edits and predictions inside the sent window, then maps them back to real lines", () => {
    expect(describeLineEdits([{ line: 120, text: "  let a = 1;" }, { line: 10, text: "far" }], 100)).toEqual(["line 21 is now: let a = 1;"]);
    expect(parseNextEdit('{"line":3,"replacement":"b()"}', 100)).toEqual({ line: 102, endLine: 102, replacement: "b()" });
    expect(parseNextEdit('{"line":3,"end_line":5,"replacement":"a\\nb"}', 100)).toEqual({ line: 102, endLine: 104, replacement: "a\nb" });
    expect(parseNextEdit('{"line":5,"end_line":3,"replacement":"a"}', 100)).toEqual({ line: 104, endLine: 104, replacement: "a" });
    expect(parseNextEdit("", 100)).toBeNull();
    expect(parseNextEdit("not json", 100)).toBeNull();
  });
});
