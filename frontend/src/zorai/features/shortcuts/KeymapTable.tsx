import { useMemo, useRef, useState, type KeyboardEvent } from "react";
import { SHORTCUT_CAPTURE_ATTRIBUTE } from "./useAppShortcuts";

export type KeymapRow = { id: string; title: string; category: string; binding: string | null; source: "Default" | "User" | "Removed"; hint?: string };
type Owner = { id: string; title: string };
type Pending = { binding: string; owner: Owner };

export type KeymapTableProps = {
  rows: KeymapRow[];
  keysOf: (binding: string) => string[];
  bindingFromEvent: (event: KeyboardEvent) => string | null;
  ownerOf: (binding: string, exceptId: string) => Owner | undefined;
  onAssign: (id: string, binding: string | null, unbindId?: string) => void;
  onReset: (id: string) => void;
};

export function splitBindingKeys(binding: string): string[] {
  const parts = binding.endsWith("++") ? [...binding.slice(0, -2).split("+"), "+"] : binding.split("+");
  return parts.filter(Boolean);
}

export function KeymapTable({ rows, keysOf, bindingFromEvent, ownerOf, onAssign, onReset }: KeymapTableProps) {
  const [query, setQuery] = useState("");
  const [editing, setEditing] = useState<string | null>(null);
  const [pending, setPendingState] = useState<Pending | null>(null);
  const pendingRef = useRef<Pending | null>(null);
  const setPending = (next: Pending | null) => { pendingRef.current = next; setPendingState(next); };
  const visible = useMemo(() => {
    const needle = query.trim().toLowerCase();
    return rows.filter((row) => `${row.title} ${row.category} ${row.id} ${row.binding ? keysOf(row.binding).join("+") : ""}`.toLowerCase().includes(needle));
  }, [keysOf, query, rows]);

  const stop = () => { setEditing(null); setPending(null); };
  const start = (id: string) => { setEditing(id); setPending(null); };
  const assign = (id: string, binding: string | null, unbindId?: string) => { onAssign(id, binding, unbindId); stop(); };
  const record = (row: KeymapRow, event: KeyboardEvent) => {
    if (event.key === "Tab") return;
    event.preventDefault();
    event.stopPropagation();
    if (event.key === "Escape") { stop(); return; }
    const plain = !event.ctrlKey && !event.metaKey && !event.altKey && !event.shiftKey;
    if (plain && (event.key === "Backspace" || event.key === "Delete")) { assign(row.id, null); return; }
    const binding = bindingFromEvent(event);
    if (!binding) return;
    const owner = ownerOf(binding, row.id);
    if (owner) setPending({ binding, owner });
    else assign(row.id, binding);
  };
  const recorderProps = { "data-code-shortcut-recorder": "", [SHORTCUT_CAPTURE_ATTRIBUTE]: "" };

  return <div className="zorai-keymap">
    <input className="zorai-keymap__search" value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Type to search commands or keys" aria-label="Search keyboard shortcuts" />
    <div className="zorai-keymap__table" role="table" aria-label="Keyboard shortcuts">
      <div className="zorai-keymap__row is-head" role="row"><span role="columnheader">Command</span><span role="columnheader">Keybinding</span><span role="columnheader">Category</span><span role="columnheader">Source</span><span /></div>
      {visible.map((row) => {
        const isEditing = editing === row.id;
        return <div key={row.id} role="row" className={isEditing ? "zorai-keymap__row is-editing" : "zorai-keymap__row"} onDoubleClick={() => start(row.id)}>
          <span role="cell" className="zorai-keymap__title" title={row.hint ?? row.id}>{row.title}</span>
          <span role="cell">
            {isEditing && pending ? <span className="zorai-keymap__conflict" role="alert" {...recorderProps}>
              <Keys keys={keysOf(pending.binding)} /> is used by <strong>{pending.owner.title}</strong>
              <button type="button" autoFocus onClick={() => assign(row.id, pending.binding, pending.owner.id)}>Reassign</button>
              <button type="button" onClick={stop}>Cancel</button>
            </span> : isEditing ? <span {...recorderProps} tabIndex={0} ref={(element) => element?.focus()} className="zorai-keymap__recorder" aria-label={`Press a new shortcut for ${row.title}`} onKeyDown={(event) => record(row, event)} onBlur={() => { if (!pendingRef.current) stop(); }}>
              Press a key combination · Esc cancels · Backspace removes
            </span> : row.binding ? <Keys keys={keysOf(row.binding)} /> : <span className="zorai-keymap__muted">—</span>}
          </span>
          <span role="cell" className="zorai-keymap__muted">{row.category}</span>
          <span role="cell" className={row.source === "Default" ? "zorai-keymap__muted" : undefined}>{row.source}</span>
          <span role="cell" className="zorai-keymap__actions">
            <button type="button" aria-label={`Change shortcut for ${row.title}`} title="Change keybinding" onClick={() => start(row.id)}>✎</button>
            {row.source !== "Default" ? <button type="button" aria-label={`Reset shortcut for ${row.title}`} title="Reset to default" onClick={() => onReset(row.id)}>↺</button> : null}
          </span>
        </div>;
      })}
    </div>
  </div>;
}

function Keys({ keys }: { keys: string[] }) {
  return <span className="zorai-keymap__keys">{keys.map((key, index) => <kbd key={index}>{key}</kbd>)}</span>;
}
