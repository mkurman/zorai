import { Component, Suspense, lazy, useEffect, useRef, useState, type ErrorInfo, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { getBridge } from "@/lib/bridge";
import { RarogFixPrompt } from "@/zorai/features/code/RarogFixPrompt";
import { createInlineDiffSession, streamingPreviewText, type InlineDiffSession } from "@/zorai/features/code/codeInlineEdit";
import { registerInlineCompletions, setInlineCompletionsEnabled } from "@/zorai/features/code/codeInlineCompletions";
import { applyMonacoGuiTheme, readGuiMonoFontFamily, resolveMonacoFontFamily, subscribeMonacoGuiTheme, ZORAI_MONACO_THEME } from "@/lib/monacoGuiTheme";
import { monaco } from "@/lib/monacoEnvironment";
import { BUILT_IN_SERVICE_BY_LANGUAGE, suppressBuiltInLanguageService } from "@/lib/monacoBuiltInSuppression";
import type { OnMount } from "@monaco-editor/react";
import { useCodeEditorSettingsStore, type CodeEditorSettings } from "@/zorai/features/code/codeEditorSettingsStore";
import { formatCodeText, prettierParserForLanguage } from "@/zorai/features/code/codeFormatter";
import { monacoModelPath } from "@/zorai/features/code/codeLanguages";
import type { languages as MonacoLanguagesApi } from "monaco-editor";
import { registerCodeEditorActions } from "@/zorai/features/code/codeEditorActions";
import { cssClassAtPosition, cssClassSelectorPattern } from "@/zorai/features/code/codeCssNavigation";
import { lspUriToRelativePath, lspWorkspaceEditFiles, type LspFileEdits, type LspRange } from "@/zorai/features/code/codeLspEdits";

const MonacoEditor = lazy(() => import("@monaco-editor/react").then((module) => ({ default: module.default })));
const MonacoDiffEditor = lazy(() => import("@monaco-editor/react").then((module) => ({ default: module.DiffEditor })));

function useMonacoGuiTheme() {
  applyMonacoGuiTheme(monaco);
  useEffect(() => subscribeMonacoGuiTheme(monaco), []);
}

async function ensureWorkspaceMonacoModel(monacoApi: Parameters<OnMount>[1], root: string, relativePath: string) {
  const uri = monacoApi.Uri.parse(monacoModelPath("zorai-workspace", relativePath));
  if (monacoApi.editor.getModel(uri)) return uri;
  const file = await getBridge()?.workspaceReadFile?.(root, relativePath).catch(() => null);
  if (!file) return null;
  if (!monacoApi.editor.getModel(uri)) monacoApi.editor.createModel(file.content, file.language || undefined, uri);
  return uri;
}

function editorFontFamily(settings: CodeEditorSettings): string {
  return resolveMonacoFontFamily(settings.fontFamily, readGuiMonoFontFamily());
}

type FallbackEditorProps = {
  value: string;
  path: string;
  onChange: (value: string) => void;
  onSelect: (startLine: number, startColumn: number, endLine: number, endColumn: number) => void;
  onSave: () => void;
  onBlur?: () => void;
  textareaRef: React.RefObject<HTMLTextAreaElement | null>;
};

class MonacoBoundary extends Component<{ fallback: ReactNode; children: ReactNode }, { failed: boolean }> {
  state = { failed: false };
  static getDerivedStateFromError() { return { failed: true }; }
  componentDidCatch(error: Error, info: ErrorInfo) { console.error("Monaco editor failed; using textarea fallback", error, info); }
  render() { return this.state.failed ? this.props.fallback : this.props.children; }
}

export function WorkspaceCodeEditor({
  value,
  path,
  language,
  onChange,
  onSelect,
  onSave,
  onBlur,
  onFormatError,
  diagnostics = [],
  testResults = [],
  lsp,
  onNavigateLocation,
  onApplyWorkspaceEdit,
  onAddToChat,
  onCodeFixError,
  onClose,
  workspaceRoot,
  onMount,
  textareaRef,
  settings,
}: FallbackEditorProps & {
  language: string;
  diagnostics?: ZoraiLspDiagnostic[];
  testResults?: Array<{ line: number; status: "passed" | "failed" | "skipped"; message?: string | null }>;
  lsp?: { root: string; path: string; language: string; available: boolean };
  onNavigateLocation?: (path: string, line: number, column: number) => void;
  onApplyWorkspaceEdit?: (files: LspFileEdits[], skipped: number) => void;
  onAddToChat?: (snippet: { startLine: number; endLine: number; text: string } | null) => void;
  onCodeFixError?: (message: string) => void;
  onClose?: () => void;
  workspaceRoot?: string;
  onMount?: OnMount;
  onBlur?: () => void;
  onFormatError?: (message: string) => void;
  settings: CodeEditorSettings;
}) {
  useMonacoGuiTheme();
  const editorRef = useRef<Parameters<OnMount>[0] | null>(null);
  const monacoRef = useRef<Parameters<OnMount>[1] | null>(null);
  const providerDisposablesRef = useRef<Array<{ dispose: () => void }>>([]);
  const actionDisposablesRef = useRef<Array<{ dispose: () => void }>>([]);
  const [mountedEditor, setMountedEditor] = useState<Parameters<OnMount>[0] | null>(null);
  const lspCallbacksRef = useRef({ onNavigateLocation, onApplyWorkspaceEdit, onAddToChat, onCodeFixError, onClose, path, language, workspaceRoot });
  lspCallbacksRef.current = { onNavigateLocation, onApplyWorkspaceEdit, onAddToChat, onCodeFixError, onClose, path, language, workspaceRoot };
  const [fixPrompt, setFixPrompt] = useState<{ id: number; startLine: number; endLine: number; diagnostics: number; busy: boolean; error: string | null; reviewing?: boolean; thinking?: string } | null>(null);
  const inlineDiffRef = useRef<InlineDiffSession | null>(null);
  const [fixPromptHost, setFixPromptHost] = useState<HTMLElement | null>(null);
  const fixRequestRef = useRef(0);
  const fixPromptId = fixPrompt?.id;
  const fixPromptStart = fixPrompt?.startLine ?? 0;
  const fixPromptEnd = fixPrompt?.endLine ?? 0;
  const fixPromptReviewing = Boolean(fixPrompt?.reviewing);
  useEffect(() => {
    const editor = mountedEditor;
    const monaco = monacoRef.current;
    if (!editor || !monaco || fixPromptId === undefined) return;
    const host = document.createElement("div");
    host.className = "zorai-rarog-fix-host";
    const overlay = { getId: () => "zorai.rarog.fixPrompt", getDomNode: () => host, getPosition: () => null };
    const zone = {
      afterLineNumber: fixPromptStart - 1,
      heightInPx: 96,
      domNode: document.createElement("div"),
      onDomNodeTop: (top: number) => { host.style.top = `${top}px`; },
    };
    let zoneId = "";
    const layout = () => {
      const info = editor.getLayoutInfo();
      host.style.left = `${info.contentLeft}px`;
      host.style.width = `${Math.max(240, Math.min(640, info.contentWidth - 32))}px`;
    };
    editor.addOverlayWidget(overlay);
    editor.changeViewZones((accessor) => { zoneId = accessor.addZone(zone); });
    layout();
    const resize = new ResizeObserver(() => {
      const height = host.offsetHeight + 8;
      if (height === zone.heightInPx) return;
      zone.heightInPx = height;
      editor.changeViewZones((accessor) => accessor.layoutZone(zoneId));
    });
    resize.observe(host);
    const layoutListener = editor.onDidLayoutChange(layout);
    const target = editor.createDecorationsCollection(fixPromptReviewing ? [] : [{ range: new monaco.Range(fixPromptStart, 1, fixPromptEnd, 1), options: { isWholeLine: true, className: "zorai-rarog-fix-target" } }]);
    setFixPromptHost(host);
    return () => {
      resize.disconnect();
      layoutListener.dispose();
      target.clear();
      editor.changeViewZones((accessor) => accessor.removeZone(zoneId));
      editor.removeOverlayWidget(overlay);
      setFixPromptHost(null);
    };
  }, [mountedEditor, fixPromptId, fixPromptStart, fixPromptEnd, fixPromptReviewing]);
  useEffect(() => {
    inlineDiffRef.current?.reject();
    inlineDiffRef.current = null;
    setFixPrompt(null);
  }, [path]);
  const closeFixPrompt = () => {
    fixRequestRef.current += 1;
    inlineDiffRef.current?.reject();
    inlineDiffRef.current = null;
    setFixPrompt(null);
    mountedEditor?.focus();
  };
  const acceptInlineEdit = () => {
    const session = inlineDiffRef.current;
    const editor = mountedEditor;
    const monaco = monacoRef.current;
    if (!session || !editor || !monaco) return;
    const { startLine, endLine } = session.accept();
    inlineDiffRef.current = null;
    setFixPrompt(null);
    const model = editor.getModel();
    if (model) editor.setSelection(new monaco.Range(startLine, 1, endLine, model.getLineMaxColumn(endLine)));
    editor.focus();
  };
  const runRarogFix = async (instruction: string, reasoning = true) => {
    const editor = mountedEditor;
    const monaco = monacoRef.current;
    const model = editor?.getModel();
    const bridge = getBridge();
    if (!editor || !monaco || !model || !fixPrompt) return;
    if (!bridge?.agentGenerateCodeFix) {
      setFixPrompt({ ...fixPrompt, error: "Leszy quick fix needs the Zorai desktop app with an updated daemon." });
      return;
    }
    const request = ++fixRequestRef.current;
    const session = inlineDiffRef.current;
    const { startLine, endLine } = session ? session.range() : fixPrompt;
    const lastLine = model.getLineCount();
    const targetRange = new monaco.Range(startLine, 1, endLine, model.getLineMaxColumn(endLine));
    const target = model.getValueInRange(targetRange);
    if (!target.trim() && !instruction.trim()) {
      setFixPrompt((current) => current && { ...current, error: "This line is empty. Describe what Leszy should write here." });
      return;
    }
    const diagnostics = monaco.editor.getModelMarkers({ resource: model.uri })
      .filter((marker) => marker.endLineNumber >= startLine && marker.startLineNumber <= endLine)
      .map((marker) => `line ${marker.startLineNumber}: ${marker.message}${marker.code ? ` (${typeof marker.code === "string" ? marker.code : marker.code.value})` : ""}`);
    setFixPrompt((current) => current && { ...current, busy: true, error: null, thinking: "" });
    const pending = editor.createDecorationsCollection([{ range: targetRange, options: { isWholeLine: true, className: "zorai-rarog-fix-pending" } }]);
    const requestId = `fix-${Date.now()}-${request}`;
    let review = session;
    const settleInlineEdit = () => {
      inlineDiffRef.current = null;
      setFixPrompt(null);
      editor.focus();
    };
    let streamed = "";
    let thought = "";
    let frame = 0;
    const unsubscribe = bridge.onAgentEvent?.((event: { type?: string; data?: { request_id?: string; text?: string; reasoning?: boolean } }) => {
      if (event?.type !== "code-fix-delta" || event.data?.request_id !== requestId || request !== fixRequestRef.current) return;
      if (event.data?.reasoning) thought += event.data?.text ?? "";
      else streamed += event.data?.text ?? "";
      if (frame) return;
      frame = requestAnimationFrame(() => {
        frame = 0;
        if (request !== fixRequestRef.current) return;
        if (thought) setFixPrompt((current) => current && current.thinking !== thought ? { ...current, thinking: thought } : current);
        const partial = streamingPreviewText(streamed);
        if (!partial || request !== fixRequestRef.current || editor.getModel() !== model) return;
        pending.clear();
        review ??= createInlineDiffSession(editor, monaco, startLine, target, settleInlineEdit);
        inlineDiffRef.current = review;
        review.preview(partial);
      });
    });
    const fail = (error: string) => {
      inlineDiffRef.current?.cancelPreview();
      if (inlineDiffRef.current && !session && !fixPromptReviewing) inlineDiffRef.current = null;
      if (request === fixRequestRef.current) setFixPrompt((current) => current && { ...current, busy: false, error, thinking: "" });
    };
    try {
      const result = await bridge.agentGenerateCodeFix({
        requestId,
        path,
        language,
        before: startLine > 1 ? model.getValueInRange(new monaco.Range(1, 1, startLine - 1, model.getLineMaxColumn(startLine - 1))) : "",
        target,
        after: endLine < lastLine ? model.getValueInRange(new monaco.Range(endLine + 1, 1, lastLine, model.getLineMaxColumn(lastLine))) : "",
        diagnostics,
        instruction,
        reasoning,
      });
      if (request !== fixRequestRef.current) return;
      cancelAnimationFrame(frame);
      if (result.error || !result.replacement) return fail(`Leszy could not fix this code: ${result.error || "empty reply"}`);
      if (editor.getModel() !== model || (review === session && model.getValueInRange(targetRange) !== target)) return fail("The code changed while Leszy was working, so the fix was not applied.");
      if (result.replacement === target) return fail("Leszy found nothing to change here. Try describing what you want.");
      review ??= createInlineDiffSession(editor, monaco, startLine, target, settleInlineEdit);
      inlineDiffRef.current = review;
      review.show(result.replacement);
      setFixPrompt((current) => current && { ...current, busy: false, error: null, reviewing: true, thinking: "" });
    } catch (reason) {
      fail(`Leszy could not fix this code: ${reason instanceof Error ? reason.message : String(reason)}`);
    } finally {
      cancelAnimationFrame(frame);
      unsubscribe?.();
      pending.clear();
    }
  };
  useEffect(() => {
    const editor = mountedEditor;
    const monaco = monacoRef.current;
    if (!editor || !monaco) return;
    const fixWithRarog = (lines?: { startLine: number; endLine: number }) => {
      const model = editor.getModel();
      if (!model) return;
      const selection = editor.getSelection();
      const cursorLine = selection?.positionLineNumber ?? 1;
      const lineMarkers = monaco.editor.getModelMarkers({ resource: model.uri }).filter((marker) => marker.startLineNumber <= cursorLine && marker.endLineNumber >= cursorLine);
      const span = selection && !selection.isEmpty()
        ? { startLine: selection.startLineNumber, endLine: selection.endColumn === 1 && selection.endLineNumber > selection.startLineNumber ? selection.endLineNumber - 1 : selection.endLineNumber }
        : lines ?? (lineMarkers.length > 0
          ? { startLine: Math.min(...lineMarkers.map((marker) => marker.startLineNumber)), endLine: Math.max(...lineMarkers.map((marker) => marker.endLineNumber)) }
          : { startLine: cursorLine, endLine: cursorLine });
      const diagnostics = monaco.editor.getModelMarkers({ resource: model.uri }).filter((marker) => marker.endLineNumber >= span.startLine && marker.startLineNumber <= span.endLine).length;
      setFixPrompt({ id: Date.now(), ...span, diagnostics, busy: false, error: null });
    };
    const addToChat = () => {
      const model = editor.getModel();
      const selection = editor.getSelection();
      if (!model) return;
      if (!selection || selection.isEmpty()) {
        lspCallbacksRef.current.onAddToChat?.(null);
        return;
      }
      lspCallbacksRef.current.onAddToChat?.({ startLine: selection.startLineNumber, endLine: selection.endLineNumber, text: model.getValueInRange(selection) });
    };
    const fixCommand = `zorai.rarog.fix.${editor.getId()}`;
    const chatCommand = `zorai.chat.add.${editor.getId()}`;
    const disposables = [
      monaco.editor.registerCommand(fixCommand, (_accessor, lines?: { startLine: number; endLine: number }) => fixWithRarog(lines)),
      monaco.editor.registerCommand(chatCommand, () => addToChat()),
      editor.addAction({ id: "zorai.rarog.fix", label: "Fix with Leszy", keybindings: [monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyI], contextMenuGroupId: "0_zorai", contextMenuOrder: 1, run: () => fixWithRarog() }),
      editor.addAction({ id: "zorai.chat.add", label: "Add to Chat", contextMenuGroupId: "0_zorai", contextMenuOrder: 2, run: () => addToChat() }),
      editor.addAction({ id: "zorai.editor.close", label: "Close Editor", run: () => lspCallbacksRef.current.onClose?.() }),
      monaco.editor.registerEditorOpener({
        openCodeEditor: (source, resource, selectionOrPosition) => {
          if (source !== editor || resource.scheme !== "zorai-workspace") return false;
          const target = resource.path.replace(/^\/+/, "");
          if (target === lspCallbacksRef.current.path) return false;
          const start = selectionOrPosition && "startLineNumber" in selectionOrPosition
            ? { line: selectionOrPosition.startLineNumber, column: selectionOrPosition.startColumn }
            : { line: selectionOrPosition?.lineNumber ?? 1, column: selectionOrPosition?.column ?? 1 };
          lspCallbacksRef.current.onNavigateLocation?.(target, start.line, start.column);
          return true;
        },
      }),
      monaco.languages.registerDefinitionProvider(["typescript", "javascript", "html"], {
        provideDefinition: async (model, position) => {
          const root = lspCallbacksRef.current.workspaceRoot;
          const bridge = getBridge();
          if (!root || !bridge?.workspaceSearch || model.uri.toString() !== editor.getModel()?.uri.toString()) return null;
          const cssClass = cssClassAtPosition(model.getLineContent(position.lineNumber), position.column);
          if (!cssClass) return null;
          const hits = await bridge.workspaceSearch(root, cssClassSelectorPattern(cssClass.name), { regex: true, caseSensitive: true, include: "**/*.css,**/*.scss,**/*.less,**/*.sass", maxResults: 50 }).catch(() => []);
          const locations = await Promise.all(hits.map(async (hit) => {
            const uri = await ensureWorkspaceMonacoModel(monaco, root, hit.path);
            const column = hit.column + 1;
            return uri ? { uri, range: new monaco.Range(hit.line, column, hit.line, column + cssClass.name.length) } : null;
          }));
          return locations.filter((location): location is NonNullable<typeof location> => location !== null);
        },
      }),
      editor.onMouseDown((event) => {
        const position = event.target.position;
        if (!event.event.rightButton || !position) return;
        if (!editor.getSelections()?.some((selection) => selection.containsPosition(position))) editor.setPosition(position);
      }),
      monaco.languages.registerCodeActionProvider("*", {
        provideCodeActions: (model, range, context) => {
          if (model.uri.toString() !== editor.getModel()?.uri.toString()) return { actions: [], dispose: () => {} };
          const selected = !range.isEmpty();
          if (!selected && context.markers.length === 0) return { actions: [], dispose: () => {} };
          const lines = context.markers.length > 0 && !selected
            ? { startLine: Math.min(...context.markers.map((marker) => marker.startLineNumber)), endLine: Math.max(...context.markers.map((marker) => marker.endLineNumber)) }
            : undefined;
          return {
            actions: [
              { title: "Fix with Leszy", kind: "quickfix", command: { id: fixCommand, title: "Fix with Leszy", arguments: lines ? [lines] : [] } },
              ...(selected ? [{ title: "Add Selection to Chat", kind: "", command: { id: chatCommand, title: "Add Selection to Chat" } }] : []),
            ],
            dispose: () => {},
          };
        },
      }),
    ];
    return () => disposables.forEach((disposable) => disposable.dispose());
  }, [mountedEditor]);
  const fallback = <FallbackEditor value={value} path={path} onChange={onChange} onSelect={onSelect} onSave={onSave} onBlur={onBlur} textareaRef={textareaRef} />;
  useEffect(() => {
    const monacoEditor = editorRef.current;
    const monaco = monacoRef.current;
    const model = monacoEditor?.getModel();
    if (!monacoEditor || !monaco || !model) return;
    monaco.editor.setModelMarkers(model, "zorai-lsp", diagnostics.map((diagnostic) => ({
      message: diagnostic.message,
      severity: diagnostic.severity === 1
        ? monaco.MarkerSeverity.Error
        : diagnostic.severity === 2
          ? monaco.MarkerSeverity.Warning
          : diagnostic.severity === 3
            ? monaco.MarkerSeverity.Info
            : monaco.MarkerSeverity.Hint,
      source: diagnostic.source ?? undefined,
      code: diagnostic.code ?? undefined,
      startLineNumber: diagnostic.startLine,
      startColumn: diagnostic.startColumn,
      endLineNumber: diagnostic.endLine,
      endColumn: diagnostic.endColumn,
    })));
    const testDecorations = testResults.map((result) => ({
      range: new monaco.Range(result.line, 1, result.line, 1),
      options: {
        isWholeLine: false,
        glyphMarginClassName: `zorai-test-glyph zorai-test-glyph--${result.status}`,
        glyphMarginHoverMessage: { value: result.message ?? `Test ${result.status}` },
      },
    }));
    const decorationCollection = monacoEditor.createDecorationsCollection(testDecorations);
    return () => {
      monaco.editor.setModelMarkers(model, "zorai-lsp", []);
      decorationCollection.clear();
    };
  }, [diagnostics, path, testResults]);
  useEffect(() => {
    const editor = mountedEditor;
    const monaco = monacoRef.current;
    if (!editor || !monaco) return;
      providerDisposablesRef.current.forEach((disposable) => disposable.dispose());
      providerDisposablesRef.current = [];
      const bridge = getBridge();
      if (lsp?.available && bridge?.workspaceLspRequest) {
        const request = (method: "hover" | "definition" | "references" | "completion" | "rename" | "codeAction" | "formatting" | "documentSymbol", position: { lineNumber: number; column: number }, extra?: Record<string, unknown>) => bridge.workspaceLspRequest!(
          lsp.root,
          lsp.path,
          lsp.language,
          method,
          { line: position.lineNumber - 1, character: position.column - 1 },
          extra,
        );
        providerDisposablesRef.current.push(monaco.languages.registerHoverProvider(language, {
          provideHover: async (model, position) => {
            if (model.uri.toString() !== editor.getModel()?.uri.toString()) return null;
            const response = await request("hover", position).catch(() => null);
            const hover = response?.result as any;
            if (!hover?.contents) return null;
            return {
              contents: (Array.isArray(hover.contents) ? hover.contents : [hover.contents]).map((content: any) => ({
                value: typeof content === "string" ? content : content.value ?? String(content),
              })),
            };
          },
        }));
        const locations = (result: any): any[] => result ? (Array.isArray(result) ? result : [result]) : [];
        const ensureWorkspaceModel = (relativePath: string) => ensureWorkspaceMonacoModel(monaco, lsp.root, relativePath);
        const convertLocation = async (location: any): Promise<MonacoLanguagesApi.Location | null> => {
          const uri = location?.targetUri ?? location?.uri;
          const range = location?.targetSelectionRange ?? location?.targetRange ?? location?.range;
          if (typeof uri !== "string" || !range) return null;
          const relativePath = lspUriToRelativePath(uri, lsp.root);
          if (!relativePath) return null;
          const target = relativePath === lsp.path ? editor.getModel()?.uri : await ensureWorkspaceModel(relativePath);
          return target ? { uri: target, range: new monaco.Range(range.start.line + 1, range.start.character + 1, range.end.line + 1, range.end.character + 1) } : null;
        };
        const convertLocations = async (result: unknown) => (await Promise.all(locations(result).map(convertLocation))).filter((location): location is MonacoLanguagesApi.Location => location !== null);
        providerDisposablesRef.current.push(monaco.languages.registerDefinitionProvider(language, {
          provideDefinition: async (model, position) => model.uri.toString() === editor.getModel()?.uri.toString()
            ? convertLocations((await request("definition", position).catch(() => null))?.result)
            : null,
        }));
        providerDisposablesRef.current.push(monaco.languages.registerReferenceProvider(language, {
          provideReferences: async (model, position) => model.uri.toString() === editor.getModel()?.uri.toString()
            ? convertLocations((await request("references", position).catch(() => null))?.result)
            : null,
        }));
        providerDisposablesRef.current.push(monaco.editor.registerLinkOpener({
          open: (resource) => {
            if (resource.scheme !== "file") return false;
            const target = lspUriToRelativePath(resource.with({ fragment: "" }).toString(), lsp.root);
            if (!target) return false;
            const match = /L(\d+)(?:[,:C](\d+))?/.exec(resource.fragment);
            lspCallbacksRef.current.onNavigateLocation?.(target, match ? Number(match[1]) : 1, match?.[2] ? Number(match[2]) : 1);
            return true;
          },
        }));
        providerDisposablesRef.current.push(monaco.languages.registerCompletionItemProvider(language, {
          triggerCharacters: ['.', ':'],
          provideCompletionItems: async (model, position) => {
            if (model.uri.toString() !== editor.getModel()?.uri.toString()) return { suggestions: [] };
            const response = await request("completion", position).catch(() => null);
            const raw: any = response?.result;
            const items: any[] = Array.isArray(raw) ? raw : Array.isArray(raw?.items) ? raw.items : [];
            const word = model.getWordUntilPosition(position);
            const range = new monaco.Range(position.lineNumber, word.startColumn, position.lineNumber, word.endColumn);
            return {
              suggestions: items.slice(0, 500).map((item) => ({
                label: typeof item.label === "string" ? item.label : item.label?.label ?? "completion",
                kind: Math.max(0, Math.min(27, Number(item.kind) || 1)),
                detail: item.detail,
                documentation: typeof item.documentation === "string" ? item.documentation : item.documentation?.value,
                insertText: typeof item.insertText === "string" ? item.insertText : typeof item.textEdit?.newText === "string" ? item.textEdit.newText : typeof item.label === "string" ? item.label : item.label?.label ?? "",
                range,
              })),
            };
          },
        }));
        const isEditorModel = (model: { uri: { toString: () => string } }) => model.uri.toString() === editor.getModel()?.uri.toString();
        const toMonacoRange = (range: LspRange) => new monaco.Range(range.start.line + 1, range.start.character + 1, range.end.line + 1, range.end.character + 1);
        const ownEdits = (edit: unknown) => {
          const { files, skipped } = lspWorkspaceEditFiles(edit, lsp.root);
          const others = files.filter((file) => file.path !== lsp.path);
          if (others.length > 0 || skipped > 0) lspCallbacksRef.current.onApplyWorkspaceEdit?.(others, skipped);
          return files.find((file) => file.path === lsp.path)?.edits ?? [];
        };
        providerDisposablesRef.current.push(monaco.languages.registerRenameProvider(language, {
          provideRenameEdits: async (model, position, newName) => {
            if (!isEditorModel(model)) return { edits: [] };
            const response = await request("rename", position, { newName }).catch((reason) => ({ result: null, reason: reason instanceof Error ? reason.message : String(reason) }));
            if (!response?.result) return { edits: [], rejectReason: response?.reason ?? "The language server cannot rename this symbol." };
            return { edits: ownEdits(response.result).map((edit) => ({ resource: model.uri, textEdit: { range: toMonacoRange(edit.range), text: edit.newText }, versionId: model.getVersionId() })) };
          },
        }));
        const applyEditCommand = `zorai.lsp.applyEdit.${editor.getId()}`;
        providerDisposablesRef.current.push(monaco.editor.registerCommand(applyEditCommand, (_accessor, edit: unknown) => {
          const edits = ownEdits(edit);
          if (edits.length === 0) return;
          editor.pushUndoStop();
          editor.executeEdits("lsp.codeAction", edits.map((textEdit) => ({ range: toMonacoRange(textEdit.range), text: textEdit.newText, forceMoveMarkers: true })));
          editor.pushUndoStop();
        }));
        const lspSeverity = (severity: number) => severity === monaco.MarkerSeverity.Error ? 1 : severity === monaco.MarkerSeverity.Warning ? 2 : severity === monaco.MarkerSeverity.Info ? 3 : 4;
        providerDisposablesRef.current.push(monaco.languages.registerCodeActionProvider(language, {
          provideCodeActions: async (model, range, context) => {
            if (!isEditorModel(model)) return { actions: [], dispose: () => {} };
            const diagnostics = context.markers.map((marker) => ({
              range: { start: { line: marker.startLineNumber - 1, character: marker.startColumn - 1 }, end: { line: marker.endLineNumber - 1, character: marker.endColumn - 1 } },
              message: marker.message,
              severity: lspSeverity(marker.severity),
              source: marker.source,
              code: typeof marker.code === "string" ? marker.code : marker.code?.value,
            }));
            const response = await request("codeAction", { lineNumber: range.startLineNumber, column: range.startColumn }, {
              range: { start: { line: range.startLineNumber - 1, character: range.startColumn - 1 }, end: { line: range.endLineNumber - 1, character: range.endColumn - 1 } },
              diagnostics,
            }).catch(() => null);
            const items: any[] = Array.isArray(response?.result) ? response.result : [];
            return {
              actions: items.filter((item) => typeof item?.title === "string" && item.edit).map((item) => ({
                title: item.title,
                kind: typeof item.kind === "string" ? item.kind : undefined,
                isPreferred: item.isPreferred === true,
                command: { id: applyEditCommand, title: item.title, arguments: [item.edit] },
              })),
              dispose: () => {},
            };
          },
        }));
        providerDisposablesRef.current.push(monaco.languages.registerDocumentFormattingEditProvider(language, {
          provideDocumentFormattingEdits: async (model, options) => {
            if (!isEditorModel(model)) return [];
            const response = await request("formatting", { lineNumber: 1, column: 1 }, { tabSize: options.tabSize, insertSpaces: options.insertSpaces }).catch(() => null);
            const edits: any[] = Array.isArray(response?.result) ? response.result : [];
            return edits.map((edit) => ({ range: toMonacoRange(edit.range), text: edit.newText }));
          },
        }));
        const toDocumentSymbol = (symbol: any): MonacoLanguagesApi.DocumentSymbol => {
          const range = toMonacoRange(symbol.range ?? symbol.location.range);
          return {
            name: String(symbol.name),
            detail: typeof symbol.detail === "string" ? symbol.detail : "",
            kind: Math.max(0, Math.min(25, (Number(symbol.kind) || 1) - 1)),
            tags: [],
            range,
            selectionRange: symbol.selectionRange ? toMonacoRange(symbol.selectionRange) : range,
            children: Array.isArray(symbol.children) ? symbol.children.map(toDocumentSymbol) : [],
          };
        };
        providerDisposablesRef.current.push(monaco.languages.registerDocumentSymbolProvider(language, {
          provideDocumentSymbols: async (model) => {
            if (!isEditorModel(model)) return [];
            const response = await request("documentSymbol", { lineNumber: 1, column: 1 }).catch(() => null);
            const symbols: any[] = Array.isArray(response?.result) ? response.result : [];
            return symbols.filter((symbol) => symbol?.range || symbol?.location?.range).map(toDocumentSymbol);
          },
        }));
      }
    const builtInService = lsp?.available ? BUILT_IN_SERVICE_BY_LANGUAGE[language] : undefined;
    const releaseBuiltInService = builtInService ? suppressBuiltInLanguageService(builtInService) : null;
    return () => {
      providerDisposablesRef.current.forEach((disposable) => disposable.dispose());
      providerDisposablesRef.current = [];
      releaseBuiltInService?.();
    };
  }, [mountedEditor, lsp?.root, lsp?.path, lsp?.language, lsp?.available, language]);
  const handleMount: OnMount = (editor, monaco) => {
    editorRef.current = editor;
    monacoRef.current = monaco;
    applyMonacoGuiTheme(monaco);
    registerInlineCompletions(monaco);
    void getBridge()?.agentGetConfig?.().then((config) => {
      setInlineCompletionsEnabled((config as { code_completion?: { enabled?: boolean } } | null)?.code_completion?.enabled !== false);
    }).catch(() => undefined);
    actionDisposablesRef.current.forEach((disposable) => disposable.dispose());
    actionDisposablesRef.current = registerCodeEditorActions(editor, {
      onSave,
      onFormatDocument: prettierParserForLanguage(language) ? async () => {
        const model = editor.getModel();
        if (!model) return;
        try {
          const formatted = await formatCodeText(model.getValue(), language);
          editor.pushUndoStop();
          editor.executeEdits("edit.formatDocument", [{ range: model.getFullModelRange(), text: formatted, forceMoveMarkers: true }]);
          editor.pushUndoStop();
        } catch (reason) { onFormatError?.(reason instanceof Error ? reason.message : String(reason)); }
      } : undefined,
    });
    setMountedEditor(editor);
    editor.onDidDispose(() => {
      providerDisposablesRef.current.forEach((disposable) => disposable.dispose());
      providerDisposablesRef.current = [];
      actionDisposablesRef.current.forEach((disposable) => disposable.dispose());
      actionDisposablesRef.current = [];
    });
    editor.onDidBlurEditorWidget(() => onBlur?.());
    editor.onDidChangeCursorSelection((event) => onSelect(
      event.selection.startLineNumber,
      event.selection.startColumn,
      event.selection.endLineNumber,
      event.selection.endColumn,
    ));
    editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS, onSave);
    onMount?.(editor, monaco);
  };
  return (
    <MonacoBoundary fallback={fallback}>
      {fixPrompt && fixPromptHost ? createPortal(<RarogFixPrompt key={fixPrompt.id} startLine={fixPrompt.startLine} endLine={fixPrompt.endLine} diagnostics={fixPrompt.diagnostics} busy={fixPrompt.busy} error={fixPrompt.error} reviewing={Boolean(fixPrompt.reviewing)} thinking={fixPrompt.thinking ?? ""} onSubmit={(instruction, reasoning) => void runRarogFix(instruction, reasoning)} onAccept={acceptInlineEdit} onClose={closeFixPrompt} />, fixPromptHost) : null}
      <Suspense fallback={fallback}>
        <MonacoEditor
          path={monacoModelPath("zorai-workspace", path)}
          value={value}
          language={language || "plaintext"}
          theme={ZORAI_MONACO_THEME}
          onMount={handleMount}
          onChange={(next) => onChange(next ?? "")}
          options={{
            automaticLayout: true,
            contextmenu: false,
            minimap: { enabled: settings.minimap, renderCharacters: true, maxColumn: 72, showSlider: "mouseover", side: "right", size: "proportional" },
            fontFamily: editorFontFamily(settings),
            fontSize: settings.fontSize,
            lineHeight: settings.lineHeight,
            tabSize: settings.tabSize,
            insertSpaces: settings.insertSpaces,
            wordWrap: settings.wordWrap === "off" ? "off" : settings.wordWrap === "viewport" ? "on" : "bounded",
            wordWrapColumn: settings.wordWrapColumn,
            renderWhitespace: settings.renderWhitespace,
            lineNumbers: settings.lineNumbers,
            bracketPairColorization: { enabled: settings.bracketGuides },
            guides: { bracketPairs: settings.bracketGuides, indentation: settings.bracketGuides },
            multiCursorModifier: "alt",
            smoothScrolling: settings.smoothScrolling,
            scrollBeyondLastLine: false,
            stickyScroll: { enabled: settings.stickyScroll },
            glyphMargin: settings.glyphMargin,
            formatOnPaste: settings.formatOnPaste,
            formatOnType: settings.formatOnType,
          }}
        />
      </Suspense>
    </MonacoBoundary>
  );
}

