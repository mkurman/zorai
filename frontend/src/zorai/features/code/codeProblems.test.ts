import { describe, expect, it } from "vitest";
import { countProblems, filterProblems, groupProblemsByFile, problemSeverity, type CodeProblem } from "./codeProblems";

const problem = (path: string, line: number, severity: CodeProblem["severity"], message = "msg"): CodeProblem => ({ path, line, column: 1, message, severity, source: "tsc", code: null });
const all = new Set(["error", "warning", "info"] as const);

describe("Problems panel", () => {
  it("maps LSP hint severity to info so nothing silently disappears", () => {
    expect([1, 2, 3, 4].map(problemSeverity)).toEqual(["error", "warning", "info", "info"]);
  });

  it("puts the file you are editing first, then the files with errors, so the problem you are fixing is on top", () => {
    const groups = groupProblemsByFile([problem("b.ts", 3, "warning"), problem("c.ts", 9, "error"), problem("a.ts", 5, "warning"), problem("a.ts", 2, "error")], "a.ts");
    expect(groups.map((group) => group.path)).toEqual(["a.ts", "c.ts", "b.ts"]);
    expect(groups[0].problems.map((item) => item.line)).toEqual([2, 5]);
  });

  it("filters by text, severity and current file together", () => {
    const items = [problem("a.ts", 1, "error", "Cannot find name foo"), problem("a.ts", 2, "warning", "unused foo"), problem("b.ts", 1, "error", "foo again")];
    expect(filterProblems(items, { query: "FOO", severities: new Set(["error"]), activePath: "a.ts", currentFileOnly: true }).map((item) => item.line)).toEqual([1]);
    expect(filterProblems(items, { query: "", severities: all, activePath: null, currentFileOnly: false })).toHaveLength(3);
  });

  it("counts by severity for the tab badge and status bar", () => {
    expect(countProblems([problem("a", 1, "error"), problem("a", 2, "warning"), problem("b", 1, "warning")])).toEqual({ error: 1, warning: 2, info: 0 });
  });
});
