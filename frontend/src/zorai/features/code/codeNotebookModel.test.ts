import { describe, expect, it } from "vitest";
import { appendLiveOutput, cellSource, insertCell, kernelOutputs, notebookCommandKey, moveCell, parseNotebook, serializeNotebook, toSourceLines, updateCell } from "./codeNotebookModel";

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

  it("opens an empty .ipynb as a blank notebook so a freshly created file is editable instead of rejected", () => {
    const notebook = parseNotebook("  \n");
    expect(notebook?.cells).toHaveLength(1);
    expect(notebook?.cells[0].cell_type).toBe("code");
    expect(notebook?.nbformat).toBe(4);
    expect(parseNotebook("{not json")).toBeNull();
  });

  it("merges streamed print chunks into one output and honours clear_output, like Jupyter does live", () => {
    let outputs = appendLiveOutput([], { output_type: "stream", name: "stdout", text: "0\n" });
    outputs = appendLiveOutput(outputs, { output_type: "stream", name: "stdout", text: "1\n" });
    expect(outputs).toEqual([{ output_type: "stream", name: "stdout", text: ["0\n", "1\n"] }]);
    outputs = appendLiveOutput(outputs, { output_type: "stream", name: "stderr", text: "warn" });
    expect(outputs).toHaveLength(2);
    expect(appendLiveOutput(outputs, { output_type: "clear_output" })).toEqual([]);
  });

  it("keeps Jupyter kernel outputs as-is but stamps the notebook's execution count on the result", () => {
    const outputs = kernelOutputs({ outputs: [
      { output_type: "stream", name: "stdout", text: "a" },
      { output_type: "stream", name: "stdout", text: "b\n" },
      { output_type: "execute_result", execution_count: 1, data: { "text/plain": "2" }, metadata: {} },
    ] }, 9);
    expect(outputs[0]).toEqual({ output_type: "stream", name: "stdout", text: ["ab\n"] });
    expect(outputs[1].execution_count).toBe(9);
  });

  it("maps Jupyter command-mode keys: A/B insert around the selected cell, arrows move, Enter edits", () => {
    expect(notebookCommandKey("a", 2, 5)).toEqual({ insertAt: 2 });
    expect(notebookCommandKey("b", 2, 5)).toEqual({ insertAt: 3 });
    expect(notebookCommandKey("ArrowUp", 0, 5)).toEqual({ select: 0 });
    expect(notebookCommandKey("ArrowDown", 4, 5)).toEqual({ select: 4 });
    expect(notebookCommandKey("Enter", 1, 5)).toEqual({ edit: true });
    expect(notebookCommandKey("x", 1, 5)).toBeNull();
  });
});
