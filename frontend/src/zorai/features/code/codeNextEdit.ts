import type { editor as MonacoEditorApi, IDisposable } from "monaco-editor";
import { getBridge } from "@/lib/bridge";
import { createInlineDiffSession, type InlineDiffSession } from "./codeInlineEdit";
import { inlineCompletionsEnabled } from "./codeInlineCompletions";

type MonacoApi = typeof import("monaco-editor");

const DEBOUNCE_MS = 700;
const WINDOW_LINES = 60;
const MAX_EDITS = 5;
let sequence = 0;

export type LineEdit = { line: number; text: string };

export function rememberLineEdit(edits: LineEdit[], edit: LineEdit): LineEdit[] {
  const last = edits[edits.length - 1];
  const next = last && last.line === edit.line ? [...edits.slice(0, -1), edit] : [...edits, edit];
  return next.slice(-MAX_EDITS);
}

export function describeLineEdits(edits: LineEdit[], windowStart: number): string[] {
  return edits
    .filter((edit) => edit.line >= windowStart)
    .map((edit) => `line ${edit.line - windowStart + 1} is now: ${edit.text.trim()}`);
}

export function parseNextEdit(raw: string, windowStart: number): { line: number; endLine: number; replacement: string } | null {
  if (!raw.trim()) return null;
  try {
    const value = JSON.parse(raw) as { line?: unknown; end_line?: unknown; replacement?: unknown };
    if (typeof value.line !== "number" || typeof value.replacement !== "string") return null;
    const endLine = typeof value.end_line === "number" && value.end_line >= value.line ? value.end_line : value.line;
    return { line: windowStart + value.line - 1, endLine: windowStart + endLine - 1, replacement: value.replacement };
  } catch {
    return null;
  }
}

export function attachNextEditPrediction(editor: MonacoEditorApi.IStandaloneCodeEditor, monaco: MonacoApi, getPath: () => string): IDisposable {
  let edits: LineEdit[] = [];
  let timer = 0;
  let applying = false;
  let prediction: { line: number; endLine: number; replacement: string; original: string } | null = null;
  let preview: InlineDiffSession | null = null;
  const hasPrediction = editor.createContextKey<boolean>("zoraiNextEdit", false);
  const previewing = editor.createContextKey<boolean>("zoraiNextEditPreview", false);
  const decorations = editor.createDecorationsCollection([]);

  const clear = () => {
    prediction = null;
    decorations.clear();
    hasPrediction.set(false);
  };
  const endPreview = () => {
    preview = null;
    previewing.set(false);
  };

  const request = async () => {
    const model = editor.getModel();
    const position = editor.getPosition();
    const bridge = getBridge();
    if (!model || !position || !bridge?.agentCodeComplete || !inlineCompletionsEnabled() || edits.length === 0) return;
    const version = model.getAlternativeVersionId();
    const start = Math.max(1, position.lineNumber - WINDOW_LINES);
    const end = Math.min(model.getLineCount(), position.lineNumber + WINDOW_LINES);
    const result = await bridge.agentCodeComplete({
      requestId: `next-edit-${Date.now()}-${++sequence}`,
      kind: "next_edit",
      path: model.uri.path.replace(/^\/+/, "") || getPath(),
      language: model.getLanguageId(),
      prefix: model.getValueInRange(new monaco.Range(start, 1, position.lineNumber, position.column)),
      suffix: model.getValueInRange(new monaco.Range(position.lineNumber, position.column, end, model.getLineMaxColumn(end))),
      recentEdits: describeLineEdits(edits, start),
    }).catch(() => null);
    if (!result || result.error || editor.getModel() !== model || model.getAlternativeVersionId() !== version) return;
    const next = parseNextEdit(result.completion, start);
    if (!next || next.line < 1 || next.endLine > model.getLineCount() || (position.lineNumber >= next.line && position.lineNumber <= next.endLine)) return;
    const original = model.getValueInRange(new monaco.Range(next.line, 1, next.endLine, model.getLineMaxColumn(next.endLine)));
    if (original === next.replacement) return;
    prediction = { ...next, original };
    hasPrediction.set(true);
    decorations.set([{
      range: new monaco.Range(next.line, 1, next.endLine, model.getLineMaxColumn(next.endLine)),
      options: {
        isWholeLine: true,
        className: "zorai-next-edit-target",
        after: { content: "  ⇥ Tab: next edit", inlineClassName: "zorai-next-edit-hint" },
      },
    }]);
  };

  const contentListener = editor.onDidChangeModelContent((event) => {
    if (applying || event.isFlush) return;
    const model = editor.getModel();
    const change = event.changes[0];
    if (!model || !change) return;
    const line = Math.min(change.range.startLineNumber, model.getLineCount());
    edits = rememberLineEdit(edits, { line, text: model.getLineContent(line) });
    clear();
    window.clearTimeout(timer);
    timer = window.setTimeout(() => void request(), DEBOUNCE_MS);
  });
  const modelListener = editor.onDidChangeModel(() => {
    edits = [];
    window.clearTimeout(timer);
    clear();
    endPreview();
  });

  editor.addCommand(monaco.KeyCode.Tab, () => {
    if (preview) {
      const range = preview.accept();
      endPreview();
      editor.setPosition({ lineNumber: range.endLine, column: editor.getModel()?.getLineMaxColumn(range.endLine) ?? 1 });
      return;
    }
    const target = prediction;
    if (!target) return;
    clear();
    editor.setPosition({ lineNumber: target.line, column: 1 });
    editor.revealLineInCenterIfOutsideViewport(target.line);
    applying = true;
    preview = createInlineDiffSession(editor, monaco, target.line, target.original);
    preview.show(target.replacement);
    applying = false;
    previewing.set(true);
  }, "(zoraiNextEdit || zoraiNextEditPreview) && !inlineSuggestionVisible && !suggestWidgetVisible");

  editor.addCommand(monaco.KeyCode.Escape, () => {
    if (preview) {
      applying = true;
      preview.reject();
      applying = false;
      endPreview();
    }
    clear();
  }, "zoraiNextEdit || zoraiNextEditPreview");

  return {
    dispose() {
      window.clearTimeout(timer);
      contentListener.dispose();
      modelListener.dispose();
      decorations.clear();
    },
  };
}
