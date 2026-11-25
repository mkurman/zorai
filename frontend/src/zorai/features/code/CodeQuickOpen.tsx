import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { parseQuickOpenQuery, rankFuzzyItems } from "./codeFuzzySearch";
import { preloadCodeEditor } from "./codeEditorPreload";
import { CodeFileIcon } from "./CodeFileIcon";
import { RUNNABLE_CODE_COMMANDS, displayCodeBinding, type CodeCommandId } from "./codeCommands";
import { useCodeEditorSettingsStore } from "./codeEditorSettingsStore";
import { QUICK_OPEN_PLACEHOLDER, parseQuickOpenMode, quickOpenLineTarget, type QuickOpenSymbol } from "./codeQuickOpenProviders";

const WORKSPACE_SEARCH_DEBOUNCE_MS = 120;

type QuickOpenItem = { id: string; label: string; searchText: string; detail: string; icon: ReactNode; open?: boolean; recent?: boolean; run: () => void };

const symbolItem = (symbol: QuickOpenSymbol, onOpen: (path: string, line?: number, column?: number) => void, showPath: boolean): QuickOpenItem => ({
  id: `${symbol.path}:${symbol.line}:${symbol.column}:${symbol.name}`,
  label: symbol.name,
  searchText: `${symbol.name} ${symbol.container}`,
  detail: [symbol.kind, symbol.container, showPath ? `${symbol.path}:${symbol.line}` : `line ${symbol.line}`].filter(Boolean).join(" · "),
  icon: null,
  run: () => onOpen(symbol.path, symbol.line, symbol.column),
});

