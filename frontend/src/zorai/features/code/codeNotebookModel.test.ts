import { describe, expect, it } from "vitest";
import { cellSource, insertCell, kernelOutputs, moveCell, parseNotebook, serializeNotebook, toSourceLines, updateCell } from "./codeNotebookModel";

const original = JSON.stringify({
  cells: [
    { cell_type: "markdown", metadata: {}, source: ["# Title\n", "text"] },
    { cell_type: "code", execution_count: 3, metadata: { tags: ["keep"] }, outputs: [{ output_type: "stream", name: "stdout", text: ["hi\n"] }], source: ["print('hi')"] },
  ],
  metadata: { kernelspec: { name: "python3", display_name: "Python 3", language: "python" }, custom: { x: 1 } },
  nbformat: 4,
  nbformat_minor: 5,
}, null, 1);

describe("notebook model", () => {
  it("round-trips nbformat JSON without dropping metadata, outputs or fields it does not know", () => {
    const notebook = parseNotebook(original)!;
    const saved = parseNotebook(serializeNotebook(notebook))!;
    expect(saved).toEqual(JSON.parse(original));
    expect(serializeNotebook(notebook).endsWith("}\n")).toBe(true);
  });

  it("edits a cell's source as nbformat line arrays and keeps its outputs and metadata", () => {
    const notebook = updateCell(parseNotebook(original)!, 1, { source: toSourceLines("a = 1\nprint(a)\n") });
    expect(notebook.cells[1].source).toEqual(["a = 1\n", "print(a)\n"]);
    expect(cellSource(notebook.cells[1])).toBe("a = 1\nprint(a)\n");
    expect(notebook.cells[1].metadata).toEqual({ tags: ["keep"] });
    expect(toSourceLines("")).toEqual([]);
  });

  it("adds and reorders cells", () => {
    const notebook = moveCell(insertCell(parseNotebook(original)!, 2, "code"), 2, -1);
    expect(notebook.cells.map((cell) => cell.cell_type)).toEqual(["markdown", "code", "code"]);
    expect(notebook.cells[1].outputs).toEqual([]);
  });

  it("turns kernel results into standard Jupyter outputs", () => {
    const outputs = kernelOutputs({ stdout: "a\nb\n", stderr: "", result: "42", html: null, images: ["iVBOR"], error: null }, 7);
    expect(outputs.map((output) => output.output_type)).toEqual(["stream", "display_data", "execute_result"]);
    expect(outputs[2]).toEqual({ output_type: "execute_result", execution_count: 7, data: { "text/plain": ["42"] }, metadata: {} });
    expect(kernelOutputs({ stdout: "", stderr: "", result: null, html: null, images: [], error: { ename: "E", evalue: "v", traceback: ["t"] } }, 1)[0].output_type).toBe("error");
  });
});
