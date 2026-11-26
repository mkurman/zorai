import { describe, expect, it } from "vitest";
import { gutterChanges, revertGutterEdit, type GutterChange } from "./codeGitGutter";

function applyRevert(current: string, change: GutterChange): string {
  const lines = current.split("\n");
  const edit = revertGutterEdit(change, lines.length, (line) => lines[line - 1].length + 1);
  const offset = (line: number, column: number) => lines.slice(0, line - 1).reduce((sum, text) => sum + text.length + 1, 0) + column - 1;
  return current.slice(0, offset(edit.startLine, edit.startColumn)) + edit.text + current.slice(offset(edit.endLine, edit.endColumn));
}

describe("git gutter", () => {
  const base = "a\nb\nc\nd";

  it("marks added, modified and deleted lines where the editor shows them", () => {
    expect(gutterChanges(base, "a\nb\nNEW\nc\nd").map(({ kind, startLine, endLine }) => ({ kind, startLine, endLine }))).toEqual([{ kind: "added", startLine: 3, endLine: 3 }]);
    expect(gutterChanges(base, "a\nB\nc\nd").map(({ kind, startLine }) => ({ kind, startLine }))).toEqual([{ kind: "modified", startLine: 2 }]);
    expect(gutterChanges(base, "a\nd").map(({ kind, startLine }) => ({ kind, startLine }))).toEqual([{ kind: "deleted", startLine: 1 }]);
    expect(gutterChanges(base, base)).toEqual([]);
  });

  it("reverting any change restores exactly the committed text, including edits at the file edges", () => {
    for (const current of ["a\nb\nNEW\nc\nd", "a\nB\nc\nd", "a\nd", "b\nc\nd", "a\nb\nc", "a\nb\nc\nd\nX\nY", "TOP\na\nb\nc\nd"]) {
      const [change] = gutterChanges(base, current);
      expect(applyRevert(current, change)).toBe(base);
    }
  });
});

describe("stage selected lines", async () => {
  const { stageSelectedLines } = await import("./codeGitGutter");
  it("stages only the changes the selection touches and leaves other edits unstaged", () => {
    const index = "a\nb\nc\nd\ne";
    const working = "A\nb\nc\nD\ne\nf";
    expect(stageSelectedLines(index, working, 1, 1)).toBe("A\nb\nc\nd\ne");
    expect(stageSelectedLines(index, working, 4, 6)).toBe("a\nb\nc\nD\ne\nf");
    expect(stageSelectedLines(index, working, 2, 3)).toBe(index);
    expect(stageSelectedLines("a\nb\nc", "a\nc", 1, 2)).toBe("a\nc");
  });
});
