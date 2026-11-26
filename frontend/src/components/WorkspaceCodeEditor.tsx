import { Component, Suspense, lazy, useEffect, useRef, useState, type ErrorInfo, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { getBridge } from "@/lib/bridge";
import { RarogFixPrompt } from "@/zorai/features/code/RarogFixPrompt";
import { createInlineDiffSession, streamingPreviewText, type InlineDiffSession } from "@/zorai/features/code/codeInlineEdit";
import { registerInlineCompletions, setInlineCompletionsEnabled } from "@/zorai/features/code/codeInlineCompletions";
import { attachNextEditPrediction } from "@/zorai/features/code/codeNextEdit";
import { attachGitGutter, type GitGutterOptions } from "@/zorai/features/code/codeGitGutter";
import { attachMergeConflicts } from "@/zorai/features/code/codeMergeConflicts";
import { attachGitBlame, type GitBlameOptions } from "@/zorai/features/code/codeGitBlame";
import { attachDebugDecorations } from "@/zorai/features/code/debug/codeDebugDecorations";
import { attachProvenanceGutter, type ProvenanceOptions } from "@/zorai/features/code/codeProvenanceGutter";
import { attachKeymapMode } from "@/zorai/features/code/codeKeymapMode";
import { loadWorkspaceSnippets, registerSnippetCompletions } from "@/zorai/features/code/codeSnippets";
import { applyMonacoGuiTheme, readGuiMonoFontFamily, resolveMonacoFontFamily, subscribeMonacoGuiTheme, ZORAI_MONACO_THEME } from "@/lib/monacoGuiTheme";
import { monaco } from "@/lib/monacoEnvironment";
import { BUILT_IN_SERVICE_BY_LANGUAGE, suppressBuiltInLanguageService } from "@/lib/monacoBuiltInSuppression";
import type { OnMount } from "@monaco-editor/react";
import { useCodeEditorSettingsStore, type CodeEditorSettings } from "@/zorai/features/code/codeEditorSettingsStore";
import { formatCodeText, prettierParserForLanguage } from "@/zorai/features/code/codeFormatter";
import { monacoModelPath } from "@/zorai/features/code/codeLanguages";
import { registerCodeEditorActions } from "@/zorai/features/code/codeEditorActions";
import { cssClassAtPosition, cssClassSelectorPattern } from "@/zorai/features/code/codeCssNavigation";
import type { LspEditStep, LspFileEdits } from "@/zorai/features/code/codeLspEdits";
import { registerLspProviders } from "@/zorai/features/code/lspProviders";

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
  resolveFormatOptions,
  gitGutter,
  gitRevision,
  gitBlame,
  provenance,
  diagnostics = [],
  testResults = [],
  lsp,
  onNavigateLocation,
  onApplyWorkspaceEdit,
  onApplyWorkspaceSteps,
  onAddToChat,
  onCodeFixError,
  onClose,
  workspaceRoot,
  onMount,
  textareaRef,
  settings,
  agentReview,
}: FallbackEditorProps & {
  language: string;
  diagnostics?: ZoraiLspDiagnostic[];
  testResults?: Array<{ line: number; status: "passed" | "failed" | "skipped"; message?: string | null }>;
  lsp?: { root: string; path: string; language: string; available: boolean };
  onNavigateLocation?: (path: string, line: number, column: number) => void;
  onApplyWorkspaceEdit?: (files: LspFileEdits[], skipped: number) => void;
  onApplyWorkspaceSteps?: (steps: LspEditStep[], skipped: number) => void;
  onAddToChat?: (snippet: { startLine: number; endLine: number; text: string } | null) => void;
  onCodeFixError?: (message: string) => void;
  onClose?: () => void;
  workspaceRoot?: string;
  onMount?: OnMount;
  onBlur?: () => void;
  onFormatError?: (message: string) => void;
  resolveFormatOptions?: () => Promise<Record<string, string | number | boolean>>;
  gitGutter?: GitGutterOptions | null;
  gitBlame?: GitBlameOptions | null;
  provenance?: ProvenanceOptions | null;
  gitRevision?: unknown;
  settings: CodeEditorSettings;
  agentReview?: { key: string; before: string; onSettled: () => void; onEdit: () => void } | null;
}) {
  useMonacoGuiTheme();
  const editorRef = useRef<Parameters<OnMount>[0] | null>(null);
  const monacoRef = useRef<Parameters<OnMount>[1] | null>(null);
  const providerDisposablesRef = useRef<Array<{ dispose: () => void }>>([]);
  const actionDisposablesRef = useRef<Array<{ dispose: () => void }>>([]);
  const [mountedEditor, setMountedEditor] = useState<Parameters<OnMount>[0] | null>(null);
  const lspCallbacksRef = useRef({ onNavigateLocation, onApplyWorkspaceEdit, onApplyWorkspaceSteps, onAddToChat, onCodeFixError, onClose, onSave, onBlur, path, language, workspaceRoot, resolveFormatOptions, codeLens: settings.lspCodeLens });
  lspCallbacksRef.current = { onNavigateLocation, onApplyWorkspaceEdit, onApplyWorkspaceSteps, onAddToChat, onCodeFixError, onClose, onSave, onBlur, path, language, workspaceRoot, resolveFormatOptions, codeLens: settings.lspCodeLens };
  const gitGutterRef = useRef(gitGutter);
  gitGutterRef.current = gitGutter;
  const gitGutterRefreshRef = useRef<(() => void) | null>(null);
  const hasGitGutter = Boolean(gitGutter);
  useEffect(() => { gitGutterRefreshRef.current?.(); }, [gitRevision, path, hasGitGutter]);
  const keymapRef = useRef(settings.keymap);
  keymapRef.current = settings.keymap;
  const keymapRefreshRef = useRef<(() => void) | null>(null);
  useEffect(() => { keymapRefreshRef.current?.(); }, [settings.keymap]);
  const provenanceRef = useRef(provenance);
  provenanceRef.current = provenance;
  const provenanceRefreshRef = useRef<(() => void) | null>(null);
  const provenanceEnabled = Boolean(provenance?.enabled);
  useEffect(() => { provenanceRefreshRef.current?.(); }, [gitRevision, path, provenanceEnabled]);
  const gitBlameRef = useRef(gitBlame);
  gitBlameRef.current = gitBlame;
  const gitBlameRefreshRef = useRef<(() => void) | null>(null);
  const blameEnabled = Boolean(gitBlame?.enabled);
  useEffect(() => { gitBlameRefreshRef.current?.(); }, [gitRevision, path, blameEnabled]);
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
  const agentReviewCallbacksRef = useRef(agentReview);
  agentReviewCallbacksRef.current = agentReview;
  const agentReviewKey = agentReview?.key ?? null;
  const agentReviewBefore = agentReview?.before ?? null;
  useEffect(() => {
    const editor = mountedEditor;
    const monaco = monacoRef.current;
    const model = editor?.getModel();
    if (!editor || !monaco || !model || agentReviewKey === null || agentReviewBefore === null) return;
    const session = createInlineDiffSession(editor, monaco, 1, agentReviewBefore, () => agentReviewCallbacksRef.current?.onSettled(), {
      review: true,
      onEdit: () => agentReviewCallbacksRef.current?.onEdit(),
    });
    if (session.review() === 0) {
      session.accept();
      agentReviewCallbacksRef.current?.onSettled();
      return;
    }
    return () => { session.accept(); };
  }, [mountedEditor, agentReviewKey, agentReviewBefore]);
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
  const runRarogFix = async (instruction: string, reasoning = true, images: string[] = []) => {
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
        images,
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
      if (lsp?.available) {
        providerDisposablesRef.current.push(registerLspProviders(
          editor,
          monaco,
          { root: lsp.root, path: lsp.path, language },
          () => lspCallbacksRef.current,
          (relativePath) => ensureWorkspaceMonacoModel(monaco, lsp.root, relativePath),
        ));
      }
    const builtInService = lsp?.available ? BUILT_IN_SERVICE_BY_LANGUAGE[language] : undefined;
    const releaseBuiltInService = builtInService ? suppressBuiltInLanguageService(builtInService) : null;
    return () => {
      providerDisposablesRef.current.forEach((disposable) => disposable.dispose());
      providerDisposablesRef.current = [];
      releaseBuiltInService?.();
    };
  }, [mountedEditor, lsp?.root, lsp?.path, lsp?.language, lsp?.available, language, settings.lspCodeLens]);
  const handleMount: OnMount = (editor, monaco) => {
    editorRef.current = editor;
    monacoRef.current = monaco;
    applyMonacoGuiTheme(monaco);
    registerInlineCompletions(monaco);
    registerSnippetCompletions(monaco);
    void loadWorkspaceSnippets(lspCallbacksRef.current.workspaceRoot ?? null);
    const nextEdit = attachNextEditPrediction(editor, monaco, () => lspCallbacksRef.current.path);
    const gutter = attachGitGutter(editor, monaco, () => gitGutterRef.current ?? null);
    gitGutterRefreshRef.current = gutter.refresh;
    const mergeConflicts = attachMergeConflicts(editor, monaco);
    const blame = attachGitBlame(editor, monaco, () => gitBlameRef.current ?? null);
    const provenanceGutter = attachProvenanceGutter(editor, monaco, () => provenanceRef.current ?? null);
    provenanceRefreshRef.current = provenanceGutter.refresh;
    const keymap = attachKeymapMode(editor, monaco, () => keymapRef.current, () => ({ onSave: lspCallbacksRef.current.onSave, onClose: lspCallbacksRef.current.onClose }));
    keymapRefreshRef.current = keymap.refresh;
    const debugDecorations = attachDebugDecorations(editor, monaco, () => {
      const { workspaceRoot: root, path: current } = lspCallbacksRef.current;
      return root && current && !/^([A-Za-z]:)?[\\/]/.test(current) ? { root, path: current } : null;
    });
    gitBlameRefreshRef.current = blame.refresh;
    editor.onDidDispose(() => {
      provenanceGutter.dispose();
      provenanceRefreshRef.current = null;
      keymap.dispose();
      keymapRefreshRef.current = null;
      debugDecorations.dispose();
      blame.dispose();
      gitBlameRefreshRef.current = null;
      nextEdit.dispose();
      gutter.dispose();
      mergeConflicts.dispose();
      gitGutterRefreshRef.current = null;
    });
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
          const formatted = await formatCodeText(model.getValue(), language, await (lspCallbacksRef.current.resolveFormatOptions?.() ?? Promise.resolve({})));
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
    editor.onDidBlurEditorWidget(() => lspCallbacksRef.current.onBlur?.());
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
      {fixPrompt && fixPromptHost ? createPortal(<RarogFixPrompt key={fixPrompt.id} startLine={fixPrompt.startLine} endLine={fixPrompt.endLine} diagnostics={fixPrompt.diagnostics} busy={fixPrompt.busy} error={fixPrompt.error} reviewing={Boolean(fixPrompt.reviewing)} thinking={fixPrompt.thinking ?? ""} allowImages onSubmit={(instruction, reasoning, images) => void runRarogFix(instruction, reasoning, images)} onAccept={acceptInlineEdit} onClose={closeFixPrompt} />, fixPromptHost) : null}
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
            "semanticHighlighting.enabled": settings.semanticHighlighting,
            inlayHints: { enabled: settings.inlayHints },
            linkedEditing: true,
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

export function WorkspaceDiffEditor({ original, modified, language, readOnly = true, onChange, onSave, onStageSelection }: {
  original: string;
  modified: string;
  language: string;
  readOnly?: boolean;
  onChange?: (modified: string) => void;
  onSave?: () => void;
  onStageSelection?: (startLine: number, endLine: number) => void;
}) {
  useMonacoGuiTheme();
  const settings = useCodeEditorSettingsStore((state) => state.settings);
  const [sideBySide, setSideBySide] = useState(true);
  const [collapseUnchanged, setCollapseUnchanged] = useState(false);
  const callbacksRef = useRef({ onChange, onSave });
  callbacksRef.current = { onChange, onSave };
  const modifiedEditorRef = useRef<{ getSelection: () => { startLineNumber: number; endLineNumber: number } | null } | null>(null);
  const fallback = <div className="zorai-workspace-diff-grid"><pre>{original}</pre><pre>{modified}</pre></div>;
  return (
    <MonacoBoundary fallback={fallback}>
      <Suspense fallback={fallback}>
        <div className="zorai-diff-editor">
          <div className="zorai-diff-editor__toolbar">
            {!readOnly ? <span className="zorai-diff-editor__hint">Editable · Ctrl+S saves</span> : null}
            {onStageSelection ? (
              <button type="button" title="Stage only the changes that touch the selected lines" onClick={() => {
                const selection = modifiedEditorRef.current?.getSelection();
                if (selection) onStageSelection(selection.startLineNumber, selection.endLineNumber);
              }}>Stage selected lines</button>
            ) : null}
            <button type="button" className={collapseUnchanged ? "is-on" : undefined} aria-pressed={collapseUnchanged} onClick={() => setCollapseUnchanged((value) => !value)}>Collapse unchanged</button>
            <button type="button" onClick={() => setSideBySide((value) => !value)}>{sideBySide ? "Inline" : "Side by side"}</button>
          </div>
          <MonacoDiffEditor
            original={original}
            modified={modified}
            language={language || "plaintext"}
            theme={ZORAI_MONACO_THEME}
            onMount={(diffEditor, monacoApi) => {
              const modifiedEditor = diffEditor.getModifiedEditor();
              modifiedEditorRef.current = modifiedEditor;
              modifiedEditor.onDidChangeModelContent(() => {
                if (!modifiedEditor.getOption(monacoApi.editor.EditorOption.readOnly)) callbacksRef.current.onChange?.(modifiedEditor.getValue());
              });
              modifiedEditor.addCommand(monacoApi.KeyMod.CtrlCmd | monacoApi.KeyCode.KeyS, () => callbacksRef.current.onSave?.());
            }}
            options={{
              automaticLayout: true,
              contextmenu: false,
              readOnly,
              originalEditable: false,
              renderSideBySide: sideBySide,
              hideUnchangedRegions: { enabled: collapseUnchanged },
              minimap: { enabled: false },
              scrollBeyondLastLine: false,
              fontFamily: editorFontFamily(settings),
              fontSize: settings.fontSize,
              lineHeight: settings.lineHeight,
            }}
          />
        </div>
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