function viewerOptions(settings: CodeEditorSettings) {
  return {
    automaticLayout: true,
    readOnly: true,
    domReadOnly: true,
    readOnlyMessage: { value: "Preview only" },
    contextmenu: false,
    dropIntoEditor: { enabled: false },
    quickSuggestions: false,
    suggestOnTriggerCharacters: false,
    parameterHints: { enabled: false },
    codeLens: false,
    formatOnPaste: false,
    formatOnType: false,
    minimap: { enabled: settings.minimap, renderCharacters: true, maxColumn: 72, showSlider: "mouseover" as const, side: "right" as const, size: "proportional" as const },
    fontFamily: editorFontFamily(settings),
    fontSize: settings.fontSize,
    lineHeight: settings.lineHeight,
    tabSize: settings.tabSize,
    insertSpaces: settings.insertSpaces,
    wordWrap: settings.wordWrap === "off" ? "off" as const : settings.wordWrap === "viewport" ? "on" as const : "bounded" as const,
    wordWrapColumn: settings.wordWrapColumn,
    renderWhitespace: settings.renderWhitespace,
    lineNumbers: settings.lineNumbers,
    bracketPairColorization: { enabled: settings.bracketGuides },
    guides: { bracketPairs: settings.bracketGuides, indentation: settings.bracketGuides },
    smoothScrolling: settings.smoothScrolling,
    scrollBeyondLastLine: false,
    stickyScroll: { enabled: settings.stickyScroll },
    glyphMargin: settings.glyphMargin,
    padding: { top: 8, bottom: 8 },
    folding: true,
  };
}

