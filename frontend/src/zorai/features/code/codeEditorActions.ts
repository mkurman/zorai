import type { editor as MonacoEditorApi } from "monaco-editor";

export type SelectionTextTransform = "uppercase" | "lowercase";
export type SelectionLinesTransform = "ascending" | "descending" | "trim";

export function transformSelectedText(value: string, transform: SelectionTextTransform): string {
  return transform === "uppercase" ? value.toUpperCase() : value.toLowerCase();
}

export function transformSelectedLines(value: string, transform: SelectionLinesTransform): string {
  if (transform === "trim") return value.split("\n").map((line) => line.replace(/[\t ]+$/g, "")).join("\n");
  const lines = value.split("\n").sort((a, b) => a.localeCompare(b));
  return (transform === "descending" ? lines.reverse() : lines).join("\n");
}

export function registerCodeEditorActions(
  editor: MonacoEditorApi.IStandaloneCodeEditor,
  callbacks: { onSave: () => void; onFormatDocument?: () => Promise<void> },
): Array<{ dispose(): void }> {
  const disposables: Array<{ dispose(): void }> = [];
  disposables.push(editor.addAction({ id: "file.save", label: "Save", run: callbacks.onSave }));
  disposables.push(editor.addAction({ id: "edit.undo", label: "Undo", run: () => editor.trigger("zorai", "undo", null) }));
  disposables.push(editor.addAction({ id: "edit.redo", label: "Redo", run: () => editor.trigger("zorai", "redo", null) }));
  const builtins: Array<[string, string, string]> = [
    ["search.file", "Find in File", "actions.find"],
    ["search.replace", "Replace in File", "editor.action.startFindReplaceAction"],
    ["navigation.line", "Go to Line", "editor.action.gotoLine"],
    ["navigation.symbol", "Go to Symbol", "editor.action.quickOutline"],
    ["edit.cut", "Cut", "editor.action.clipboardCutAction"],
    ["edit.copy", "Copy", "editor.action.clipboardCopyAction"],
    ["edit.paste", "Paste", "editor.action.clipboardPasteAction"],
    ["edit.selectAll", "Select All", "editor.action.selectAll"],
    ["edit.indent", "Indent Line or Selection", "editor.action.indentLines"],
    ["edit.outdent", "Outdent Line or Selection", "editor.action.outdentLines"],
    ["navigation.definition", "Go to Definition", "editor.action.revealDefinition"],
    ["navigation.references", "Find References", "editor.action.goToReferences"],
    ["edit.deleteLine", "Delete Line", "editor.action.deleteLines"],
    ["edit.duplicate", "Duplicate Line or Selection", "editor.action.copyLinesDownAction"],
    ["edit.moveUp", "Move Line Up", "editor.action.moveLinesUpAction"],
    ["edit.moveDown", "Move Line Down", "editor.action.moveLinesDownAction"],
    ["edit.commentLine", "Toggle Line Comment", "editor.action.commentLine"],
    ["edit.commentBlock", "Toggle Block Comment", "editor.action.blockComment"],
    ["edit.selectNext", "Select Next Occurrence", "editor.action.addSelectionToNextFindMatch"],
    ["edit.selectAllOccurrences", "Select All Occurrences", "editor.action.selectHighlights"],
    ["edit.insertLineAbove", "Insert Line Above", "editor.action.insertLineBefore"],
    ["edit.insertLineBelow", "Insert Line Below", "editor.action.insertLineAfter"],
    ["edit.joinLines", "Join Lines", "editor.action.joinLines"],
    ...(callbacks.onFormatDocument ? [] : [["edit.formatDocument", "Format Document", "editor.action.formatDocument"] as [string, string, string]]),
    ["edit.formatSelection", "Format Selection", "editor.action.formatSelection"],
  ];
  for (const [id, label, actionId] of builtins) {
    disposables.push(editor.addAction({ id, label, run: () => editor.getAction(actionId)?.run() }));
  }

  if (callbacks.onFormatDocument) disposables.push(editor.addAction({ id: "edit.formatDocument", label: "Format Document", run: callbacks.onFormatDocument }));

  const transform = (id: string, label: string, mapper: (text: string) => string) => {
    disposables.push(editor.addAction({
      id,
      label,
      run: () => {
        const model = editor.getModel();
        const selection = editor.getSelection();
        if (!model || !selection) return;
        const range = selection.isEmpty()
          ? { startLineNumber: selection.startLineNumber, startColumn: 1, endLineNumber: selection.endLineNumber, endColumn: model.getLineMaxColumn(selection.endLineNumber) }
          : selection;
        const text = model.getValueInRange(range);
        editor.pushUndoStop();
        editor.executeEdits(id, [{ range, text: mapper(text), forceMoveMarkers: true }]);
        editor.pushUndoStop();
      },
    }));
  };
  transform("edit.uppercase", "Transform to Uppercase", (text) => transformSelectedText(text, "uppercase"));
  transform("edit.lowercase", "Transform to Lowercase", (text) => transformSelectedText(text, "lowercase"));
  transform("edit.sortAscending", "Sort Selected Lines Ascending", (text) => transformSelectedLines(text, "ascending"));
  transform("edit.sortDescending", "Sort Selected Lines Descending", (text) => transformSelectedLines(text, "descending"));
  transform("edit.trimWhitespace", "Trim Trailing Whitespace", (text) => transformSelectedLines(text, "trim"));
  return disposables;
}
