import { useMemo, useState } from "react";
import { CodeFileIcon } from "./CodeFileIcon";
import { countProblems, filterProblems, groupProblemsByFile, type CodeProblem, type CodeProblemSeverity } from "./codeProblems";

const SEVERITY_LABEL: Record<CodeProblemSeverity, string> = { error: "Errors", warning: "Warnings", info: "Info" };
const SEVERITY_GLYPH: Record<CodeProblemSeverity, string> = { error: "⊗", warning: "⚠", info: "ⓘ" };

export function CodeProblemsPanel({ problems, activePath, onOpen, onFix, onAddToChat }: {
  problems: readonly CodeProblem[];
  activePath: string | null;
  onOpen: (problem: CodeProblem) => void;
  onFix?: (problem: CodeProblem) => void;
  onAddToChat?: (problem: CodeProblem) => void;
}) {
  const [query, setQuery] = useState("");
  const [severities, setSeverities] = useState<ReadonlySet<CodeProblemSeverity>>(() => new Set(["error", "warning", "info"]));
  const [currentFileOnly, setCurrentFileOnly] = useState(false);
  const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(() => new Set());
  const counts = useMemo(() => countProblems(problems), [problems]);
  const groups = useMemo(
    () => groupProblemsByFile(filterProblems(problems, { query, severities, activePath, currentFileOnly }), activePath),
    [activePath, currentFileOnly, problems, query, severities],
  );
  const toggleSeverity = (severity: CodeProblemSeverity) => setSeverities((current) => {
    const next = new Set(current);
    if (next.has(severity)) next.delete(severity);
    else next.add(severity);
    return next;
  });
  const toggleGroup = (path: string) => setCollapsed((current) => {
    const next = new Set(current);
    if (next.has(path)) next.delete(path);
    else next.add(path);
    return next;
  });

  return (
    <div className="zorai-code-problems">
      <div className="zorai-code-problems__toolbar">
        <input value={query} placeholder="Filter (e.g. text, path, source)" aria-label="Filter problems" spellCheck={false} onChange={(event) => setQuery(event.target.value)} />
        {(["error", "warning", "info"] as const).map((severity) => (
          <button key={severity} type="button" className={`severity-${severity}${severities.has(severity) ? " is-on" : ""}`} aria-pressed={severities.has(severity)} title={`Show ${SEVERITY_LABEL[severity].toLowerCase()}`} onClick={() => toggleSeverity(severity)}>
            {SEVERITY_GLYPH[severity]} {counts[severity]}
          </button>
        ))}
        <button type="button" className={currentFileOnly ? "is-on" : undefined} aria-pressed={currentFileOnly} title="Show only the current file" onClick={() => setCurrentFileOnly((current) => !current)}>Current file</button>
      </div>
      {problems.length === 0 ? <p className="zorai-code-panel__empty">No problems have been detected in the workspace.</p>
        : groups.length === 0 ? <p className="zorai-code-panel__empty">No problems match the current filters.</p>
          : (
            <div className="zorai-code-problems__list" role="tree" aria-label="Problems">
              {groups.map((group) => {
                const open = !collapsed.has(group.path);
                const name = group.path.split("/").pop() ?? group.path;
                const dir = group.path.slice(0, group.path.length - name.length).replace(/\/$/, "");
                return (
                  <div key={group.path} role="treeitem" aria-expanded={open}>
                    <button type="button" className="zorai-code-problems__file" onClick={() => toggleGroup(group.path)}>
                      <span className={open ? "zorai-code-problems__caret is-open" : "zorai-code-problems__caret"} aria-hidden="true">›</span>
                      <CodeFileIcon path={group.path} />
                      <strong>{name}</strong>
                      {dir ? <span className="zorai-code-problems__dir">{dir}</span> : null}
                      <span className="zorai-code-problems__count">{group.problems.length}</span>
                    </button>
                    {open ? group.problems.map((problem, index) => (
                      <div key={`${problem.line}:${problem.column}:${index}`} className="zorai-code-problems__row">
                        <button type="button" className="zorai-code-problems__open" title={problem.message} onClick={() => onOpen(problem)}>
                          <span className={`severity-${problem.severity}`} aria-label={problem.severity}>{SEVERITY_GLYPH[problem.severity]}</span>
                          <span className="zorai-code-problems__message">{problem.message}</span>
                          <span className="zorai-code-problems__meta">{problem.source ? `${problem.source}${problem.code ? `(${problem.code})` : ""} ` : ""}[Ln {problem.line}, Col {problem.column}]</span>
                        </button>
                        {onFix ? <button type="button" className="zorai-code-problems__action" title="Fix with Leszy" onClick={() => onFix(problem)}>Fix</button> : null}
                        {onAddToChat ? <button type="button" className="zorai-code-problems__action" title="Add to Chat" onClick={() => onAddToChat(problem)}>Chat</button> : null}
                      </div>
                    )) : null}
                  </div>
                );
              })}
            </div>
          )}
    </div>
  );
}
