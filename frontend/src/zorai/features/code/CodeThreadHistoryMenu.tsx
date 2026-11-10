import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import type { CodeProjectThreadEntry, CodeProjectThreadStatus } from "./codeProjectThreads";
import { filterCodeProjectThreads, statusPresentation } from "./codeProjectThreads";

export type CodeThreadHistoryEntry = CodeProjectThreadEntry & {
  status: CodeProjectThreadStatus;
  latestCompletionAt: number | null;
};

type CodeThreadHistoryMenuProps = {
  entries: CodeThreadHistoryEntry[];
  currentIdentity: string | null;
  canCreate: boolean;
  loading: boolean;
  error: string | null;
  onCreate: () => void;
  onOpen: () => void;
  onSelect: (entry: CodeThreadHistoryEntry) => void;
  onRetry: () => void;
};

export function CodeThreadHistoryMenu({
  entries,
  currentIdentity,
  canCreate,
  loading,
  error,
  onCreate,
  onOpen,
  onSelect,
  onRetry,
}: CodeThreadHistoryMenuProps) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [activeIndex, setActiveIndex] = useState(0);
  const [expanded, setExpanded] = useState(false);
  const rootRef = useRef<HTMLDivElement | null>(null);
  const searchRef = useRef<HTMLInputElement | null>(null);
  const clockRef = useRef<HTMLButtonElement | null>(null);
  const filtered = useMemo(() => filterCodeProjectThreads(entries, query), [entries, query]);
  const collapsedCount = 5;
  const searching = query.trim().length > 0;
  const visible = useMemo(() => {
    if (searching || expanded) return filtered;
    if (filtered.length <= collapsedCount) return filtered;
    return filtered.slice(0, collapsedCount);
  }, [expanded, filtered, searching]);
  const hiddenCount = filtered.length - visible.length;

  useEffect(() => {
    if (!open) return;
    setActiveIndex(0);
    setExpanded(false);
    requestAnimationFrame(() => searchRef.current?.focus());
    const onPointerDown = (event: globalThis.PointerEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) setOpen(false);
    };
    document.addEventListener("pointerdown", onPointerDown);
    return () => document.removeEventListener("pointerdown", onPointerDown);
  }, [open]);

  const close = () => {
    setOpen(false);
    setQuery("");
    setExpanded(false);
    requestAnimationFrame(() => clockRef.current?.focus());
  };

  const choose = (entry: CodeThreadHistoryEntry) => {
    onSelect(entry);
    close();
  };

  const onSearchKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Escape") {
      event.preventDefault();
      close();
      return;
    }
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      if (visible.length === 0) return;
      const direction = event.key === "ArrowDown" ? 1 : -1;
      setActiveIndex((current) => (current + direction + visible.length) % visible.length);
      return;
    }
    if (event.key === "Enter") {
      const entry = visible[activeIndex];
      if (entry) {
        event.preventDefault();
        choose(entry);
      }
    }
  };

  return (
    <div ref={rootRef} className="zorai-code-thread-controls">
      <button
        type="button"
        className="zorai-thread-header__icon zorai-code-thread-action"
        title={canCreate ? "New project thread" : "Open a project first"}
        aria-label="New project thread"
        disabled={!canCreate}
        onClick={onCreate}
      >
        <PlusIcon />
      </button>
      <button
        ref={clockRef}
        type="button"
        className="zorai-thread-header__icon zorai-code-thread-action"
        title="Project thread history"
        aria-label="Project thread history"
        aria-expanded={open}
        aria-haspopup="dialog"
        onClick={() => setOpen((value) => {
          const next = !value;
          if (next) onOpen();
          return next;
        })}
      >
        <ClockIcon />
      </button>
      {open ? (
        <div className="zorai-code-thread-history" role="dialog" aria-label="Project thread history">
          <input
            ref={searchRef}
            type="search"
            className="zorai-code-thread-search"
            aria-label="Search project threads"
            placeholder="Search project threads…"
            value={query}
            onChange={(event) => {
              setQuery(event.target.value);
              setActiveIndex(0);
              if (event.target.value.trim()) setExpanded(true);
            }}
            onKeyDown={onSearchKeyDown}
          />
          {error ? (
            <div className="zorai-code-thread-error" role="alert">
              <span>{error}</span>
              <button type="button" onClick={onRetry}>Retry</button>
            </div>
          ) : null}
          <div className="zorai-code-thread-history-list" role="listbox" aria-label="Threads for this project">
            {loading && entries.length === 0 ? <div className="zorai-code-thread-empty">Loading project threads…</div> : null}
            {!loading && visible.length === 0 ? (
              <div className="zorai-code-thread-empty">
                {searching ? `No matches for “${query.trim()}”.` : "No other threads for this project."}
              </div>
            ) : null}
            {visible.map((entry, index) => {
              const presentation = statusPresentation(entry.status);
              const selected = entry.identity === currentIdentity;
              return (
                <button
                  type="button"
                  role="option"
                  aria-selected={selected}
                  aria-current={selected ? "true" : undefined}
                  className={[
                    "zorai-code-thread-history-item",
                    selected ? "is-current" : "",
                    index === activeIndex ? "is-keyboard-active" : "",
                  ].filter(Boolean).join(" ")}
                  key={entry.identity}
                  onMouseEnter={() => setActiveIndex(index)}
                  onClick={() => choose(entry)}
                >
                  <span
                    className={presentation.dot ? `zorai-code-thread-status is-${presentation.dot}` : "zorai-code-thread-status"}
                    aria-hidden="true"
                  />
                  <span className="zorai-code-thread-history-copy">
                    <strong>{entry.thread.title}</strong>
                    <span>{presentation.label} · {entry.responder.name}</span>
                    <time dateTime={new Date(entry.thread.updatedAt).toISOString()}>{new Date(entry.thread.updatedAt).toLocaleString()}</time>
                  </span>
                </button>
              );
            })}
            {hiddenCount > 0 ? (
              <button
                type="button"
                className="zorai-code-thread-history-more"
                onClick={() => setExpanded(true)}
              >
                Show {hiddenCount} more
              </button>
            ) : null}
            {searching && expanded && filtered.length > collapsedCount ? (
              <div className="zorai-code-thread-history-hint">
                Searching all {entries.length} thread{entries.length === 1 ? "" : "s"} in this workspace
              </div>
            ) : null}
          </div>
        </div>
      ) : null}
    </div>
  );
}

function PlusIcon() {
  return <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 5v14M5 12h14" /></svg>;
}

function ClockIcon() {
  return <svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="8" /><path d="M12 8v5l3 2" /></svg>;
}
