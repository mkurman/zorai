import { useEffect, useMemo, useState } from "react";
import { useCodeTaskStore } from "./codeTaskStore";

export function CodeTasksPanel({ root }: { root: string | null }) {
  const { tasks, runs, selectedRunId, discover, run, stop, select } = useCodeTaskStore();
  const [query, setQuery] = useState("");
  const selected = runs.find((entry) => entry.runId === selectedRunId) ?? runs[0] ?? null;
  const visible = useMemo(() => tasks.filter((task) => task.label.toLowerCase().includes(query.trim().toLowerCase())), [query, tasks]);

  useEffect(() => {
    if (root) void discover(root);
  }, [discover, root]);

  if (!root) return <p className="zorai-code-panel__empty">Open a folder to run its tasks.</p>;
  return (
    <div className="zorai-code-tasks">
      <div className="zorai-code-tasks__list">
        <input value={query} placeholder="Filter tasks" aria-label="Filter tasks" onChange={(event) => setQuery(event.target.value)} />
        {visible.length === 0 ? <p className="zorai-code-panel__empty">No tasks found. Add .vscode/tasks.json or package.json scripts.</p> : visible.map((task) => (
          <button key={task.id} type="button" title={`${task.command} · ${task.source}`} onClick={() => void run(root, task)}>
            <span>▶ {task.label}</span>
            {task.group ? <small>{task.group}{task.isDefault ? " · default" : ""}</small> : null}
          </button>
        ))}
        {runs.length > 0 ? <div className="zorai-code-tasks__heading">Recent runs</div> : null}
        {runs.map((entry) => (
          <button key={entry.runId} type="button" className={entry.runId === selected?.runId ? "is-active" : undefined} onClick={() => select(entry.runId)}>
            <span className={`zorai-code-tasks__status is-${entry.status}`}>{entry.status === "running" ? "●" : entry.status === "succeeded" ? "✓" : "✗"}</span>
            <span>{entry.label}</span>
          </button>
        ))}
      </div>
      <div className="zorai-code-tasks__output">
        {selected ? (
          <>
            <div className="zorai-code-tasks__bar">
              <code>{selected.command}</code>
              <span>{selected.status === "running" ? "Running…" : `Exited with code ${selected.code ?? "?"}`}</span>
              {selected.status === "running" ? <button type="button" onClick={() => stop(selected.runId)}>Stop</button> : null}
            </div>
            <pre>{selected.output.replace(/\u001b\[[0-9;?]*[A-Za-z]/g, "") || " "}</pre>
          </>
        ) : <p className="zorai-code-panel__empty">Pick a task to run it. Ctrl+Shift+B runs the default build task.</p>}
      </div>
    </div>
  );
}
