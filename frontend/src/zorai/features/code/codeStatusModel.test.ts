import { describe, expect, it } from "vitest";
import { describeEditorStatus } from "./codeStatusModel";

const editor = (options: { text: string; selections: Array<[number, number]>; eol?: string; insertSpaces?: boolean; tabSize?: number }) => ({
  getPosition: () => ({ lineNumber: 3, column: 7 }),
  getSelections: () => options.selections.map(([start, end]) => ({ start, end, isEmpty: () => start === end })),
  getModel: () => ({
    getOptions: () => ({ insertSpaces: options.insertSpaces ?? true, tabSize: options.tabSize ?? 2 }),
    getEOL: () => options.eol ?? "\n",
    getValueInRange: (range: never) => {
      const { start, end } = range as unknown as { start: number; end: number };
      return options.text.slice(start, end);
    },
  }),
});

describe("status bar editor info", () => {
  it("counts characters across every selection, like VS Code's '(N selected)' with multiple cursors", () => {
    const status = describeEditorStatus(editor({ text: "hello world", selections: [[0, 5], [6, 11], [3, 3]] }));
    expect(status).toMatchObject({ line: 3, column: 7, selectedChars: 10 });
  });

  it("reports the file's real indentation and line endings so the pickers show what is in use", () => {
    expect(describeEditorStatus(editor({ text: "", selections: [], eol: "\r\n", insertSpaces: false, tabSize: 4 }))).toMatchObject({ insertSpaces: false, tabSize: 4, eol: "CRLF" });
  });
});
