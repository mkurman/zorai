import { useEffect, useMemo, useRef, useState } from "react";
import { parseQuickOpenQuery, rankFuzzyItems } from "./codeFuzzySearch";
import { preloadCodeEditor } from "./codeEditorPreload";
import { CodeFileIcon } from "./CodeFileIcon";

export function CodeQuickOpen({ files, onOpen, onClose }: { files: string[]; onOpen: (path: string, line?: number, column?: number) => void; onClose: () => void }) {
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const inputRef = useRef<HTMLInputElement | null>(null);
  const parsed = parseQuickOpenQuery(query);
  const results = useMemo(() => rankFuzzyItems(parsed.query, [...new Set(files)].map((path, index) => ({ id: path, label: path.split(/[\\/]/).pop() ?? path, searchText: path, open: index < files.length, recent: index < 5 }))).slice(0, 100), [files, parsed.query]);
  useEffect(() => { inputRef.current?.focus(); void preloadCodeEditor(); }, []);
  return <div className="zorai-code-overlay" role="dialog" onBlur={(event) => { if (!event.currentTarget.contains(event.relatedTarget as Node | null)) onClose(); }} aria-label="Quick open file">
    <input ref={inputRef} value={query} onChange={(event) => { setQuery(event.target.value); setActive(0); }} placeholder="Search files by name (append :line to jump)" aria-label="Quick open file" onKeyDown={(event) => {
      if (event.key === "Escape") onClose();
      if (event.key === "ArrowDown") { event.preventDefault(); setActive((value) => Math.min(results.length - 1, value + 1)); }
      if (event.key === "ArrowUp") { event.preventDefault(); setActive((value) => Math.max(0, value - 1)); }
      if (event.key === "Enter" && results[active]) { event.preventDefault(); onOpen(results[active].id, parsed.line ?? undefined, parsed.column ?? undefined); onClose(); }
    }} />
    <div role="listbox" className="zorai-code-overlay-list">{results.map((item, index) => <button type="button" role="option" aria-selected={index === active} className={index === active ? "is-active" : ""} key={item.id} onMouseEnter={() => { setActive(index); void preloadCodeEditor(); }} onClick={() => { onOpen(item.id, parsed.line ?? undefined, parsed.column ?? undefined); onClose(); }}><CodeFileIcon path={item.id} /><strong>{item.label}</strong><span>{item.searchText.split(/[\\/]/).slice(0, -1).join("/")}</span></button>)}</div>
  </div>;
}