let diffLanguageReady = false;

function ensureDiffLanguage() {
  if (diffLanguageReady) return;
  diffLanguageReady = true;
  monaco.languages.register({ id: "diff" });
  monaco.languages.setMonarchTokensProvider("diff", {
    tokenizer: {
      root: [
        [/^(\+\+\+|---).*$/, "diff-meta"],
        [/^diff .*$/, "diff-meta"],
        [/^@@.*$/, "diff-hunk"],
        [/^\+.*/, "diff-add"],
        [/^-.*/, "diff-remove"],
      ],
    },
  });
}

export function MonacoReadOnlyView({ value, path, language }: { value: string; path: string; language: string }) {
  useMonacoGuiTheme();
  const settings = useCodeEditorSettingsStore((state) => state.settings);
  ensureDiffLanguage();
  const fallback = <pre className="zorai-file-preview-overlay__pre">{value}</pre>;
  return (
    <div className="zorai-monaco-viewer" aria-readonly="true">
      <MonacoBoundary fallback={fallback}>
        <Suspense fallback={fallback}>
          <MonacoEditor
            loading={fallback}
            path={monacoModelPath("zorai-preview", path)}
            value={value}
            language={language || "plaintext"}
            theme={ZORAI_MONACO_THEME}
            options={viewerOptions(settings)}
            onMount={(editor) => {
              editor.updateOptions({ readOnly: true, domReadOnly: true });
            }}
          />
        </Suspense>
      </MonacoBoundary>
    </div>
  );
}

