import { describe, expect, it } from "vitest";
import { buildNotebookShadow, mapShadowDiagnostics, notebookShadowPath, shadowLineToCell } from "./notebookLsp";

const cells = [
  { code: true, source: "import pandas as pd\ndf = pd.read_json('x')" },
  { code: false, source: "# Notes" },
  { code: true, source: "%time df.head()\nfor row in df:\n    !echo hi\ndf.mean()" },
];

describe("notebook shadow document", () => {
  it("joins code cells into one Python file, skipping Markdown, so the language server sees imports from earlier cells", () => {
    const shadow = buildNotebookShadow(cells);
    expect(shadow.starts).toEqual([0, -1, 4]);
    expect(shadow.text.split("\n").slice(4, 8)).toEqual(["pass", "for row in df:", "    pass", "df.mean()"]);
  });

  it("maps shadow lines back to the cell and line the user is looking at", () => {
    const shadow = buildNotebookShadow(cells);
    expect(shadowLineToCell(shadow, 1)).toEqual({ cell: 0, line: 1 });
    expect(shadowLineToCell(shadow, 7)).toEqual({ cell: 2, line: 3 });
    expect(shadowLineToCell(shadow, 2)).toBeNull();
  });

  it("puts each diagnostic on its own cell and drops ones that land in the gap between cells", () => {
    const shadow = buildNotebookShadow(cells);
    const mapped = mapShadowDiagnostics(shadow, [
      { message: "undefined name", severity: 1, source: "Pyright", code: null, startLine: 8, startColumn: 1, endLine: 8, endColumn: 3 },
      { message: "gap", severity: 2, source: null, code: null, startLine: 3, startColumn: 1, endLine: 3, endColumn: 1 },
    ]);
    expect(mapped).toEqual([{ cell: 2, startLine: 4, startColumn: 1, endLine: 4, endColumn: 3, message: "undefined name", severity: 1, source: "Pyright" }]);
  });

  it("uses a sibling path the session accepts without a file on disk", () => {
    expect(notebookShadowPath("notebooks/test.ipynb")).toMatch(/\.ipynb\.zorai-notebook\.py$/);
  });
});
