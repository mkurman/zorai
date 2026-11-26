import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { CodeProblemsPanel } from "./CodeProblemsPanel";
import type { CodeProblem } from "./codeProblems";

const problem = (path: string, line: number, severity: CodeProblem["severity"], message: string): CodeProblem => ({ path, line, column: 3, message, severity, source: "ts", code: "2304" });

describe("Problems panel rendering", () => {
  it("groups by file with counts and shows where each problem is", () => {
    const html = renderToStaticMarkup(<CodeProblemsPanel problems={[problem("src/a.ts", 4, "error", "Cannot find name 'x'"), problem("src/a.ts", 9, "warning", "unused")]} activePath="src/a.ts" onOpen={vi.fn()} onFix={vi.fn()} onAddToChat={vi.fn()} />);
    expect(html).toContain("<strong>a.ts</strong>");
    expect(html).toContain("ts(2304) [Ln 4, Col 3]");
    expect(html).toContain("⊗ 1");
    expect(html).toContain("⚠ 1");
    expect(html).toContain('title="Fix with Leszy"');
  });

  it("says plainly when the workspace is clean", () => {
    expect(renderToStaticMarkup(<CodeProblemsPanel problems={[]} activePath={null} onOpen={vi.fn()} />)).toContain("No problems have been detected");
  });
});
