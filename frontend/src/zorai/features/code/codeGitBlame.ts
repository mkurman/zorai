import type { editor as MonacoEditorApi, IDisposable } from "monaco-editor";
import { computeLineHunks, type LineHunk } from "./codeInlineEdit";

type MonacoApi = typeof import("monaco-editor");

export type BlameLine = { hash: string; author: string; time: number; summary: string; uncommitted: boolean } | null;
export type GitBlameOptions = { enabled: boolean; load: () => Promise<{ lines: BlameLine[]; content: string }> };

export function mapLineToBlamed(hunks: LineHunk[], line: number): number | null {
  let offset = 0;
  for (const hunk of hunks) {
    if (line - 1 < hunk.start) break;
    if (line - 1 < hunk.start + hunk.added.length) return null;
    offset += hunk.removed.length - hunk.added.length;
  }
  return line + offset;
}

export function relativeTime(time: number, now = Date.now()): string {
  const seconds = Math.max(0, Math.round((now - time) / 1000));
  const units: Array<[number, string]> = [[31536000, "year"], [2592000, "month"], [604800, "week"], [86400, "day"], [3600, "hour"], [60, "minute"]];
  for (const [size, unit] of units) {
    const count = Math.floor(seconds / size);
    if (count >= 1) return `${count} ${unit}${count === 1 ? "" : "s"} ago`;
  }
  return "just now";
}

export function blameText(entry: BlameLine, now = Date.now()): string {
  if (!entry || entry.uncommitted) return "You · Uncommitted change";
  return `${entry.author}, ${relativeTime(entry.time, now)} · ${entry.summary}`;
}

export function attachGitBlame(editor: MonacoEditorApi.IStandaloneCodeEditor, monaco: MonacoApi, options: () => GitBlameOptions | null): IDisposable & { refresh: () => void } {
  let blame: { lines: BlameLine[]; content: string } | null = null;
  let hunks: LineHunk[] = [];
  let generation = 0;
  let timer = 0;
  const decorations = editor.createDecorationsCollection([]);

  const render = () => {
    const position = editor.getPosition();
    const model = editor.getModel();
    if (!blame || !position || !model || !options()?.enabled) {
      decorations.clear();
      return;
    }
    const mapped = mapLineToBlamed(hunks, position.lineNumber);
    const entry = mapped === null ? null : blame.lines[mapped - 1] ?? null;
    const line = position.lineNumber;
    decorations.set([{
      range: new monaco.Range(line, model.getLineMaxColumn(line), line, model.getLineMaxColumn(line)),
      options: {
        after: { content: `    ${blameText(entry)}`, inlineClassName: "zorai-git-blame" },
        hoverMessage: entry && !entry.uncommitted
          ? { value: `**${entry.summary.replace(/[*_`]/g, "\\$&")}**\n\n${entry.author} · ${new Date(entry.time).toLocaleString()}\n\n\`${entry.hash.slice(0, 12)}\`` }
          : undefined,
      },
    }]);
  };
  const remap = () => {
    const model = editor.getModel();
    hunks = blame && model ? computeLineHunks(blame.content, model.getValue()) : [];
    render();
  };
  const refresh = () => {
    const current = options();
    const requested = ++generation;
    if (!current?.enabled) {
      blame = null;
      render();
      return;
    }
    void current.load().catch(() => null).then((next) => {
      if (requested !== generation) return;
      blame = next && next.lines.length > 0 ? next : null;
      remap();
    });
  };
  const listeners = [
    editor.onDidChangeCursorPosition(render),
    editor.onDidChangeModel(refresh),
    editor.onDidChangeModelContent(() => {
      window.clearTimeout(timer);
      timer = window.setTimeout(remap, 300);
    }),
  ];
  refresh();
  return {
    refresh,
    dispose() {
      generation += 1;
      window.clearTimeout(timer);
      listeners.forEach((listener) => listener.dispose());
      decorations.clear();
    },
  };
}
