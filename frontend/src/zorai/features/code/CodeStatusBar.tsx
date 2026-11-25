import { useEffect, useState, type MouseEvent, type ReactNode } from "react";
import type { editor as MonacoEditorApi } from "monaco-editor";
import { monaco } from "@/lib/monacoEnvironment";
import { CodeContextMenu, type CodeToolbarMenuItem } from "./CodeToolbarMenu";
import { describeEditorStatus, type CodeEditorStatus } from "./codeStatusModel";
import { saveInlineCompletionsEnabled, setInlineCompletionsEnabled, useInlineCompletionsEnabled } from "./codeInlineCompletions";

type Menu = { x: number; y: number; label: string; items: CodeToolbarMenuItem[] };

export function CodeStatusBar({ getEditor, documentPath, language, lspIndicator, git, problems, onToggleProblems, problemsActive, terminalActive, onToggleTerminal, onOpenScm, onSync, onChangeLanguage, agentWorking = false, onShowAgent }: {
  getEditor: () => MonacoEditorApi.ICodeEditor | null;
  documentPath: string | null;
  language: string | null;
  lspIndicator: ReactNode;
  git: { branch: string | null; ahead: number; behind: number } | null;
  problems: { error: number; warning: number };
  problemsActive: boolean;
  onToggleProblems: () => void;
  terminalActive: boolean;
  onToggleTerminal: () => void;
  onOpenScm: () => void;
  onSync: () => void;
  onChangeLanguage: (language: string) => void;
  agentWorking?: boolean;
  onShowAgent?: () => void;
}) {
  const [status, setStatus] = useState<CodeEditorStatus | null>(null);
  const [menu, setMenu] = useState<Menu | null>(null);
  const tabCompletion = useInlineCompletionsEnabled();

  useEffect(() => {
    if (!documentPath) { setStatus(null); return; }
    let disposables: Array<{ dispose: () => void }> = [];
    let frame = 0;
    let attempts = 0;
    const attach = () => {
      const editor = getEditor();
      const model = editor?.getModel();
      if (!editor || !model || model.uri.path.replace(/^\/+/, "") !== documentPath) {
        if (++attempts < 120) frame = requestAnimationFrame(attach);
        return;
      }
      const update = () => setStatus(describeEditorStatus(editor));
      update();
      disposables = [editor.onDidChangeCursorSelection(update), editor.onDidChangeModelOptions(update), editor.onDidChangeModelContent(update), editor.onDidChangeModel(update)];
    };
    attach();
    return () => {
      cancelAnimationFrame(frame);
      disposables.forEach((disposable) => disposable.dispose());
    };
  }, [documentPath, getEditor]);

  const withEditor = (run: (editor: MonacoEditorApi.ICodeEditor, model: MonacoEditorApi.ITextModel) => void) => () => {
    const editor = getEditor();
    const model = editor?.getModel();
    if (!editor || !model) return;
    run(editor, model);
    editor.focus();
  };
  const openMenu = (label: string, items: CodeToolbarMenuItem[]) => (event: MouseEvent<HTMLButtonElement>) => {
    const rect = event.currentTarget.getBoundingClientRect();
    setMenu({ x: rect.left, y: rect.top, label, items });
  };
  const indentationItems = (current: CodeEditorStatus): CodeToolbarMenuItem[] => [
    ...[2, 4, 8].map((size) => ({ key: `spaces-${size}`, label: `Indent Using Spaces: ${size}`, checked: current.insertSpaces && current.tabSize === size, onSelect: withEditor((_editor, model) => model.updateOptions({ insertSpaces: true, tabSize: size, indentSize: size })) })),
    ...[2, 4, 8].map((size, index) => ({ key: `tabs-${size}`, label: `Indent Using Tabs: ${size}`, separatorBefore: index === 0, checked: !current.insertSpaces && current.tabSize === size, onSelect: withEditor((_editor, model) => model.updateOptions({ insertSpaces: false, tabSize: size, indentSize: size })) })),
    { key: "to-spaces", label: "Convert Indentation to Spaces", separatorBefore: true, onSelect: withEditor((editor) => void editor.getAction("editor.action.indentationToSpaces")?.run()) },
    { key: "to-tabs", label: "Convert Indentation to Tabs", onSelect: withEditor((editor) => void editor.getAction("editor.action.indentationToTabs")?.run()) },
  ];
  const eolItems = (current: CodeEditorStatus): CodeToolbarMenuItem[] => (["LF", "CRLF"] as const).map((eol) => ({
    key: eol,
    label: eol === "LF" ? "LF (\\n)" : "CRLF (\\r\\n)",
    checked: current.eol === eol,
    onSelect: withEditor((editor, model) => {
      if (current.eol === eol) return;
      editor.pushUndoStop();
      model.pushEOL(eol === "LF" ? monaco.editor.EndOfLineSequence.LF : monaco.editor.EndOfLineSequence.CRLF);
      editor.pushUndoStop();
    }),
  }));
  const languageItems = (): CodeToolbarMenuItem[] => monaco.languages.getLanguages()
    .map((entry) => ({ id: entry.id, label: entry.aliases?.[0] ?? entry.id }))
    .sort((a, b) => a.label.localeCompare(b.label))
    .map((entry) => ({ key: entry.id, label: entry.label, checked: entry.id === language, onSelect: () => onChangeLanguage(entry.id) }));

  return (
    <div className="zorai-workspace-statusbar">
      <span className="zorai-code-statusbar-toggles">
        {git?.branch ? <button type="button" title="Open Source Control" onClick={onOpenScm}>⎇ {git.branch}</button> : null}
        {git && (git.ahead > 0 || git.behind > 0) ? <button type="button" title={`Pull ${git.behind} and push ${git.ahead} commit(s)`} onClick={onSync}>↓{git.behind} ↑{git.ahead}</button> : null}
        <button type="button" className={problemsActive ? "is-active zorai-code-statusbar-problems" : "zorai-code-statusbar-problems"} title={`${problems.error} errors, ${problems.warning} warnings — show Problems`} onClick={onToggleProblems}>
          <span className={problems.error > 0 ? "severity-error" : undefined}>⊗ {problems.error}</span>
          <span className={problems.warning > 0 ? "severity-warning" : undefined}>⚠ {problems.warning}</span>
        </button>
        <button type="button" className={terminalActive ? "is-active" : undefined} title="Toggle Terminal (Ctrl+`)" onClick={onToggleTerminal}>
          <span aria-hidden="true">›_</span> Terminal
        </button>
        {agentWorking ? (
          <button type="button" className="zorai-code-statusbar-agent" title="The agent is working in this thread — show the agent panel" onClick={onShowAgent}>
            <span className="zorai-code-statusbar-agent__spinner" aria-hidden="true" /> Agent working
          </button>
        ) : null}
      </span>
      {documentPath ? (
        <span className="zorai-code-statusbar-editor">
          {status ? (
            <>
              <button type="button" title="Go to Line (Ctrl+G)" onClick={withEditor((editor) => void editor.getAction("editor.action.gotoLine")?.run())}>
                Ln {status.line}, Col {status.column}{status.selectedChars > 0 ? ` (${status.selectedChars} selected)` : ""}
              </button>
              <button type="button" title="Select indentation" onClick={openMenu("Indentation", indentationItems(status))}>{status.insertSpaces ? "Spaces" : "Tab Size"}: {status.tabSize}</button>
              <span title="Files are read and written as UTF-8">UTF-8</span>
              <button type="button" title="Select end of line sequence" onClick={openMenu("End of line", eolItems(status))}>{status.eol}</button>
            </>
          ) : null}
          <button type="button" title="Select language mode" onClick={openMenu("Language mode", languageItems())}>{language ?? "plaintext"}</button>
          <button
            type="button"
            className={tabCompletion ? "is-active" : undefined}
            aria-pressed={tabCompletion}
            title={tabCompletion ? "Tab completion on: Leszy suggests code as you type (click to turn off)" : "Tab completion off (click to turn on)"}
            onClick={() => void saveInlineCompletionsEnabled(!tabCompletion)?.catch(() => setInlineCompletionsEnabled(tabCompletion))}
          >
            Tab: {tabCompletion ? "On" : "Off"}
          </button>
          {lspIndicator}
        </span>
      ) : <span>Ready · Select a file from Explorer</span>}
      {menu ? <CodeContextMenu x={menu.x} y={menu.y} items={menu.items} label={menu.label} onClose={() => setMenu(null)} /> : null}
    </div>
  );
}
