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

describe("LSP refactors that move files", () => {
  it("keeps file creates, renames and deletes in the server's order so edits land on the renamed file", async () => {
    const { lspWorkspaceEditSteps, describeLspEditSteps } = await import("./codeLspEdits");
    const { steps, skipped } = lspWorkspaceEditSteps({
      documentChanges: [
        { kind: "rename", oldUri: "file:///w/src/old.rs", newUri: "file:///w/src/new.rs" },
        { textDocument: { uri: "file:///w/src/new.rs" }, edits: [edit(0, 0, 0, "// moved\n")] },
        { kind: "create", uri: "file:///w/src/mod.rs" },
        { kind: "delete", uri: "file:///outside/x.rs" },
      ],
    }, "/w");
    expect(steps.map((step) => step.kind)).toEqual(["rename", "edit", "create"]);
    expect(skipped).toBe(1);
    expect(describeLspEditSteps(steps)).toEqual(["Rename src/old.rs → src/new.rs", "Create src/mod.rs"]);
  });
});
