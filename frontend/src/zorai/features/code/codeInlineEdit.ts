import type { editor as MonacoEditorApi } from "monaco-editor";

type MonacoApi = typeof import("monaco-editor");

export type LineHunk = { baseStart: number; start: number; removed: string[]; added: string[] };

const MAX_DIFF_CELLS = 4_000_000;

export function computeLineHunks(base: string, proposed: string): LineHunk[] {
  const before = base.split("\n");
  const after = proposed.split("\n");
  let prefix = 0;
  while (prefix < before.length && prefix < after.length && before[prefix] === after[prefix]) prefix += 1;
  let suffix = 0;
  while (suffix < before.length - prefix && suffix < after.length - prefix && before[before.length - 1 - suffix] === after[after.length - 1 - suffix]) suffix += 1;
  const a = before.slice(prefix, before.length - suffix);
  const b = after.slice(prefix, after.length - suffix);
  if (a.length === 0 && b.length === 0) return [];
  if ((a.length + 1) * (b.length + 1) > MAX_DIFF_CELLS) return [{ baseStart: prefix, start: prefix, removed: a, added: b }];
  const width = b.length + 1;
  const lcs = new Uint32Array((a.length + 1) * width);
  for (let i = a.length - 1; i >= 0; i -= 1) {
    for (let j = b.length - 1; j >= 0; j -= 1) {
      lcs[i * width + j] = a[i] === b[j] ? lcs[(i + 1) * width + j + 1] + 1 : Math.max(lcs[(i + 1) * width + j], lcs[i * width + j + 1]);
    }
  }
  const hunks: LineHunk[] = [];
  let current: LineHunk | null = null;
  let i = 0;
  let j = 0;
  const flush = () => {
    if (current) hunks.push(current);
    current = null;
  };
  while (i < a.length || j < b.length) {
    if (i < a.length && j < b.length && a[i] === b[j]) {
      flush();
      i += 1;
      j += 1;
      continue;
    }
    current ??= { baseStart: prefix + i, start: prefix + j, removed: [], added: [] };
    if (j < b.length && (i === a.length || lcs[i * width + j + 1] >= lcs[(i + 1) * width + j])) {
      current.added.push(b[j]);
      j += 1;
    } else {
      current.removed.push(a[i]);
      i += 1;
    }
  }
  flush();
  return hunks;
}

const CLOSING_TAG = "</target>";

