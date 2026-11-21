import { useEffect, useRef, useState } from "react";
import { CodeFileIcon } from "./CodeFileIcon";
import { FILE_RESULTS_SHOWN, mergeSearchItems, type CodeSearchHit, type SearchItem } from "./projectSearchItems";

export type { CodeSearchHit } from "./projectSearchItems";

const SEARCH_DEBOUNCE_MS = 220;

export function CodeProjectSearch({ search, searchFiles, onOpen, onOpenFile, onClose }: {
  search: (query: string) => Promise<CodeSearchHit[]>;
  searchFiles: (query: string) => Promise<string[]>;
  onOpen: (hit: CodeSearchHit) => void;
  onOpenFile: (path: string) => void;
  onClose: () => void;
}) {
  const [query, setQuery] = useState("");
  const [files, setFiles] = useState<string[]>([]);
  const [hits, setHits] = useState<CodeSearchHit[]>([]);
  const [active, setActive] = useState(0);
  const [pending, setPending] = useState({ files: false, text: false });
  const [failed, setFailed] = useState(false);
  const inputRef = useRef<HTMLInputElement | null>(null);
  const items = mergeSearchItems(files, hits);

  useEffect(() => { inputRef.current?.focus(); }, []);

  useEffect(() => {
    const text = query.trim();
    setActive(0);
    setFailed(false);
    if (!text) {
      setFiles([]);
      setHits([]);
      setPending({ files: false, text: false });
      return;
    }
    let cancelled = false;
    setPending({ files: true, text: true });
    const timer = window.setTimeout(() => {
      searchFiles(text).then((results) => { if (!cancelled) setFiles(results); }).catch(() => { if (!cancelled) setFailed(true); })
        .finally(() => { if (!cancelled) setPending((current) => ({ ...current, files: false })); });
      search(text).then((results) => { if (!cancelled) setHits(results); }).catch(() => { if (!cancelled) setFailed(true); })
        .finally(() => { if (!cancelled) setPending((current) => ({ ...current, text: false })); });
    }, SEARCH_DEBOUNCE_MS);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [query, search, searchFiles]);

  const open = (item: SearchItem | undefined) => {
    if (!item) return;
    if (item.kind === "file") onOpenFile(item.path);
    else onOpen(item.hit);
    onClose();
  };
  const fileCount = Math.min(files.length, FILE_RESULTS_SHOWN);
  const searching = pending.files || pending.text;

  return (
    <div className="zorai-code-overlay" role="dialog" aria-label="Search files" onBlur={(event) => { if (!event.currentTarget.contains(event.relatedTarget as Node | null)) onClose(); }}>
      <input
        ref={inputRef}
        value={query}
        placeholder="Search file names and text in the workspace"
        aria-label="Search files"
        onChange={(event) => setQuery(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Escape") onClose();
          if (event.key === "ArrowDown") { event.preventDefault(); setActive((value) => Math.min(items.length - 1, value + 1)); }
          if (event.key === "ArrowUp") { event.preventDefault(); setActive((value) => Math.max(0, value - 1)); }
          if (event.key === "Enter") { event.preventDefault(); open(items[active]); }
        }}
      />
      <div role="listbox" className="zorai-code-overlay-list">
        {query.trim() && !searching && items.length === 0 && !failed ? <div className="zorai-code-overlay-empty">No matches</div> : null}
        {failed && items.length === 0 ? <div className="zorai-code-overlay-empty">Search failed</div> : null}
        {fileCount > 0 ? <div className="zorai-code-overlay-section">Files</div> : null}
        {items.map((item, index) => {
          const section = index === fileCount && item.kind === "text" ? <div className="zorai-code-overlay-section" key="text-section">In files</div> : null;
          const path = item.kind === "file" ? item.path : item.hit.path;
          const key = item.kind === "file" ? `file:${path}` : `text:${path}:${item.hit.line}:${item.hit.column}`;
          return [
            section,
            <button
              type="button"
              role="option"
              aria-selected={index === active}
              className={index === active ? "is-active" : ""}
              key={key}
              onMouseEnter={() => setActive(index)}
              onClick={() => open(item)}
            >
              <CodeFileIcon path={path} />
              <strong>{item.kind === "file" ? path.split(/[\\/]/).pop() : `${path.split(/[\\/]/).pop()}:${item.hit.line}`}</strong>
              <span>{item.kind === "file" ? path.split(/[\\/]/).slice(0, -1).join("/") : item.hit.preview.trim()}</span>
            </button>,
          ];
        })}
        {searching ? <div className="zorai-code-overlay-empty">{items.length > 0 ? "Searching more…" : "Searching…"}</div> : null}
      </div>
    </div>
  );
}
