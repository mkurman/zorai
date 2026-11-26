import { Suspense, lazy, useEffect, useRef, useState } from "react";
import { MarkdownContent } from "@/components/agent-chat-panel/chat-view/markdown";
import { getBridge } from "@/lib/bridge";
import { ZORAI_MONACO_THEME } from "@/lib/monacoGuiTheme";
import { cellSource, insertCell, kernelOutputs, moveCell, outputText, parseNotebook, removeCell, serializeNotebook, toSourceLines, updateCell, type Notebook, type NotebookOutput } from "./codeNotebookModel";

const MonacoEditor = lazy(() => import("@monaco-editor/react").then((module) => ({ default: module.default })));
const LINE_HEIGHT = 19;

function OutputView({ output }: { output: NotebookOutput }) {
  const data = (output.data ?? {}) as Record<string, unknown>;
  if (output.output_type === "stream") return <pre className={output.name === "stderr" ? "zorai-notebook__stream is-stderr" : "zorai-notebook__stream"}>{outputText(output.text)}</pre>;
  if (output.output_type === "error") return <pre className="zorai-notebook__stream is-stderr">{(Array.isArray(output.traceback) ? output.traceback.join("\n") : `${output.ename}: ${output.evalue}`).replace(/\u001b\[[0-9;]*m/g, "")}</pre>;
  if (typeof data["image/png"] === "string") return <img className="zorai-notebook__image" alt="Cell output" src={`data:image/png;base64,${String(data["image/png"]).replace(/\s/g, "")}`} />;
  if (typeof data["image/svg+xml"] === "string" || Array.isArray(data["image/svg+xml"])) return <img className="zorai-notebook__image" alt="Cell output" src={`data:image/svg+xml;utf8,${encodeURIComponent(outputText(data["image/svg+xml"]))}`} />;
  if (data["text/html"]) return <iframe className="zorai-notebook__html" title="HTML output" sandbox="" srcDoc={outputText(data["text/html"])} />;
  if (data["text/plain"]) return <pre className="zorai-notebook__stream">{outputText(data["text/plain"])}</pre>;
  return null;
}

export function CodeNotebookEditor({ path, content, root, onChange }: { path: string; content: string; root: string | null; onChange?: (content: string) => void }) {
  const [notebook, setNotebook] = useState<Notebook | null>(() => parseNotebook(content));
  const [editingMarkdown, setEditingMarkdown] = useState<number | null>(null);
  const [running, setRunning] = useState<Set<number>>(() => new Set());
  const [error, setError] = useState<string | null>(null);
  const lastWritten = useRef(content);
  const executionCounter = useRef(0);
  const language = String(((notebook?.metadata?.kernelspec as { language?: string } | undefined)?.language) ?? "python");

  useEffect(() => {
    if (content === lastWritten.current) return;
    lastWritten.current = content;
    setNotebook(parseNotebook(content));
  }, [content]);

  const commit = (next: Notebook) => {
    setNotebook(next);
    const text = serializeNotebook(next);
    lastWritten.current = text;
    onChange?.(text);
  };

  const run = async (index: number) => {
    const bridge = getBridge();
    if (!notebook || !root || !bridge?.notebookExecute) {
      setError("Running cells needs the desktop app and an open folder.");
      return;
    }
    setRunning((current) => new Set(current).add(index));
    setError(null);
    try {
      const result = await bridge.notebookExecute(root, path, cellSource(notebook.cells[index]));
      executionCounter.current += 1;
      setNotebook((current) => {
        if (!current) return current;
        const next = updateCell(current, index, { outputs: kernelOutputs(result, executionCounter.current), execution_count: executionCounter.current });
        const text = serializeNotebook(next);
        lastWritten.current = text;
        onChange?.(text);
        return next;
      });
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setRunning((current) => {
        const next = new Set(current);
        next.delete(index);
        return next;
      });
    }
  };

  const runAll = async () => {
    for (let index = 0; index < (notebook?.cells.length ?? 0); index += 1) if (notebook?.cells[index].cell_type === "code") await run(index);
  };

  const runRef = useRef(run);
  runRef.current = run;

  if (!notebook) return <div className="zorai-empty">This notebook is not valid nbformat JSON. Switch to Code view to fix it.</div>;
  return (
    <div className="zorai-notebook">
      <div className="zorai-notebook__toolbar">
        <button type="button" onClick={() => void runAll()}>▶ Run all</button>
        <button type="button" onClick={() => { if (root) void getBridge()?.notebookRestart?.(root, path); executionCounter.current = 0; }}>Restart kernel</button>
        <button type="button" onClick={() => commit({ ...notebook, cells: notebook.cells.map((cell) => (cell.cell_type === "code" ? { ...cell, outputs: [], execution_count: null } : cell)) })}>Clear outputs</button>
        <span>{language} · {notebook.cells.length} cells</span>
      </div>
      {error ? <p className="zorai-notebook__error">{error}</p> : null}
      {notebook.cells.map((cell, index) => {
        const source = cellSource(cell);
        return (
          <div key={index} className={`zorai-notebook__cell is-${cell.cell_type}`}>
            <div className="zorai-notebook__gutter">
              {cell.cell_type === "code" ? (
                <button type="button" title="Run cell (Shift+Enter)" disabled={running.has(index)} onClick={() => void run(index)}>{running.has(index) ? "…" : "▶"}</button>
              ) : null}
              <span>{cell.cell_type === "code" ? `[${cell.execution_count ?? " "}]` : "md"}</span>
            </div>
            <div className="zorai-notebook__body">
              {cell.cell_type === "markdown" && editingMarkdown !== index ? (
                <div className="zorai-notebook__markdown" onDoubleClick={() => setEditingMarkdown(index)} title="Double-click to edit">
                  {source.trim() ? <MarkdownContent content={source} filePreview /> : <em>Empty Markdown cell — double-click to edit</em>}
                </div>
              ) : cell.cell_type === "markdown" ? (
                <textarea
                  className="zorai-notebook__textarea"
                  autoFocus
                  value={source}
                  rows={Math.max(2, source.split("\n").length)}
                  onChange={(event) => commit(updateCell(notebook, index, { source: toSourceLines(event.target.value) }))}
                  onBlur={() => setEditingMarkdown(null)}
                  onKeyDown={(event) => { if (event.key === "Enter" && event.shiftKey) { event.preventDefault(); setEditingMarkdown(null); } }}
                />
              ) : (
                <Suspense fallback={<pre className="zorai-notebook__stream">{source}</pre>}>
                  <div style={{ height: Math.max(2, source.split("\n").length) * LINE_HEIGHT + 10 }}>
                    <MonacoEditor
                      value={source}
                      language={cell.cell_type === "code" ? language : "plaintext"}
                      theme={ZORAI_MONACO_THEME}
                      onChange={(value) => commit(updateCell(notebook, index, { source: toSourceLines(value ?? "") }))}
                      onMount={(editor, monacoApi) => {
                        editor.addCommand(monacoApi.KeyMod.Shift | monacoApi.KeyCode.Enter, () => void runRef.current(index));
                      }}
                      options={{ minimap: { enabled: false }, scrollBeyondLastLine: false, lineNumbers: "off", glyphMargin: false, folding: false, lineHeight: LINE_HEIGHT, automaticLayout: true, scrollbar: { vertical: "hidden", alwaysConsumeMouseWheel: false }, overviewRulerLanes: 0, renderLineHighlight: "none" }}
                    />
                  </div>
                </Suspense>
              )}
              {(cell.outputs ?? []).length > 0 ? <div className="zorai-notebook__outputs">{(cell.outputs ?? []).map((output, position) => <OutputView key={position} output={output} />)}</div> : null}
            </div>
            <div className="zorai-notebook__actions">
              <button type="button" title="Move up" onClick={() => commit(moveCell(notebook, index, -1))}>↑</button>
              <button type="button" title="Move down" onClick={() => commit(moveCell(notebook, index, 1))}>↓</button>
              <button type="button" title="Add code cell below" onClick={() => commit(insertCell(notebook, index + 1, "code"))}>+ Code</button>
              <button type="button" title="Add Markdown cell below" onClick={() => commit(insertCell(notebook, index + 1, "markdown"))}>+ Md</button>
              <button type="button" title="Delete cell" onClick={() => commit(removeCell(notebook, index))}>×</button>
            </div>
          </div>
        );
      })}
      <div className="zorai-notebook__add">
        <button type="button" onClick={() => commit(insertCell(notebook, notebook.cells.length, "code"))}>+ Code</button>
        <button type="button" onClick={() => commit(insertCell(notebook, notebook.cells.length, "markdown"))}>+ Markdown</button>
      </div>
    </div>
  );
}
