import type { editor as MonacoEditorApi, IDisposable } from "monaco-editor";
import { computeLineHunks, type LineHunk } from "./codeInlineEdit";

type MonacoApi = typeof import("monaco-editor");

export type GutterChange = { kind: "added" | "modified" | "deleted"; startLine: number; endLine: number; hunk: LineHunk };
export type GutterEdit = { startLine: number; startColumn: number; endLine: number; endColumn: number; text: string };
export type GitGutterOptions = {
  loadBase: () => Promise<string | null>;
  onStage?: (change: GutterChange) => Promise<void>;
};

const RECOMPUTE_MS = 400;
const MAX_GUTTER_LINES = 20000;
const MAX_PEEK_LINES = 12;

export function gutterChanges(base: string, current: string): GutterChange[] {
  return computeLineHunks(base, current).map((hunk) => {
    if (hunk.added.length === 0) {
      const line = Math.max(1, hunk.start);
      return { kind: "deleted", startLine: line, endLine: line, hunk };
    }
    return { kind: hunk.removed.length === 0 ? "added" : "modified", startLine: hunk.start + 1, endLine: hunk.start + hunk.added.length, hunk };
  });
}

export function revertGutterEdit(change: GutterChange, lineCount: number, maxColumn: (line: number) => number): GutterEdit {
  const { hunk } = change;
  const first = hunk.start + 1;
  const last = hunk.start + hunk.added.length;
  if (hunk.added.length > 0 && hunk.removed.length > 0) {
    return { startLine: first, startColumn: 1, endLine: last, endColumn: maxColumn(last), text: hunk.removed.join("\n") };
  }
  if (hunk.added.length > 0) {
    if (last < lineCount) return { startLine: first, startColumn: 1, endLine: last + 1, endColumn: 1, text: "" };
    if (first > 1) return { startLine: first - 1, startColumn: maxColumn(first - 1), endLine: last, endColumn: maxColumn(last), text: "" };
    return { startLine: 1, startColumn: 1, endLine: last, endColumn: maxColumn(last), text: "" };
  }
  if (first <= lineCount) return { startLine: first, startColumn: 1, endLine: first, endColumn: 1, text: `${hunk.removed.join("\n")}\n` };
  return { startLine: lineCount, startColumn: maxColumn(lineCount), endLine: lineCount, endColumn: maxColumn(lineCount), text: `\n${hunk.removed.join("\n")}` };
}

