import type { editor as MonacoEditorApi, IDisposable } from "monaco-editor";

type MonacoApi = typeof import("monaco-editor");

export type MergeConflict = { start: number; base: number | null; separator: number; end: number };
export type ConflictChoice = "current" | "incoming" | "both";

const ACCEPT_COMMAND = "zorai.merge.accept";
const RESOLVE_COMMAND = "zorai.merge.resolveWithRarog";
let registered = false;
const resolvers = new Map<string, (conflict: MergeConflict) => void>();

export function findMergeConflicts(text: string): MergeConflict[] {
  const lines = text.split("\n");
  const conflicts: MergeConflict[] = [];
  let open: Partial<MergeConflict> | null = null;
  lines.forEach((raw, index) => {
    const line = raw.replace(/\r$/, "");
    const number = index + 1;
    if (line.startsWith("<<<<<<<")) open = { start: number, base: null };
    else if (open && line.startsWith("|||||||") && open.separator === undefined) open.base = number;
    else if (open && line.startsWith("=======") && open.separator === undefined) open.separator = number;
    else if (open && line.startsWith(">>>>>>>") && open.separator !== undefined) {
      conflicts.push({ start: open.start!, base: open.base ?? null, separator: open.separator, end: number });
      open = null;
    }
  });
  return conflicts;
}

export function resolveConflictText(lines: string[], conflict: MergeConflict, choice: ConflictChoice): string {
  const current = lines.slice(conflict.start, (conflict.base ?? conflict.separator) - 1);
  const incoming = lines.slice(conflict.separator, conflict.end - 1);
  const kept = choice === "current" ? current : choice === "incoming" ? incoming : [...current, ...incoming];
  return kept.join("\n");
}

function mayHaveConflicts(model: MonacoEditorApi.ITextModel): boolean {
  return model.findNextMatch("<<<<<<<", { lineNumber: 1, column: 1 }, false, true, null, false) !== null;
}

function applyChoice(monaco: MonacoApi, model: MonacoEditorApi.ITextModel, conflict: MergeConflict, choice: ConflictChoice) {
  const lines = model.getLinesContent();
  const text = resolveConflictText(lines, conflict, choice);
  const range = text === "" && conflict.end < model.getLineCount()
    ? new monaco.Range(conflict.start, 1, conflict.end + 1, 1)
    : new monaco.Range(conflict.start, 1, conflict.end, model.getLineMaxColumn(conflict.end));
  model.pushStackElement();
  model.pushEditOperations([], [{ range, text, forceMoveMarkers: true }], () => null);
  model.pushStackElement();
}

function ensureRegistered(monaco: MonacoApi) {
  if (registered) return;
  registered = true;
  monaco.editor.registerCommand(ACCEPT_COMMAND, (_accessor, uri: string, conflict: MergeConflict, choice: ConflictChoice) => {
    const model = monaco.editor.getModel(monaco.Uri.parse(uri));
    if (model) applyChoice(monaco, model, conflict, choice);
  });
  monaco.editor.registerCommand(RESOLVE_COMMAND, (_accessor, uri: string, conflict: MergeConflict) => {
    resolvers.get(uri)?.(conflict);
  });
  monaco.languages.registerCodeLensProvider("*", {
    provideCodeLenses: (model) => {
      const uri = model.uri.toString();
      if (!mayHaveConflicts(model)) return { lenses: [], dispose: () => {} };
      const lenses = findMergeConflicts(model.getValue()).flatMap((conflict) => {
        const range = new monaco.Range(conflict.start, 1, conflict.start, 1);
        const lens = (id: string, title: string, args: unknown[]) => ({ range, command: { id, title, arguments: [uri, conflict, ...args] } });
        return [
          lens(ACCEPT_COMMAND, "Accept Current", ["current"]),
          lens(ACCEPT_COMMAND, "Accept Incoming", ["incoming"]),
          lens(ACCEPT_COMMAND, "Accept Both", ["both"]),
          ...(resolvers.has(uri) ? [lens(RESOLVE_COMMAND, "Resolve with Rarog", [])] : []),
        ];
      });
      return { lenses, dispose: () => {} };
    },
  });
}

export function attachMergeConflicts(editor: MonacoEditorApi.IStandaloneCodeEditor, monaco: MonacoApi): IDisposable {
  ensureRegistered(monaco);
  const decorations = editor.createDecorationsCollection([]);
  let timer = 0;
  let resolverUri: string | null = null;
  const bindResolver = () => {
    if (resolverUri) resolvers.delete(resolverUri);
    const model = editor.getModel();
    resolverUri = model?.uri.toString() ?? null;
    if (!resolverUri) return;
    resolvers.set(resolverUri, (conflict) => {
      const current = editor.getModel();
      if (!current) return;
      editor.setSelection(new monaco.Range(conflict.start, 1, conflict.end, current.getLineMaxColumn(conflict.end)));
      editor.focus();
      void editor.getAction("zorai.rarog.fix")?.run();
    });
  };
  const render = () => {
    const model = editor.getModel();
    const conflicts = model && mayHaveConflicts(model) ? findMergeConflicts(model.getValue()) : [];
    decorations.set(conflicts.flatMap((conflict) => {
      const block = (startLine: number, endLine: number, className: string) => (endLine >= startLine ? [{ range: new monaco.Range(startLine, 1, endLine, 1), options: { isWholeLine: true, className } }] : []);
      return [
        ...block(conflict.start, conflict.start, "zorai-merge-marker zorai-merge-marker--current"),
        ...block(conflict.start + 1, (conflict.base ?? conflict.separator) - 1, "zorai-merge-current"),
        ...(conflict.base ? block(conflict.base, conflict.separator - 1, "zorai-merge-base") : []),
        ...block(conflict.separator, conflict.separator, "zorai-merge-marker"),
        ...block(conflict.separator + 1, conflict.end - 1, "zorai-merge-incoming"),
        ...block(conflict.end, conflict.end, "zorai-merge-marker zorai-merge-marker--incoming"),
      ];
    }));
  };
  const listeners = [
    editor.onDidChangeModelContent(() => {
      window.clearTimeout(timer);
      timer = window.setTimeout(render, 200);
    }),
    editor.onDidChangeModel(() => {
      bindResolver();
      render();
    }),
  ];
  bindResolver();
  render();
  return {
    dispose() {
      window.clearTimeout(timer);
      if (resolverUri) resolvers.delete(resolverUri);
      listeners.forEach((listener) => listener.dispose());
      decorations.clear();
    },
  };
}
