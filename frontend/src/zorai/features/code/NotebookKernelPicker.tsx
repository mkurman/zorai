import { useMemo, useState } from "react";
import { createPortal } from "react-dom";
import { getBridge } from "@/lib/bridge";
import { rankFuzzyItems } from "./codeFuzzySearch";
import { useNotebookInterpreterStore } from "./notebookInterpreterStore";

type Interpreter = { path: string; label: string; version: string; jupyter: boolean };
export type NotebookKernelInfo = { mode: "jupyter" | "basic" | null; python: string; note: string | null };
type KernelChoice = { id: string; label: string; searchText: string; detail: string; python: string | null };

export function interpreterDisplayName(python: string | null, interpreters: Interpreter[]): string {
  if (!python) return "Workspace default";
  const match = interpreters.find((interpreter) => interpreter.path === python);
  return match ? `${match.label} (Python ${match.version})` : python.split(/[\\/]/).slice(-3).join("/");
}

export function kernelChoices(query: string, interpreters: Interpreter[], defaultPython: string | null, selected: string | null): KernelChoice[] {
  const current = (python: string | null) => (python === selected ? " · Currently selected" : "");
  const choices: KernelChoice[] = [
    { id: "default", label: "Workspace default", searchText: "default workspace", detail: `${defaultPython ?? "first .venv / venv, else python3"}${current(null)}`, python: null },
    ...interpreters.map((interpreter) => ({
      id: interpreter.path,
      label: `${interpreter.label} (Python ${interpreter.version})`,
      searchText: interpreter.path,
      detail: `${interpreter.path}${interpreter.jupyter ? "" : " · no ipykernel, basic mode"}${current(interpreter.path)}`,
      python: interpreter.path,
    })),
  ];
  const typed = query.trim();
  const ranked = typed ? rankFuzzyItems(typed, choices) : choices;
  if (/^(\/|~|\.|[a-zA-Z]:\\)/.test(typed)) ranked.unshift({ id: `custom:${typed}`, label: `Use interpreter at ${typed}`, searchText: typed, detail: "Custom path", python: typed });
  return ranked;
}

export function NotebookKernelPicker({ root, path, info }: { root: string | null; path: string; info: NotebookKernelInfo | null }) {
  const selected = useNotebookInterpreterStore((state) => (root ? state.byRoot[root] ?? null : null));
  const setInterpreter = useNotebookInterpreterStore((state) => state.setInterpreter);
  const [interpreters, setInterpreters] = useState<Interpreter[]>([]);
  const [defaultPython, setDefaultPython] = useState<string | null>(null);
  const [open, setOpen] = useState<{ top: number; right: number } | null>(null);
  const [searching, setSearching] = useState(false);
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const results = useMemo(() => kernelChoices(query, interpreters, defaultPython, selected), [defaultPython, interpreters, query, selected]);

  const show = async (rect: DOMRect) => {
    setOpen({ top: rect.bottom + 6, right: Math.max(16, window.innerWidth - rect.right) });
    setQuery("");
    setActive(0);
    if (!root) return;
    setSearching(true);
    try {
      const found = await getBridge()?.notebookInterpreters?.(root);
      if (found) {
        setInterpreters(found.interpreters);
        setDefaultPython(found.defaultPython);
      }
    } finally {
      setSearching(false);
    }
  };
  const choose = (choice: KernelChoice) => {
    if (root) setInterpreter(root, choice.python);
    setOpen(null);
  };

  const modeLabel = info?.mode === "jupyter" ? "Jupyter" : info?.mode === "basic" ? "Basic" : null;
  return (
    <>
      <button type="button" className="zorai-notebook__kernel" title={info ? `${info.python}${info.note ? `\n${info.note}` : ""}` : "Select the Python interpreter for this workspace's notebooks"} disabled={!root} onClick={(event) => void show(event.currentTarget.getBoundingClientRect())}>
        {interpreterDisplayName(selected, interpreters)}
        {modeLabel ? <span className={info?.mode === "jupyter" ? "zorai-notebook__mode is-jupyter" : "zorai-notebook__mode"}>{modeLabel}</span> : null}
      </button>
      {open ? createPortal(
        <div className="zorai-code-overlay zorai-notebook__kernel-picker" style={{ top: open.top, right: open.right }} role="dialog" aria-label="Select kernel" onBlur={(event) => { if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setOpen(null); }}>
          <input autoFocus value={query} placeholder={`Select kernel for '${path}' — or type a path to python`} aria-label="Select kernel" spellCheck={false} onChange={(event) => { setQuery(event.target.value); setActive(0); }} onKeyDown={(event) => {
            if (event.key === "Escape") setOpen(null);
            if (event.key === "ArrowDown") { event.preventDefault(); setActive((value) => Math.min(results.length - 1, value + 1)); }
            if (event.key === "ArrowUp") { event.preventDefault(); setActive((value) => Math.max(0, value - 1)); }
            if (event.key === "Enter" && results[active]) { event.preventDefault(); choose(results[active]); }
          }} />
          <div role="listbox" className="zorai-code-overlay-list">
            {results.map((choice, index) => <button type="button" role="option" aria-selected={index === active} className={index === active ? "is-active" : ""} key={choice.id} onMouseEnter={() => setActive(index)} onClick={() => choose(choice)}><strong>{choice.label}</strong><span>{choice.detail}</span></button>)}
            {searching ? <p className="zorai-code-overlay-empty">Searching for Python interpreters…</p> : null}
          </div>
        </div>,
        document.body,
      ) : null}
    </>
  );
}
