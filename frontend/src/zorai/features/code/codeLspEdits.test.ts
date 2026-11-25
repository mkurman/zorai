import { describe, expect, it } from "vitest";
import { applyLspTextEdits, lspUriToRelativePath, lspWorkspaceEditFiles } from "./codeLspEdits";

const edit = (line: number, start: number, end: number, newText: string) => ({ range: { start: { line, character: start }, end: { line, character: end } }, newText });

describe("LSP workspace edits", () => {
  it("renames every occurrence without shifting later edits, keeping CRLF intact", () => {
    const source = "let count = 1;\r\nprint(count, count);\r\n";
    expect(applyLspTextEdits(source, [edit(0, 4, 9, "total"), edit(1, 6, 11, "total"), edit(1, 13, 18, "total")])).toBe("let total = 1;\r\nprint(total, total);\r\n");
  });

  it("never lets an edit past the line end swallow the line break", () => {
    expect(applyLspTextEdits("ab\r\ncd", [edit(0, 1, 99, "X")])).toBe("aX\r\ncd");
  });

  it("only touches files inside the workspace and reports what it had to skip", () => {
    const result = lspWorkspaceEditFiles({
      documentChanges: [
        { textDocument: { uri: "file:///w/src/a.rs" }, edits: [edit(0, 0, 1, "x")] },
        { textDocument: { uri: "file:///elsewhere/b.rs" }, edits: [edit(0, 0, 1, "x")] },
        { kind: "rename", oldUri: "file:///w/a.rs", newUri: "file:///w/b.rs" },
      ],
    }, "/w");
    expect(result.files.map((file) => file.path)).toEqual(["src/a.rs"]);
    expect(result.skipped).toBe(2);
  });

  it("does not treat a sibling folder sharing the root prefix as inside the workspace", () => {
    expect(lspUriToRelativePath("file:///work-other/a.py", "/work")).toBeNull();
    expect(lspUriToRelativePath("file:///work/pkg/a%20b.py", "/work")).toBe("pkg/a b.py");
  });
});
