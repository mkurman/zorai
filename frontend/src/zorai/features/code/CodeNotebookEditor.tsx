import { Suspense, lazy, useEffect, useMemo, useRef, useState } from "react";
import { MarkdownContent } from "@/components/agent-chat-panel/chat-view/markdown";
import { getBridge } from "@/lib/bridge";
import { applyMonacoGuiTheme, subscribeMonacoGuiTheme, ZORAI_MONACO_THEME } from "@/lib/monacoGuiTheme";
import { appendLiveOutput, cellSource, insertCell, kernelOutputs, notebookCommandKey, moveCell, outputText, parseNotebook, removeCell, serializeNotebook, toSourceLines, updateCell, type Notebook, type NotebookOutput } from "./codeNotebookModel";

import { NotebookKernelPicker, type NotebookKernelInfo } from "./NotebookKernelPicker";
import { useNotebookInterpreterStore } from "./notebookInterpreterStore";
import { registerNotebookCell } from "./notebookCompletion";
import { createNotebookLspSession, mapShadowDiagnostics, notebookShadowPath } from "./notebookLsp";
import { useCodeEditorSettingsStore } from "./codeEditorSettingsStore";
import type { Monaco } from "@monaco-editor/react";
import type { editor as MonacoEditorApi } from "monaco-editor";
import { NotebookHtmlOutput } from "./notebookHtmlOutput";
import { revealScrollDelta } from "./WorkspaceExplorerTree";

const MonacoEditor = lazy(() => import("@monaco-editor/react").then((module) => ({ default: module.default })));
const LINE_HEIGHT = 19;

