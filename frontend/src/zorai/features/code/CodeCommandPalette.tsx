import { useMemo, useRef, useState, useEffect } from "react";
import { RUNNABLE_CODE_COMMANDS, displayCodeBinding, type CodeCommandId } from "./codeCommands";
import { rankFuzzyItems } from "./codeFuzzySearch";
import { useCodeEditorSettingsStore } from "./codeEditorSettingsStore";

export function CodeCommandPalette({ onRun, onClose }: { onRun: (id: CodeCommandId) => void; onClose: () => void }) {
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const inputRef = useRef<HTMLInputElement | null>(null);
  const keybindings = useCodeEditorSettingsStore((state) => state.settings.keybindings);
  const results = useMemo(() => rankFuzzyItems(query, RUNNABLE_CODE_COMMANDS.map((command) => {
    const currentBinding = keybindings[command.id] === undefined ? command.defaultKeybinding : keybindings[command.id];
    return { ...command, currentBinding, searchText: `${command.category} ${command.title} ${currentBinding ?? ""} ${(command.aliases ?? []).join(" ")}`, label: command.title };
  })).slice(0, 100), [keybindings, query]);
  useEffect(() => inputRef.current?.focus(), []);
  const run = (id: CodeCommandId) => { onRun(id); onClose(); };
  return <div className="zorai-code-overlay" role="dialog" onBlur={(event) => { if (!event.currentTarget.contains(event.relatedTarget as Node | null)) onClose(); }} aria-label="Code command palette">
    <input ref={inputRef} value={query} onChange={(event) => { setQuery(event.target.value); setActive(0); }} placeholder="Type a command…" aria-label="Code command palette" onKeyDown={(event) => {
      if (event.key === "Escape") onClose();
      if (event.key === "ArrowDown") { event.preventDefault(); setActive((value) => Math.min(results.length - 1, value + 1)); }
      if (event.key === "ArrowUp") { event.preventDefault(); setActive((value) => Math.max(0, value - 1)); }
      if (event.key === "Enter" && results[active]) { event.preventDefault(); run(results[active].id); }
    }} />
    <div role="listbox" className="zorai-code-overlay-list">{results.map((command, index) => <button type="button" role="option" aria-selected={index === active} className={index === active ? "is-active" : ""} key={command.id} onMouseEnter={() => setActive(index)} onClick={() => run(command.id)}><strong>{command.title}</strong><span>{command.category}{command.currentBinding ? ` · ${displayCodeBinding(command.currentBinding)}` : ""}</span></button>)}</div>
  </div>;
}
