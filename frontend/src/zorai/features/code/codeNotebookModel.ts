export type NotebookOutput = Record<string, unknown> & { output_type: string };
export type NotebookCell = Record<string, unknown> & { cell_type: "code" | "markdown" | "raw"; source: string | string[]; outputs?: NotebookOutput[]; execution_count?: number | null; metadata?: Record<string, unknown> };
export type Notebook = Record<string, unknown> & { cells: NotebookCell[]; metadata?: Record<string, unknown>; nbformat?: number; nbformat_minor?: number };
export type KernelResult = { stdout?: string; stderr?: string; result?: string | null; html?: string | null; images?: string[]; error?: { ename: string; evalue: string; traceback: string[] } | null; outputs?: NotebookOutput[]; execution_count?: number | null; mode?: "jupyter" | "basic" | null; python?: string; note?: string | null };

export function parseNotebook(text: string): Notebook | null {
  if (text.trim() === "") return { cells: [{ cell_type: "code", execution_count: null, metadata: {}, outputs: [], source: [] }], metadata: {}, nbformat: 4, nbformat_minor: 5 };
  try {
    const value = JSON.parse(text) as Notebook;
    return value && Array.isArray(value.cells) ? value : null;
  } catch {
    return null;
  }
}

export function cellSource(cell: NotebookCell): string {
  return Array.isArray(cell.source) ? cell.source.join("") : String(cell.source ?? "");
}

export function toSourceLines(text: string): string[] {
  if (text === "") return [];
  const lines = text.split("\n");
  return lines.map((line, index) => (index < lines.length - 1 ? `${line}\n` : line)).filter((line, index) => index < lines.length - 1 || line !== "");
}

export function serializeNotebook(notebook: Notebook): string {
  return `${JSON.stringify(notebook, null, 1)}\n`;
}

export function updateCell(notebook: Notebook, index: number, patch: Partial<NotebookCell>): Notebook {
  return { ...notebook, cells: notebook.cells.map((cell, position) => (position === index ? { ...cell, ...patch } as NotebookCell : cell)) };
}

export function insertCell(notebook: Notebook, index: number, type: "code" | "markdown"): Notebook {
  const cell: NotebookCell = type === "code"
    ? { cell_type: "code", execution_count: null, metadata: {}, outputs: [], source: [] }
    : { cell_type: "markdown", metadata: {}, source: [] };
  const cells = [...notebook.cells];
  cells.splice(index, 0, cell);
  return { ...notebook, cells };
}

export function removeCell(notebook: Notebook, index: number): Notebook {
  return { ...notebook, cells: notebook.cells.filter((_, position) => position !== index) };
}

export function moveCell(notebook: Notebook, index: number, offset: number): Notebook {
  const target = index + offset;
  if (target < 0 || target >= notebook.cells.length) return notebook;
  const cells = [...notebook.cells];
  [cells[index], cells[target]] = [cells[target], cells[index]];
  return { ...notebook, cells };
}

export function appendLiveOutput(outputs: NotebookOutput[], output: NotebookOutput): NotebookOutput[] {
  if (output.output_type === "clear_output") return [];
  const last = outputs[outputs.length - 1];
  if (output.output_type === "stream" && last?.output_type === "stream" && last.name === output.name) {
    return [...outputs.slice(0, -1), { ...last, text: toSourceLines(outputText(last.text) + outputText(output.text)) }];
  }
  return [...outputs, output.output_type === "stream" ? { ...output, text: toSourceLines(outputText(output.text)) } : output];
}

export function kernelOutputs(result: KernelResult, executionCount: number): NotebookOutput[] {
  if (result.outputs) return result.outputs.reduce<NotebookOutput[]>((merged, output) => appendLiveOutput(merged, output.output_type === "execute_result" ? { ...output, execution_count: executionCount } : output), []);
  const outputs: NotebookOutput[] = [];
  if (result.stdout) outputs.push({ output_type: "stream", name: "stdout", text: toSourceLines(result.stdout) });
  if (result.stderr) outputs.push({ output_type: "stream", name: "stderr", text: toSourceLines(result.stderr) });
  for (const image of result.images ?? []) outputs.push({ output_type: "display_data", data: { "image/png": image, "text/plain": ["<Figure>"] }, metadata: {} });
  if ((result.result !== null && result.result !== undefined) || result.html) {
    outputs.push({
      output_type: "execute_result",
      execution_count: executionCount,
      data: { ...(result.result !== null && result.result !== undefined ? { "text/plain": toSourceLines(result.result) } : {}), ...(result.html ? { "text/html": toSourceLines(result.html) } : {}) },
      metadata: {},
    });
  }
  if (result.error) outputs.push({ output_type: "error", ename: result.error.ename, evalue: result.error.evalue, traceback: result.error.traceback });
  return outputs;
}

export function outputText(value: unknown): string {
  return Array.isArray(value) ? value.join("") : typeof value === "string" ? value : "";
}

export type NotebookCommand = { insertAt: number } | { select: number } | { edit: true };

export function notebookCommandKey(key: string, index: number, count: number): NotebookCommand | null {
  if (key === "a" || key === "A") return { insertAt: index };
  if (key === "b" || key === "B") return { insertAt: index + 1 };
  if (key === "ArrowUp" || key === "k") return { select: Math.max(0, index - 1) };
  if (key === "ArrowDown" || key === "j") return { select: Math.min(count - 1, index + 1) };
  if (key === "Enter") return { edit: true };
  return null;
}
