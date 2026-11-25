import { useEffect, useMemo, useRef, useState } from "react";
import { CodeFileIcon } from "./CodeFileIcon";
import { CodeIcon } from "./CodeIcon";
import {
  groupCodeSearchHits,
  previewCodeSearchReplacement,
  splitCodeSearchPreview,
  type CodeSearchHit,
  type CodeSearchOptions,
} from "./codeSearchResults";

const SEARCH_DEBOUNCE_MS = 300;
export const CODE_SEARCH_MAX_RESULTS = 2000;

type SearchRequest = CodeSearchOptions & { include: string; exclude: string; maxResults: number; useIgnoreFiles: boolean; fresh?: boolean };
type ReplaceTarget = { path: string; line?: number; column?: number };

function errorText(reason: unknown): string {
  return reason instanceof Error ? reason.message.replace(/^Error invoking remote method '[^']+': (Error: )?/, "") : String(reason);
}

export function CodeSearchPanel({ root, focusToken, seedQuery = "", search, replace, onOpen }: {
  root: string;
  focusToken: number;
  seedQuery?: string;
  search: (query: string, options: SearchRequest) => Promise<CodeSearchHit[]>;
  replace: (query: string, replacement: string, options: CodeSearchOptions, targets: ReplaceTarget[]) => Promise<{ files: number; replacements: number }>;
  onOpen: (hit: CodeSearchHit) => void;
}) {
  const [query, setQuery] = useState("");
  const [replacement, setReplacement] = useState("");
  const [showReplace, setShowReplace] = useState(false);
  const [showDetails, setShowDetails] = useState(false);
  const [include, setInclude] = useState("");
  const [exclude, setExclude] = useState("");
  const [useIgnoreFiles, setUseIgnoreFiles] = useState(true);
  const freshRef = useRef(false);
  const [options, setOptions] = useState<CodeSearchOptions>({ caseSensitive: false, wholeWord: false, regex: false });
  const [hits, setHits] = useState<CodeSearchHit[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [searching, setSearching] = useState(false);
  const [replacing, setReplacing] = useState(false);
  const [collapsed, setCollapsed] = useState<Set<string>>(() => new Set());
  const [runToken, setRunToken] = useState(0);
  const inputRef = useRef<HTMLInputElement | null>(null);
  const groups = useMemo(() => groupCodeSearchHits(hits), [hits]);

  useEffect(() => {
    if (focusToken === 0) return;
    if (seedQuery) setQuery(seedQuery);
    window.requestAnimationFrame(() => {
      inputRef.current?.focus();
      inputRef.current?.select();
    });
  }, [focusToken, seedQuery]);

  useEffect(() => {
    if (!query.trim()) {
      setHits([]);
      setError(null);
      setSearching(false);
      return;
    }
    let cancelled = false;
    setSearching(true);
    const timer = window.setTimeout(() => {
      const fresh = freshRef.current;
      freshRef.current = false;
      search(query, { ...options, include, exclude, useIgnoreFiles, fresh, maxResults: CODE_SEARCH_MAX_RESULTS })
        .then((results) => {
          if (cancelled) return;
          setHits(results);
          setError(null);
        })
        .catch((reason: unknown) => {
          if (cancelled) return;
          setHits([]);
          setError(errorText(reason));
        })
        .finally(() => { if (!cancelled) setSearching(false); });
    }, SEARCH_DEBOUNCE_MS);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [query, options, include, exclude, root, runToken, search, useIgnoreFiles]);

  const toggleOption = (key: keyof CodeSearchOptions) => setOptions((current) => ({ ...current, [key]: !current[key] }));
  const runReplace = async (targets: ReplaceTarget[]) => {
    if (replacing || targets.length === 0) return;
    setReplacing(true);
    try {
      await replace(query, replacement, options, targets);
      setRunToken((value) => value + 1);
    } catch (reason: unknown) {
      setError(errorText(reason));
    } finally {
      setReplacing(false);
    }
  };
  const allCollapsed = groups.length > 0 && groups.every((group) => collapsed.has(group.path));
  const fileCount = groups.length;
  const resultCount = hits.length;

  return (
    <div className="zorai-code-search">
      <div className="zorai-code-search__header">
        <span>Search</span>
        <span className="zorai-code-search__header-actions">
          <button type="button" title="Refresh" aria-label="Refresh search" disabled={!query.trim()} onClick={() => { freshRef.current = true; setRunToken((value) => value + 1); }}><CodeIcon icon="reload" /></button>
          <button type="button" title="Clear search results" aria-label="Clear search results" disabled={!query && hits.length === 0} onClick={() => { setQuery(""); setReplacement(""); setCollapsed(new Set()); inputRef.current?.focus(); }}>
            <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false"><path d="M4 6h10M4 11h10M4 16h6M15 14l5 5M20 14l-5 5" /></svg>
          </button>
          <button type="button" title={allCollapsed ? "Expand all" : "Collapse all"} aria-label={allCollapsed ? "Expand all" : "Collapse all"} disabled={groups.length === 0} onClick={() => setCollapsed(allCollapsed ? new Set() : new Set(groups.map((group) => group.path)))}>
            <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false">{allCollapsed ? <path d="M5 5h14v14H5zM9 12h6M12 9v6" /> : <path d="M5 5h14v14H5zM9 12h6" />}</svg>
          </button>
        </span>
      </div>
      <div className="zorai-code-search__form">
        <button
          type="button"
          className={showReplace ? "zorai-code-search__replace-toggle is-open" : "zorai-code-search__replace-toggle"}
          title="Toggle Replace"
          aria-label="Toggle Replace"
          aria-expanded={showReplace}
          onClick={() => setShowReplace((value) => !value)}
        >
          <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false"><path d="m9 6 6 6-6 6" /></svg>
        </button>
        <div className="zorai-code-search__fields">
          <div className={error ? "zorai-code-search__field is-invalid" : "zorai-code-search__field"}>
            <input
              ref={inputRef}
              value={query}
              placeholder="Search"
              aria-label="Search"
              spellCheck={false}
              onChange={(event) => setQuery(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") setRunToken((value) => value + 1);
                if (event.key === "Escape" && query) setQuery("");
              }}
            />
            {query ? <button type="button" className="zorai-code-search__clear" title="Clear search (Esc)" aria-label="Clear search" onClick={() => { setQuery(""); inputRef.current?.focus(); }}>×</button> : null}
            <button type="button" className={options.caseSensitive ? "is-on" : undefined} title="Match Case" aria-label="Match Case" aria-pressed={options.caseSensitive} onClick={() => toggleOption("caseSensitive")}>Aa</button>
            <button type="button" className={options.wholeWord ? "is-on zorai-code-search__word" : "zorai-code-search__word"} title="Match Whole Word" aria-label="Match Whole Word" aria-pressed={options.wholeWord} onClick={() => toggleOption("wholeWord")}>ab</button>
            <button type="button" className={options.regex ? "is-on" : undefined} title="Use Regular Expression" aria-label="Use Regular Expression" aria-pressed={options.regex} onClick={() => toggleOption("regex")}>.*</button>
          </div>
          {showReplace ? (
            <div className="zorai-code-search__field">
              <input
                value={replacement}
                placeholder="Replace"
                aria-label="Replace"
                spellCheck={false}
                onChange={(event) => setReplacement(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) void runReplace(groups.map((group) => ({ path: group.path })));
                }}
              />
              <button
                type="button"
                title="Replace All (Ctrl+Enter)"
                aria-label="Replace All"
                disabled={replacing || groups.length === 0}
                onClick={() => void runReplace(groups.map((group) => ({ path: group.path })))}
              >
                <CodeIcon icon="replace" />
              </button>
            </div>
          ) : null}
        </div>
      </div>
      <div className="zorai-code-search__details-toggle">
        <button type="button" title="Toggle Search Details" aria-label="Toggle Search Details" aria-expanded={showDetails} className={showDetails || include || exclude ? "is-on" : undefined} onClick={() => setShowDetails((value) => !value)}>⋯</button>
      </div>
      {showDetails ? (
        <div className="zorai-code-search__details">
          <label>
            <span>files to include</span>
            <input value={include} placeholder="e.g. *.ts, src/**/include" spellCheck={false} onChange={(event) => setInclude(event.target.value)} />
          </label>
          <label>
            <span>files to exclude</span>
            <input value={exclude} placeholder="e.g. *.ts, src/**/exclude" spellCheck={false} onChange={(event) => setExclude(event.target.value)} />
          </label>
          <label className="zorai-code-search__check">
            <input type="checkbox" checked={useIgnoreFiles} onChange={(event) => setUseIgnoreFiles(event.target.checked)} />
            <span>Use .gitignore (skip ignored files)</span>
          </label>
        </div>
      ) : null}
      {query.trim() ? (
        <div className="zorai-code-search__summary" role="status">
          {error ? <span className="is-error">{error}</span>
            : searching && hits.length === 0 ? "Searching…"
              : resultCount === 0 ? "No results found."
                : `${resultCount}${resultCount >= CODE_SEARCH_MAX_RESULTS ? "+" : ""} result${resultCount === 1 ? "" : "s"} in ${fileCount} file${fileCount === 1 ? "" : "s"}${resultCount >= CODE_SEARCH_MAX_RESULTS ? " (limited)" : ""}`}
        </div>
      ) : null}
      <div className="zorai-code-search__results" role="tree" aria-label="Search results">
        {groups.map((group) => {
          const open = !collapsed.has(group.path);
          return (
            <div key={group.path} role="treeitem" aria-expanded={open}>
              <div className="zorai-code-search__file" title={group.path}>
                <button
                  type="button"
                  className="zorai-code-search__file-toggle"
                  onClick={() => setCollapsed((current) => {
                    const next = new Set(current);
                    if (next.has(group.path)) next.delete(group.path);
                    else next.add(group.path);
                    return next;
                  })}
                >
                  <svg className={open ? "is-open" : undefined} viewBox="0 0 24 24" aria-hidden="true" focusable="false"><path d="m9 6 6 6-6 6" /></svg>
                  <CodeFileIcon path={group.path} />
                  <span className="zorai-code-search__file-name">{group.name}</span>
                  <span className="zorai-code-search__file-dir">{group.dir}</span>
                </button>
                {showReplace ? (
                  <button type="button" className="zorai-code-search__row-action" title="Replace All in File" aria-label={`Replace all in ${group.name}`} disabled={replacing} onClick={() => void runReplace([{ path: group.path }])}>
                    <CodeIcon icon="replace" />
                  </button>
                ) : null}
                <span className="zorai-code-search__count">{group.hits.length}</span>
              </div>
              {open ? group.hits.map((hit) => {
                const parts = splitCodeSearchPreview(hit);
                return (
                  <div className="zorai-code-search__match" key={`${hit.line}:${hit.column}`} role="treeitem">
                    <button type="button" title={`${group.path}:${hit.line}:${hit.column}`} onClick={() => onOpen(hit)}>
                      <span>{parts.before}</span>
                      {showReplace && replacement !== "" ? (
                        <>
                          <del>{parts.match}</del>
                          <ins>{previewCodeSearchReplacement(parts.match, query, replacement, options)}</ins>
                        </>
                      ) : <mark>{parts.match}</mark>}
                      <span>{parts.after}</span>
                    </button>
                    {showReplace ? (
                      <button type="button" className="zorai-code-search__row-action" title="Replace" aria-label={`Replace on line ${hit.line}`} disabled={replacing} onClick={() => void runReplace([{ path: hit.path, line: hit.line, column: hit.column }])}>
                        <CodeIcon icon="replace" />
                      </button>
                    ) : null}
                  </div>
                );
              }) : null}
            </div>
          );
        })}
      </div>
    </div>
  );
}