export function WorkspaceDiffEditor({ original, modified, language }: { original: string; modified: string; language: string }) {
  useMonacoGuiTheme();
  const settings = useCodeEditorSettingsStore((state) => state.settings);
  const fallback = <div className="zorai-workspace-diff-grid"><pre>{original}</pre><pre>{modified}</pre></div>;
  return (
    <MonacoBoundary fallback={fallback}>
      <Suspense fallback={fallback}>
        <MonacoDiffEditor
          original={original}
          modified={modified}
          language={language || "plaintext"}
          theme={ZORAI_MONACO_THEME}
          options={{
            automaticLayout: true,
            contextmenu: false,
            readOnly: true,
            renderSideBySide: true,
            minimap: { enabled: false },
            scrollBeyondLastLine: false,
            fontFamily: editorFontFamily(settings),
            fontSize: settings.fontSize,
            lineHeight: settings.lineHeight,
          }}
        />
      </Suspense>
    </MonacoBoundary>
  );
}

function FallbackEditor({ value, path, onChange, onSelect, onSave, onBlur, textareaRef }: FallbackEditorProps) {
  const updateSelection = () => {
    const textarea = textareaRef.current;
    if (!textarea) return;
    const beforeStart = textarea.value.slice(0, textarea.selectionStart);
    const beforeEnd = textarea.value.slice(0, textarea.selectionEnd);
    onSelect(
      beforeStart.split("\n").length,
      beforeStart.length - beforeStart.lastIndexOf("\n"),
      beforeEnd.split("\n").length,
      beforeEnd.length - beforeEnd.lastIndexOf("\n"),
    );
  };
  return (
    <textarea
      ref={textareaRef}
      className="zorai-workspace-code-editor"
      value={value}
      spellCheck={false}
      aria-label={`Edit ${path}`}
      onSelect={updateSelection}
      onKeyDown={(event) => {
        if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "s") {
          event.preventDefault();
          onSave();
        }
        if (event.key === "Tab") {
          event.preventDefault();
          const target = event.currentTarget;
          const start = target.selectionStart;
          const end = target.selectionEnd;
          onChange(`${target.value.slice(0, start)}  ${target.value.slice(end)}`);
          requestAnimationFrame(() => { target.selectionStart = target.selectionEnd = start + 2; });
        }
      }}
      onChange={(event) => onChange(event.target.value)}
      onBlur={onBlur}
    />
  );
}
