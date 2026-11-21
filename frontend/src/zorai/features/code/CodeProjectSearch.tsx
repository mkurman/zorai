import { useEffect, useRef, useState } from "react";
import { CodeFileIcon } from "./CodeFileIcon";

export type CodeSearchHit = { path: string; line: number; column: number; preview: string };

const SEARCH_DEBOUNCE_MS = 220;

export function CodeProjectSearch({ search, onOpen, onClose }: {
  search: (query: string) => Promise<CodeSearchHit[]>;
  onOpen: (hit: CodeSearchHit) => void;
  onClose: () => void;
}) {
  const [query, setQuery] = useState("");
  const [hits, setHits] = useState<CodeSearchHit[]>([]);
  const [active, setActive] = useState(0);
  const [status, setStatus] = useState<"idle" | "searching" | "done" | "error">("idle");
  const inputRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => { inputRef.current?.focus(); }, []);

  useEffect(() => {
    const text = query.trim();
    if (!text) {
      setHits([]);
      setStatus("idle");
      return;
    }
    let cancelled = false;
    setStatus("searching");
    const timer = window.setTimeout(() => {
      search(text).then((results) => {
        if (cancelled) return;
        setHits(results);
        setActive(0);
        setStatus("done");
      }).catch(() => {
        if (!cancelled) setStatus("error");
      });
    }, SEARCH_DEBOUNCE_MS);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [query, search]);

  const open = (hit: CodeSearchHit | undefined) => {
    if (!hit) return;
    onOpen(hit);
    onClose();
  };

  return (
    <div className="zorai-code-overlay" role="dialog" aria-label="Search in files" onBlur={(event) => { if (!event.currentTarget.contains(event.relatedTarget as Node | null)) onClose(); }}>
      <input
        ref={inputRef}
        value={query}
        placeholder="Search text in workspace files"
        aria-label="Search in files"
        onChange={(event) => setQuery(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Escape") onClose();
          if (event.key === "ArrowDown") { event.preventDefault(); setActive((value) => Math.min(hits.length - 1, value + 1)); }
          if (event.key === "ArrowUp") { event.preventDefault(); setActive((value) => Math.max(0, value - 1)); }
          if (event.key === "Enter") { event.preventDefault(); open(hits[active]); }
        }}
      />
      <div role="listbox" className="zorai-code-overlay-list">
        {status === "searching" && hits.length === 0 ? <div className="zorai-code-overlay-empty">Searching…</div> : null}
        {status === "done" && hits.length === 0 ? <div className="zorai-code-overlay-empty">No matches</div> : null}
        {status === "error" ? <div className="zorai-code-overlay-empty">Search failed</div> : null}
        {hits.map((hit, index) => (
          <button
            type="button"
            role="option"
            aria-selected={index === active}
            className={index === active ? "is-active" : ""}
            key={`${hit.path}:${hit.line}:${hit.column}`}
            onMouseEnter={() => setActive(index)}
            onClick={() => open(hit)}
          >
            <CodeFileIcon path={hit.path} />
            <strong>{hit.path.split(/[\\/]/).pop()}:{hit.line}</strong>
            <span>{hit.preview.trim()}</span>
          </button>
        ))}
      </div>
    </div>
  );
}