function OutputView({ output }: { output: NotebookOutput }) {
  const data = (output.data ?? {}) as Record<string, unknown>;
  if (output.output_type === "stream") return <pre className={output.name === "stderr" ? "zorai-notebook__stream is-stderr" : "zorai-notebook__stream"}>{outputText(output.text)}</pre>;
  if (output.output_type === "error") return <pre className="zorai-notebook__stream is-stderr">{(Array.isArray(output.traceback) ? output.traceback.join("\n") : `${output.ename}: ${output.evalue}`).replace(/\u001b\[[0-9;]*m/g, "")}</pre>;
  if (typeof data["image/jpeg"] === "string") return <img className="zorai-notebook__image" alt="Cell output" src={`data:image/jpeg;base64,${String(data["image/jpeg"]).replace(/\s/g, "")}`} />;
  if (typeof data["image/png"] === "string") return <img className="zorai-notebook__image" alt="Cell output" src={`data:image/png;base64,${String(data["image/png"]).replace(/\s/g, "")}`} />;
  if (typeof data["image/svg+xml"] === "string" || Array.isArray(data["image/svg+xml"])) return <img className="zorai-notebook__image" alt="Cell output" src={`data:image/svg+xml;utf8,${encodeURIComponent(outputText(data["image/svg+xml"]))}`} />;
  if (data["text/html"]) return <NotebookHtmlOutput html={outputText(data["text/html"])} />;
  if (data["text/plain"]) return <pre className="zorai-notebook__stream">{outputText(data["text/plain"])}</pre>;
  return null;
}

export function CodeNotebookEditor({ path, content, root, onChange }: { path: string; content: string; root: string | null; onChange?: (content: string) => void }) {
  const [notebook, setNotebook] = useState<Notebook | null>(() => parseNotebook(content));
  const [editingMarkdown, setEditingMarkdown] = useState<number | null>(null);
  const [running, setRunning] = useState<Set<number>>(() => new Set());
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<number | null>(null);
  const cellRefs = useRef<Array<HTMLDivElement | null>>([]);
  const editorRefs = useRef<Array<MonacoEditorApi.IStandaloneCodeEditor | null>>([]);
  const monacoRef = useRef<Monaco | null>(null);
  const notebookRef = useRef(notebook);
  notebookRef.current = notebook;
  const lspEnabled = useCodeEditorSettingsStore((state) => state.settings.lspEnabled);
  const lspSession = useMemo(() => (root && lspEnabled ? createNotebookLspSession(root, path, () => (notebookRef.current?.cells ?? []).map((cell, index) => ({
    code: cell.cell_type === "code",
    source: editorRefs.current[index]?.getModel()?.getValue() ?? cellSource(cell),
  }))) : null), [lspEnabled, path, root]);
  useEffect(() => () => lspSession?.close(), [lspSession]);
  useEffect(() => {
    if (!lspSession) return;
    const timer = window.setTimeout(() => void lspSession.sync(), 400);
    return () => window.clearTimeout(timer);
  }, [lspSession, notebook]);
  useEffect(() => {
    const subscribe = getBridge()?.onWorkspaceLspDiagnostics;
    if (!lspSession || !subscribe) return;
    const shadowPath = notebookShadowPath(path);
    const unsubscribe = subscribe((payload) => {
      if (payload.root.replace(/[\\/]+$/, "") !== root?.replace(/[\\/]+$/, "") || payload.path.replace(/\\/g, "/") !== shadowPath) return;
      const shadow = lspSession.current();
      const monacoApi = monacoRef.current;
      if (!shadow || !monacoApi) return;
      const byCell = new Map<number, MonacoEditorApi.IMarkerData[]>();
      for (const item of mapShadowDiagnostics(shadow, payload.diagnostics)) {
        const severity = item.severity === 1 ? monacoApi.MarkerSeverity.Error : item.severity === 2 ? monacoApi.MarkerSeverity.Warning : item.severity === 3 ? monacoApi.MarkerSeverity.Info : monacoApi.MarkerSeverity.Hint;
        byCell.set(item.cell, [...(byCell.get(item.cell) ?? []), { severity, message: item.message, source: item.source ?? undefined, startLineNumber: item.startLine, startColumn: item.startColumn, endLineNumber: item.endLine, endColumn: item.endColumn }]);
      }
      editorRefs.current.forEach((editor, index) => {
        const model = editor?.getModel();
        if (model) monacoApi.editor.setModelMarkers(model, "zorai-notebook-lsp", byCell.get(index) ?? []);
      });
    });
    return () => { if (typeof unsubscribe === "function") unsubscribe(); };
  }, [lspSession, path, root]);
  const pendingSelect = useRef<number | null>(null);
  const revealCell = (index: number) => {
    const cell = cellRefs.current[index];
    const scroller = cell?.closest<HTMLElement>(".zorai-notebook");
    if (!cell || !scroller) return;
    const toolbar = scroller.querySelector<HTMLElement>(".zorai-notebook__toolbar")?.getBoundingClientRect().height ?? 0;
    const view = scroller.getBoundingClientRect();
    scroller.scrollTop += revealScrollDelta(cell.getBoundingClientRect(), { top: view.top + toolbar, bottom: view.bottom });
  };
  const focusCell = (index: number) => {
    const cell = cellRefs.current[index];
    if (!cell) return;
    setSelected(index);
    cell.focus({ preventScroll: true });
    revealCell(index);
  };
  useEffect(() => {
    if (pendingSelect.current === null) return;
    const index = pendingSelect.current;
    pendingSelect.current = null;
    focusCell(index);
  });
  const lastWritten = useRef(content);
  const executionCounter = useRef(0);
  const python = useNotebookInterpreterStore((state) => (root ? state.byRoot[root] ?? null : null));
  const [kernelInfo, setKernelInfo] = useState<NotebookKernelInfo | null>(null);
  useEffect(() => { setKernelInfo(null); }, [python]);
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
    const runId = `${path}\0${index}\0${Date.now()}`;
    setNotebook((current) => (current ? updateCell(current, index, { outputs: [] }) : current));
    const unsubscribe = bridge.onNotebookOutput?.((payload) => {
      if (payload.runId !== runId) return;
      setNotebook((current) => (current ? updateCell(current, index, { outputs: appendLiveOutput(current.cells[index]?.outputs ?? [], payload.output) }) : current));
    });
    try {
      const result = await bridge.notebookExecute(root, path, cellSource(notebook.cells[index]), python, runId);
      executionCounter.current = result.execution_count ?? executionCounter.current + 1;
      setKernelInfo({ mode: result.mode ?? null, python: result.python ?? "", note: result.note ?? null });
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
      unsubscribe?.();
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
  const pendingEdit = useRef<number | null>(null);
  const runAndAdvance = (index: number) => {
    if (!notebook) return;
    if (notebook.cells[index]?.cell_type === "code") void run(index);
    const next = index + 1;
    if (next >= notebook.cells.length) {
      pendingEdit.current = next;
      commit(insertCell(notebook, next, "code"));
    } else if (notebook.cells[next].cell_type === "code" && editorRefs.current[next]) {
      setSelected(next);
      editorRefs.current[next]?.focus();
      revealCell(next);
    } else {
      focusCell(next);
    }
  };
  const runAndAdvanceRef = useRef(runAndAdvance);
  runAndAdvanceRef.current = runAndAdvance;

  if (!notebook) return <div className="zorai-empty">This notebook is not valid nbformat JSON. Switch to Code view to fix it.</div>;
  return (
    <div className="zorai-notebook">
      <div className="zorai-notebook__toolbar">
        <button type="button" onClick={() => void runAll()}>▶ Run all</button>
        {running.size > 0 ? <button type="button" onClick={() => { if (root) void getBridge()?.notebookInterrupt?.(root, path); }}>■ Interrupt</button> : null}
        <button type="button" onClick={() => { if (root) void getBridge()?.notebookRestart?.(root, path); executionCounter.current = 0; setKernelInfo(null); }}>Restart kernel</button>
        <button type="button" onClick={() => commit({ ...notebook, cells: notebook.cells.map((cell) => (cell.cell_type === "code" ? { ...cell, outputs: [], execution_count: null } : cell)) })}>Clear outputs</button>
        <span>{language} · {notebook.cells.length} cells</span>
        <NotebookKernelPicker root={root} path={path} info={kernelInfo} />
      </div>
      {error ? <p className="zorai-notebook__error">{error}</p> : null}
      {notebook.cells.map((cell, index) => {
        const source = cellSource(cell);
        return (
          <div
            key={index}
            ref={(element) => { cellRefs.current[index] = element; }}
            tabIndex={-1}
            className={`zorai-notebook__cell is-${cell.cell_type}${selected === index ? " is-selected" : ""}`}
            onMouseDown={() => setSelected(index)}
            onKeyDown={(event) => {
              if (event.target !== event.currentTarget || event.ctrlKey || event.metaKey || event.altKey) return;
              if (event.key === "Enter" && event.shiftKey) { event.preventDefault(); runAndAdvance(index); return; }
              const command = notebookCommandKey(event.key, index, notebook.cells.length);
              if (!command) return;
              event.preventDefault();
              if ("insertAt" in command) {
                commit(insertCell(notebook, command.insertAt, "code"));
                pendingSelect.current = command.insertAt;
              } else if ("select" in command) {
                focusCell(command.select);
              } else if (cell.cell_type === "markdown") {
                setEditingMarkdown(index);
              } else {
                editorRefs.current[index]?.focus();
              }
            }}
          >
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
                  onKeyDown={(event) => {
                    if (event.key === "Enter" && event.shiftKey) { event.preventDefault(); setEditingMarkdown(null); }
                    if (event.key === "Escape") { event.preventDefault(); setEditingMarkdown(null); focusCell(index); }
                  }}
                />
              ) : (
                <Suspense fallback={<pre className="zorai-notebook__stream">{source}</pre>}>
                  <div style={{ height: Math.max(2, source.split("\n").length) * LINE_HEIGHT + 14 }}>
                    <MonacoEditor
                      value={source}
                      language={cell.cell_type === "code" ? language : "plaintext"}
                      theme={ZORAI_MONACO_THEME}
                      beforeMount={(monacoApi) => { applyMonacoGuiTheme(monacoApi); }}
                      onChange={(value) => commit(updateCell(notebook, index, { source: toSourceLines(value ?? "") }))}
                      onMount={(editor, monacoApi) => {
                        editor.addAction({ id: "zorai.notebook.runCell", label: "Run Cell", keybindings: [monacoApi.KeyMod.Shift | monacoApi.KeyCode.Enter], run: () => runAndAdvanceRef.current(index) });
                        editor.onDidDispose(subscribeMonacoGuiTheme(monacoApi));
                        editorRefs.current[index] = editor;
                        monacoRef.current = monacoApi;
                        if (pendingEdit.current === index) { pendingEdit.current = null; setSelected(index); editor.focus(); requestAnimationFrame(() => revealCell(index)); }
                        editor.onDidDispose(() => { if (editorRefs.current[index] === editor) editorRefs.current[index] = null; });
                        editor.addAction({ id: "zorai.notebook.commandMode", label: "Notebook: Command Mode", keybindings: [monacoApi.KeyCode.Escape], precondition: "!suggestWidgetVisible && !findWidgetVisible && !parameterHintsVisible && !renameInputVisible", run: () => focusCell(index) });
                        const model = editor.getModel();
                        if (root && model && cell.cell_type === "code") {
                          const unregister = registerNotebookCell(monacoApi, model.uri.toString(), language, { root, path, python: () => useNotebookInterpreterStore.getState().byRoot[root] ?? null, cell: index, lsp: lspSession });
                          editor.onDidDispose(unregister);
                        }
                      }}
                      options={{ fixedOverflowWidgets: true, padding: { top: 6, bottom: 6 }, minimap: { enabled: false }, scrollBeyondLastLine: false, lineNumbers: "off", glyphMargin: false, folding: false, lineHeight: LINE_HEIGHT, automaticLayout: true, scrollbar: { vertical: "hidden", alwaysConsumeMouseWheel: false }, overviewRulerLanes: 0, renderLineHighlight: "none" }}
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
              <button type="button" className="zorai-notebook__delete" title="Delete cell" aria-label="Delete cell" onClick={() => commit(removeCell(notebook, index))}><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M5 7h14M10 7V4h4v3M7 7l1 13h8l1-13" /></svg></button>
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