export function attachGitGutter(editor: MonacoEditorApi.IStandaloneCodeEditor, monaco: MonacoApi, options: () => GitGutterOptions | null): IDisposable & { refresh: () => void } {
  let base: string | null = null;
  let changes: GutterChange[] = [];
  let timer = 0;
  let generation = 0;
  let peek: { zoneId: string; change: GutterChange; handlers: Map<string, () => void> } | null = null;
  const decorations = editor.createDecorationsCollection([]);

  const closePeek = () => {
    if (!peek) return;
    const zoneId = peek.zoneId;
    editor.changeViewZones((accessor) => accessor.removeZone(zoneId));
    peek = null;
  };
  const render = () => {
    decorations.set(changes.map((change) => ({
      range: new monaco.Range(change.startLine, 1, change.endLine, 1),
      options: {
        isWholeLine: true,
        linesDecorationsClassName: `zorai-git-gutter zorai-git-gutter--${change.kind}`,
        overviewRuler: { color: change.kind === "added" ? "#3fb95080" : change.kind === "deleted" ? "#f8514980" : "#58a6ff80", position: monaco.editor.OverviewRulerLane.Left },
      },
    })));
  };
  const recompute = () => {
    const model = editor.getModel();
    if (!model || base === null || model.getLineCount() > MAX_GUTTER_LINES) {
      changes = [];
      render();
      return;
    }
    const current = model.getValue();
    changes = base === "" && current !== "" ? [] : gutterChanges(base, current);
    if (peek && !changes.some((change) => change.startLine === peek!.change.startLine)) closePeek();
    render();
  };
  const refresh = () => {
    const current = options();
    const requested = ++generation;
    if (!current) {
      base = null;
      recompute();
      return;
    }
    void current.loadBase().catch(() => null).then((next) => {
      if (requested !== generation) return;
      base = next;
      recompute();
    });
  };
  const revert = (change: GutterChange) => {
    const model = editor.getModel();
    if (!model) return;
    const edit = revertGutterEdit(change, model.getLineCount(), (line) => model.getLineMaxColumn(line));
    closePeek();
    editor.pushUndoStop();
    editor.executeEdits("zorai.git.revertHunk", [{ range: new monaco.Range(edit.startLine, edit.startColumn, edit.endLine, edit.endColumn), text: edit.text, forceMoveMarkers: true }]);
    editor.pushUndoStop();
  };
  const jump = (direction: 1 | -1) => {
    if (changes.length === 0) return;
    const line = editor.getPosition()?.lineNumber ?? 1;
    const ordered = direction === 1 ? changes : [...changes].reverse();
    const target = ordered.find((change) => (direction === 1 ? change.startLine > line : change.endLine < line)) ?? ordered[0];
    editor.setPosition({ lineNumber: target.startLine, column: 1 });
    editor.revealLineInCenterIfOutsideViewport(target.startLine);
    openPeek(target);
  };
  const openPeek = (change: GutterChange) => {
    closePeek();
    const node = document.createElement("div");
    node.className = "zorai-git-peek";
    const toolbar = document.createElement("div");
    toolbar.className = "zorai-git-peek__toolbar";
    const label = document.createElement("span");
    label.textContent = change.kind === "added" ? `${change.hunk.added.length} line(s) added` : change.kind === "deleted" ? `${change.hunk.removed.length} line(s) removed` : `${change.hunk.removed.length} line(s) changed`;
    toolbar.append(label);
    const actions: Array<[string, string, () => void]> = [
      ["revert", "Revert", () => revert(change)],
      ...(options()?.onStage ? [["stage", "Stage", () => { closePeek(); void options()?.onStage?.(change).then(refresh); }] as [string, string, () => void]] : []),
      ["previous", "↑", () => jump(-1)],
      ["next", "↓", () => jump(1)],
      ["close", "×", closePeek],
    ];
    for (const [action, text] of actions) {
      const button = document.createElement("button");
      button.type = "button";
      button.dataset.gitPeekAction = action;
      button.textContent = text;
      toolbar.append(button);
    }
    node.append(toolbar);
    if (change.hunk.removed.length > 0) {
      const removed = document.createElement("pre");
      removed.className = "zorai-git-peek__removed";
      removed.textContent = change.hunk.removed.join("\n");
      node.append(removed);
    }
    const handlers = new Map(actions.map(([action, , run]) => [action, run]));
    const lines = 1 + Math.min(MAX_PEEK_LINES, change.hunk.removed.length);
    editor.changeViewZones((accessor) => {
      const zoneId = accessor.addZone({ afterLineNumber: change.kind === "deleted" ? change.startLine : change.endLine, heightInLines: lines + 0.5, domNode: node });
      peek = { zoneId, change, handlers };
    });
  };

  const listeners = [
    editor.onDidChangeModelContent(() => {
      window.clearTimeout(timer);
      timer = window.setTimeout(recompute, RECOMPUTE_MS);
    }),
    editor.onDidChangeModel(() => {
      closePeek();
      refresh();
    }),
    editor.onMouseDown((event) => {
      const action = event.target.element?.closest<HTMLElement>("[data-git-peek-action]")?.dataset.gitPeekAction;
      if (action && peek) {
        event.event.preventDefault();
        peek.handlers.get(action)?.();
        return;
      }
      if (event.target.type !== monaco.editor.MouseTargetType.GUTTER_LINE_DECORATIONS) return;
      const line = event.target.position?.lineNumber;
      const change = line ? changes.find((entry) => line >= entry.startLine && line <= entry.endLine) : undefined;
      if (!change) return;
      if (peek?.change.startLine === change.startLine) closePeek();
      else openPeek(change);
    }),
    editor.addAction({ id: "zorai.git.nextChange", label: "Go to Next Change", keybindings: [monaco.KeyMod.Alt | monaco.KeyCode.F5], run: () => jump(1) }),
    editor.addAction({ id: "zorai.git.previousChange", label: "Go to Previous Change", keybindings: [monaco.KeyMod.Shift | monaco.KeyMod.Alt | monaco.KeyCode.F5], run: () => jump(-1) }),
  ];
  refresh();

  return {
    refresh,
    dispose() {
      window.clearTimeout(timer);
      generation += 1;
      closePeek();
      listeners.forEach((listener) => listener.dispose());
      decorations.clear();
    },
  };
}

export function stageSelectedLines(index: string, working: string, startLine: number, endLine: number): string {
  const base = index.split("\n");
  const out: string[] = [];
  let cursor = 0;
  for (const hunk of computeLineHunks(index, working)) {
    const selected = hunk.added.length > 0
      ? hunk.start + 1 <= endLine && hunk.start + hunk.added.length >= startLine
      : hunk.start <= endLine && hunk.start + 1 >= startLine;
    if (!selected) continue;
    out.push(...base.slice(cursor, hunk.baseStart), ...hunk.added);
    cursor = hunk.baseStart + hunk.removed.length;
  }
  out.push(...base.slice(cursor));
  return out.join("\n");
}
