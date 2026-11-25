export type CodeProblemSeverity = "error" | "warning" | "info";

export type CodeProblem = {
  path: string;
  line: number;
  column: number;
  message: string;
  severity: CodeProblemSeverity;
  source: string | null;
  code: string | null;
};

export type CodeProblemFilter = {
  query: string;
  severities: ReadonlySet<CodeProblemSeverity>;
  activePath: string | null;
  currentFileOnly: boolean;
};

export function problemSeverity(lspSeverity: number): CodeProblemSeverity {
  return lspSeverity === 1 ? "error" : lspSeverity === 2 ? "warning" : "info";
}

const SEVERITY_ORDER: Record<CodeProblemSeverity, number> = { error: 0, warning: 1, info: 2 };

export function countProblems(problems: readonly CodeProblem[]): Record<CodeProblemSeverity, number> {
  const counts = { error: 0, warning: 0, info: 0 };
  for (const problem of problems) counts[problem.severity] += 1;
  return counts;
}

export function filterProblems(problems: readonly CodeProblem[], filter: CodeProblemFilter): CodeProblem[] {
  const query = filter.query.trim().toLowerCase();
  return problems.filter((problem) => filter.severities.has(problem.severity)
    && (!filter.currentFileOnly || problem.path === filter.activePath)
    && (!query || `${problem.message} ${problem.path} ${problem.source ?? ""} ${problem.code ?? ""}`.toLowerCase().includes(query)));
}

export function groupProblemsByFile(problems: readonly CodeProblem[], activePath: string | null): Array<{ path: string; problems: CodeProblem[] }> {
  const groups = new Map<string, CodeProblem[]>();
  for (const problem of problems) groups.set(problem.path, [...(groups.get(problem.path) ?? []), problem]);
  const worst = (items: CodeProblem[]) => Math.min(...items.map((problem) => SEVERITY_ORDER[problem.severity]));
  return [...groups.entries()]
    .map(([path, items]) => ({ path, problems: items.sort((a, b) => SEVERITY_ORDER[a.severity] - SEVERITY_ORDER[b.severity] || a.line - b.line || a.column - b.column) }))
    .sort((a, b) => Number(b.path === activePath) - Number(a.path === activePath) || worst(a.problems) - worst(b.problems) || a.path.localeCompare(b.path));
}