export function CodeQuickOpen({ files, searchFiles, openEditors = [], initialQuery = "", hasActiveFile = false, getSymbols, searchWorkspaceSymbols, onOpen, onRunCommand, onGoToLine, onClose }: {
  files: string[];
  searchFiles?: (query: string) => Promise<string[]>;
  openEditors?: string[];
  initialQuery?: string;
  hasActiveFile?: boolean;
  getSymbols?: () => Promise<QuickOpenSymbol[]>;
  searchWorkspaceSymbols?: (query: string) => Promise<QuickOpenSymbol[]>;
  onOpen: (path: string, line?: number, column?: number) => void;
  onRunCommand?: (id: CodeCommandId) => void;
  onGoToLine?: (line: number, column: number) => void;
  onClose: () => void;
}) {
  const [query, setQuery] = useState(initialQuery);
  const [active, setActive] = useState(0);
  const [workspaceMatches, setWorkspaceMatches] = useState<string[]>([]);
  const [fileSymbols, setFileSymbols] = useState<QuickOpenSymbol[] | null>(null);
  const [workspaceSymbols, setWorkspaceSymbols] = useState<QuickOpenSymbol[]>([]);
  const keybindings = useCodeEditorSettingsStore((state) => state.settings.keybindings);
  const inputRef = useRef<HTMLInputElement | null>(null);
  const { mode, text } = parseQuickOpenMode(query);
  const parsed = parseQuickOpenQuery(mode === "files" ? text : "");

  useEffect(() => { inputRef.current?.focus(); void preloadCodeEditor(); }, []);
  useEffect(() => {
    const fileText = parsed.query.trim();
    if (mode !== "files" || !searchFiles || !fileText) { setWorkspaceMatches([]); return; }
    let cancelled = false;
    const timer = window.setTimeout(() => { searchFiles(fileText).then((matches) => { if (!cancelled) setWorkspaceMatches(matches); }).catch(() => {}); }, WORKSPACE_SEARCH_DEBOUNCE_MS);
    return () => { cancelled = true; window.clearTimeout(timer); };
  }, [mode, parsed.query, searchFiles]);
  useEffect(() => {
    if (mode !== "symbols" || fileSymbols !== null || !getSymbols) return;
    let cancelled = false;
    getSymbols().then((symbols) => { if (!cancelled) setFileSymbols(symbols); }).catch(() => { if (!cancelled) setFileSymbols([]); });
    return () => { cancelled = true; };
  }, [fileSymbols, getSymbols, mode]);
  useEffect(() => {
    if (mode !== "workspaceSymbols" || !searchWorkspaceSymbols || !text.trim()) { setWorkspaceSymbols([]); return; }
    let cancelled = false;
    const timer = window.setTimeout(() => { searchWorkspaceSymbols(text.trim()).then((symbols) => { if (!cancelled) setWorkspaceSymbols(symbols); }).catch(() => {}); }, WORKSPACE_SEARCH_DEBOUNCE_MS);
    return () => { cancelled = true; window.clearTimeout(timer); };
  }, [mode, searchWorkspaceSymbols, text]);

  const results = useMemo<QuickOpenItem[]>(() => {
    if (mode === "commands") {
      return rankFuzzyItems(text, RUNNABLE_CODE_COMMANDS.map((command) => {
        const binding = keybindings[command.id] === undefined ? command.defaultKeybinding : keybindings[command.id];
        return { id: command.id, label: command.title, searchText: `${command.category} ${command.title} ${binding ?? ""} ${(command.aliases ?? []).join(" ")}`, detail: `${command.category}${binding ? ` · ${displayCodeBinding(binding)}` : ""}`, icon: null, run: () => onRunCommand?.(command.id) };
      })).slice(0, 100);
    }
    if (mode === "symbols") return rankFuzzyItems(text, (fileSymbols ?? []).map((symbol) => symbolItem(symbol, onOpen, false))).slice(0, 200);
    if (mode === "workspaceSymbols") return workspaceSymbols.slice(0, 200).map((symbol) => symbolItem(symbol, onOpen, true));
    if (mode === "line") {
      const target = quickOpenLineTarget(text);
      return target && hasActiveFile ? [{ id: "line", label: `Go to line ${target.line}${target.column > 1 ? `, column ${target.column}` : ""}`, searchText: "", detail: "", icon: null, run: () => onGoToLine?.(target.line, target.column) }] : [];
    }
    const paths = mode === "editors" ? openEditors : [...new Set([...files, ...workspaceMatches])];
    const fileQuery = mode === "editors" ? text : parsed.query;
    return rankFuzzyItems(fileQuery, paths.map((path, index) => ({
      id: path,
      label: path.split(/[\\/]/).pop() ?? path,
      searchText: path,
      detail: path.split(/[\\/]/).slice(0, -1).join("/"),
      icon: <CodeFileIcon path={path} />,
      open: mode === "editors" || index < files.length,
      recent: index < 5,
      run: () => onOpen(path, parsed.line ?? undefined, parsed.column ?? undefined),
    }))).slice(0, 100);
  }, [fileSymbols, files, hasActiveFile, keybindings, mode, onGoToLine, onOpen, onRunCommand, openEditors, parsed.column, parsed.line, parsed.query, text, workspaceMatches, workspaceSymbols]);

  const empty = mode === "line" ? (hasActiveFile ? "Type a line number, e.g. :42 or :42:7" : "Open a file to go to a line.")
    : mode === "symbols" ? (!hasActiveFile ? "Open a file to see its symbols." : fileSymbols === null ? "Loading symbols…" : "No matching symbols in this file.")
      : mode === "workspaceSymbols" ? (text.trim() ? "No matching symbols. Workspace symbols need a running language server." : "Type to search symbols across the workspace.")
        : "No matches.";
  const run = (item: QuickOpenItem) => { item.run(); onClose(); };

  return <div className="zorai-code-overlay" role="dialog" onBlur={(event) => { if (!event.currentTarget.contains(event.relatedTarget as Node | null)) onClose(); }} aria-label="Quick open">
    <input ref={inputRef} value={query} onChange={(event) => { setQuery(event.target.value); setActive(0); }} placeholder={QUICK_OPEN_PLACEHOLDER[mode]} aria-label="Quick open" onKeyDown={(event) => {
      if (event.key === "Escape") onClose();
      if (event.key === "ArrowDown") { event.preventDefault(); setActive((value) => Math.min(results.length - 1, value + 1)); }
      if (event.key === "ArrowUp") { event.preventDefault(); setActive((value) => Math.max(0, value - 1)); }
      if (event.key === "Enter" && results[active]) { event.preventDefault(); run(results[active]); }
    }} />
    <div role="listbox" className="zorai-code-overlay-list">
      {results.length === 0 && (query || mode !== "files") ? <p className="zorai-code-overlay-empty">{empty}</p> : null}
      {results.map((item, index) => <button type="button" role="option" aria-selected={index === active} className={index === active ? "is-active" : ""} key={item.id} onMouseEnter={() => { setActive(index); if (mode === "files") void preloadCodeEditor(); }} onClick={() => run(item)}>{item.icon}<strong>{item.label}</strong><span>{item.detail}</span></button>)}
    </div>
  </div>;
}
