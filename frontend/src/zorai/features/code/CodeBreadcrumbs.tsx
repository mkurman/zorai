import { Fragment, useEffect, useState, type MouseEvent } from "react";
import type { editor as MonacoEditorApi } from "monaco-editor";
import { CodeFileIcon, CodeFolderIcon } from "./CodeFileIcon";
import { CodeContextMenu, type CodeToolbarMenuItem } from "./CodeToolbarMenu";
import { symbolAtLine, type QuickOpenSymbol } from "./codeQuickOpenProviders";

const SYMBOL_REFRESH_MS = 800;

type Menu = { x: number; y: number; label: string; items: CodeToolbarMenuItem[] };

export function CodeBreadcrumbs({ root, path, getEditor, getSymbols, listDirectory, onReveal, onOpenFile, onOpenSymbolPicker }: {
  root: string | null;
  path: string;
  getEditor: () => MonacoEditorApi.ICodeEditor | null;
  getSymbols: () => Promise<QuickOpenSymbol[]>;
  listDirectory: (path: string) => Promise<ZoraiWorkspaceEntry[]>;
  onReveal: (path: string) => void;
  onOpenFile: (path: string) => void;
  onOpenSymbolPicker: () => void;
}) {
  const [symbols, setSymbols] = useState<QuickOpenSymbol[]>([]);
  const [cursorLine, setCursorLine] = useState<number | null>(null);
  const [menu, setMenu] = useState<Menu | null>(null);
  const parts = path.split(/[\\/]/).filter(Boolean);
  const current = cursorLine === null ? null : symbolAtLine(symbols, cursorLine);
  const symbolTrail = current ? [...(current.container ? current.container.split(".") : []), current.name] : [];

  useEffect(() => {
    let disposables: Array<{ dispose: () => void }> = [];
    let frame = 0;
    let attempts = 0;
    let refreshTimer: number | null = null;
    let cancelled = false;
    const load = () => { void getSymbols().then((next) => { if (!cancelled) setSymbols(next); }).catch(() => {}); };
    const attach = () => {
      const editor = getEditor();
      const model = editor?.getModel();
      if (!editor || !model || model.uri.path.replace(/^\/+/, "") !== path) {
        if (++attempts < 120) frame = requestAnimationFrame(attach);
        return;
      }
      setCursorLine(editor.getPosition()?.lineNumber ?? 1);
      load();
      disposables = [
        editor.onDidChangeCursorPosition((event) => setCursorLine(event.position.lineNumber)),
        editor.onDidChangeModelContent(() => {
          if (refreshTimer !== null) window.clearTimeout(refreshTimer);
          refreshTimer = window.setTimeout(load, SYMBOL_REFRESH_MS);
        }),
      ];
    };
    setSymbols([]);
    setCursorLine(null);
    attach();
    return () => {
      cancelled = true;
      cancelAnimationFrame(frame);
      if (refreshTimer !== null) window.clearTimeout(refreshTimer);
      disposables.forEach((disposable) => disposable.dispose());
    };
  }, [getEditor, getSymbols, path]);

  const openFolderMenu = (folder: string) => (event: MouseEvent<HTMLButtonElement>) => {
    const rect = event.currentTarget.getBoundingClientRect();
    void listDirectory(folder).then((entries) => {
      const sorted = [...entries].sort((a, b) => Number(b.isDirectory) - Number(a.isDirectory) || a.name.localeCompare(b.name));
      setMenu({
        x: rect.left,
        y: rect.bottom + 2,
        label: `Contents of ${folder || "workspace"}`,
        items: sorted.length > 0
          ? sorted.map((entry) => ({ key: entry.path, label: `${entry.isDirectory ? "▸ " : ""}${entry.name}`, checked: entry.path === path, onSelect: () => (entry.isDirectory ? onReveal(entry.path) : onOpenFile(entry.path)) }))
          : [{ key: "empty", label: "Empty folder", disabled: true }],
      });
    }).catch(() => {});
  };

  return (
    <nav className="zorai-code-path" aria-label="File path" title={root ? `${root.replace(/[\\/]+$/, "")}/${path}` : path}>
      {parts.map((part, index) => {
        const segmentPath = parts.slice(0, index + 1).join("/");
        const parentPath = parts.slice(0, index).join("/");
        const separator = <button type="button" className="zorai-code-path__sep" title={`Show contents of ${parentPath || "the workspace"}`} aria-label={`Show contents of ${parentPath || "the workspace"}`} onClick={openFolderMenu(parentPath)}>›</button>;
        return index === parts.length - 1 ? (
          <Fragment key={`${index}:${part}`}>
            {index > 0 ? separator : null}
            <button type="button" className="zorai-code-path__file" title={`Reveal ${path} in Explorer`} onClick={() => onReveal(path)}><CodeFileIcon path={path} />{part}</button>
          </Fragment>
        ) : (
          <Fragment key={`${index}:${part}`}>
            {index > 0 ? separator : null}
            <button type="button" className="zorai-code-path__folder" title={`Reveal ${segmentPath} in Explorer`} onClick={() => onReveal(segmentPath)}><CodeFolderIcon /><span>{part}</span></button>
          </Fragment>
        );
      })}
      {symbolTrail.map((name, index) => (
        <Fragment key={`symbol:${index}:${name}`}>
          <span className="zorai-code-path__sep" aria-hidden="true">›</span>
          <button type="button" className="zorai-code-path__symbol" title="Go to a symbol in this file (@)" onClick={onOpenSymbolPicker}>{name}</button>
        </Fragment>
      ))}
      {menu ? <CodeContextMenu x={menu.x} y={menu.y} items={menu.items} label={menu.label} onClose={() => setMenu(null)} /> : null}
    </nav>
  );
}