export function streamingPreviewText(raw: string): string {
  const open = raw.lastIndexOf("<think>");
  if (open !== -1 && raw.indexOf("</think>", open) === -1) return "";
  let text = raw.replace(/<think>[\s\S]*?<\/think>\s*/g, "").replace(/^\s*```[^\n]*\n/, "").replace(/^\s*<target>\n?/, "");
  if (/^\s*```[^\n]*$/.test(text)) return "";
  text = text.replace(/\n?`{1,3}\s*$/, "");
  const tail = text.slice(text.lastIndexOf("<"));
  if (tail.length >= 2 && CLOSING_TAG.startsWith(tail.trimEnd())) text = text.slice(0, text.lastIndexOf("<")).replace(/\n$/, "");
  return text;
}

export type InlineDiffSession = {
  show: (proposed: string) => void;
  preview: (partial: string) => void;
  cancelPreview: () => void;
  accept: () => { startLine: number; endLine: number };
  reject: () => void;
  currentText: () => string;
  range: () => { startLine: number; endLine: number };
  review: () => number;
};

export type InlineDiffOptions = { review?: boolean; onEdit?: () => void };

export function createInlineDiffSession(editor: MonacoEditorApi.ICodeEditor, monaco: MonacoApi, startLine: number, original: string, onSettled?: () => void, diffOptions: InlineDiffOptions = {}): InlineDiffSession {
  const model = editor.getModel()!;
  let endLine = diffOptions.review ? editor.getModel()!.getLineCount() : startLine + original.split("\n").length - 1;
  let zoneIds: string[] = [];
  let baseline = original;
  let committed = original;
  let previewing = false;
  let hunks: LineHunk[] = [];
  const decorations = editor.createDecorationsCollection([]);

  const currentRange = () => new monaco.Range(startLine, 1, endLine, model.getLineMaxColumn(endLine));
  const currentText = () => model.getValueInRange(currentRange());
  const clearView = () => {
    decorations.clear();
    if (zoneIds.length > 0) {
      const ids = zoneIds;
      editor.changeViewZones((accessor) => ids.forEach((id) => accessor.removeZone(id)));
      zoneIds = [];
    }
  };
  const replace = (text: string) => {
    editor.pushUndoStop();
    editor.executeEdits("zorai.inlineEdit", [{ range: currentRange(), text, forceMoveMarkers: true }]);
    editor.pushUndoStop();
    endLine = startLine + text.split("\n").length - 1;
  };
  const hunkToolbar = (index: number, height: number) => {
    const bar = document.createElement("div");
    bar.className = "zorai-inline-diff-hunk-bar";
    bar.style.height = `${height}px`;
    for (const [action, label, title] of [["accept", "Keep", "Keep this change"], ["reject", "Undo", "Undo this change"]] as const) {
      const button = document.createElement("button");
      button.type = "button";
      button.className = `zorai-inline-diff-hunk-bar__${action}`;
      button.textContent = label;
      button.title = title;
      button.dataset.hunkAction = action;
      button.dataset.hunkIndex = String(index);
      bar.appendChild(button);
    }
    return bar;
  };
  const render = (proposed: string) => {
    clearView();
    hunks = computeLineHunks(baseline, proposed);
    const options = editor.getOptions();
    const fontInfo = options.get(monaco.editor.EditorOption.fontInfo);
    const lineHeight = options.get(monaco.editor.EditorOption.lineHeight);
    const withToolbar = !previewing && (hunks.length > 1 || Boolean(diffOptions.review));
    decorations.set(hunks.filter((hunk) => hunk.added.length > 0).map((hunk) => ({
      range: new monaco.Range(startLine + hunk.start, 1, startLine + hunk.start + hunk.added.length - 1, 1),
      options: { isWholeLine: true, className: "zorai-inline-diff-added", linesDecorationsClassName: "zorai-inline-diff-added-gutter" },
    })));
    editor.changeViewZones((accessor) => {
      hunks.forEach((hunk, index) => {
        if (hunk.removed.length === 0 && !withToolbar) return;
        const domNode = document.createElement("div");
        domNode.className = "zorai-inline-diff-zone";
        if (withToolbar) domNode.appendChild(hunkToolbar(index, lineHeight));
        if (hunk.removed.length > 0) {
          const removed = document.createElement("div");
          removed.className = "zorai-inline-diff-removed";
          removed.style.fontFamily = fontInfo.fontFamily;
          removed.style.fontSize = `${fontInfo.fontSize}px`;
          removed.style.lineHeight = `${lineHeight}px`;
          for (const text of hunk.removed) {
            const line = document.createElement("div");
            line.textContent = text || " ";
            removed.appendChild(line);
          }
          domNode.appendChild(removed);
        }
        zoneIds.push(accessor.addZone({ afterLineNumber: startLine + hunk.start - 1, heightInLines: hunk.removed.length + (withToolbar ? 1 : 0), domNode }));
      });
    });
  };
  const settleIfDone = () => {
    if (hunks.length > 0) return;
    dispose();
    onSettled?.();
  };
  const decideHunk = (action: string, index: number) => {
    const hunk = hunks[index];
    if (!hunk || previewing) return;
    if (action === "accept") {
      const lines = baseline.split("\n");
      lines.splice(hunk.baseStart, hunk.removed.length, ...hunk.added);
      baseline = lines.join("\n");
    } else {
      const lines = currentText().split("\n");
      lines.splice(hunk.start, hunk.added.length, ...hunk.removed);
      replace(lines.join("\n"));
      diffOptions.onEdit?.();
    }
    committed = currentText();
    render(committed);
    settleIfDone();
  };
  const mouseListener = editor.onMouseDown((event) => {
    const button = (event.target.element as HTMLElement | null)?.closest?.("[data-hunk-action]") as HTMLElement | null;
    if (!button) return;
    event.event.preventDefault();
    event.event.stopPropagation();
    decideHunk(button.dataset.hunkAction ?? "", Number(button.dataset.hunkIndex));
  });
  const dispose = () => {
    clearView();
    mouseListener.dispose();
  };

  const applyQuietly = (text: string) => {
    model.applyEdits([{ range: currentRange(), text, forceMoveMarkers: true }]);
    endLine = startLine + text.split("\n").length - 1;
  };

  return {
    preview(partial) {
      previewing = true;
      applyQuietly(partial);
      render(partial);
    },
    cancelPreview() {
      if (!previewing) return;
      previewing = false;
      applyQuietly(committed);
      if (committed === baseline) clearView();
      else render(committed);
    },
    show(proposed) {
      if (previewing) applyQuietly(committed);
      previewing = false;
      replace(proposed);
      committed = proposed;
      render(proposed);
      editor.revealRangeInCenterIfOutsideViewport(currentRange());
    },
    accept() {
      dispose();
      return { startLine, endLine };
    },
    reject() {
      dispose();
      if (currentText() === baseline) return;
      replace(baseline);
      diffOptions.onEdit?.();
    },
    review() {
      committed = currentText();
      render(committed);
      return hunks.length;
    },
    currentText,
    range: () => ({ startLine, endLine }),
  };
}
