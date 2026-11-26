export type CodeEditorStatus = {
  line: number;
  column: number;
  selectedChars: number;
  insertSpaces: boolean;
  tabSize: number;
  eol: "LF" | "CRLF";
};

type StatusEditor = {
  getPosition: () => { lineNumber: number; column: number } | null;
  getSelections: () => Array<{ isEmpty: () => boolean }> | null;
  getModel: () => {
    getOptions: () => { insertSpaces: boolean; tabSize: number };
    getEOL: () => string;
    getValueInRange: (range: never) => string;
  } | null;
};

export function describeEditorStatus(editor: StatusEditor): CodeEditorStatus | null {
  const model = editor.getModel();
  if (!model) return null;
  const position = editor.getPosition();
  const options = model.getOptions();
  const selectedChars = (editor.getSelections() ?? [])
    .filter((selection) => !selection.isEmpty())
    .reduce((total, selection) => total + model.getValueInRange(selection as never).length, 0);
  return {
    line: position?.lineNumber ?? 1,
    column: position?.column ?? 1,
    selectedChars,
    insertSpaces: options.insertSpaces,
    tabSize: options.tabSize,
    eol: model.getEOL() === "\r\n" ? "CRLF" : "LF",
  };
}
