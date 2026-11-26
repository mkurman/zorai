import type { editor as MonacoEditorApi, IDisposable, languages as MonacoLanguagesApi, Uri } from "monaco-editor";
import { getBridge } from "@/lib/bridge";
import { lspUriToRelativePath, lspWorkspaceEditFiles, lspWorkspaceEditSteps, type LspEditStep, type LspFileEdits, type LspRange } from "./codeLspEdits";

type MonacoApi = typeof import("monaco-editor");
type LspMethod = Parameters<NonNullable<NonNullable<ReturnType<typeof getBridge>>["workspaceLspRequest"]>>[3];
export type LspTarget = { root: string; path: string; language: string };
export type LspProviderCallbacks = {
  onNavigateLocation?: (path: string, line: number, column: number) => void;
  onApplyWorkspaceEdit?: (files: LspFileEdits[], skipped: number) => void;
  onApplyWorkspaceSteps?: (steps: LspEditStep[], skipped: number) => void;
  codeLens?: boolean;
};

const LARGE_DOCUMENT_LINES = 5000;
const LINKED_EDITING_LANGUAGES = new Set(["html", "xml", "typescriptreact", "javascriptreact", "vue", "svelte"]);

export function lspCompletionInsert(item: any): { text: string; snippet: boolean } {
  const text = typeof item?.textEdit?.newText === "string"
    ? item.textEdit.newText
    : typeof item?.insertText === "string"
      ? item.insertText
      : typeof item?.label === "string" ? item.label : item?.label?.label ?? "";
  return { text, snippet: item?.insertTextFormat === 2 };
}

export function lspMarkup(value: unknown): string | undefined {
  if (typeof value === "string") return value;
  if (value && typeof value === "object" && typeof (value as { value?: unknown }).value === "string") return (value as { value: string }).value;
  return undefined;
}

export function lspInlayLabel(label: unknown): string {
  if (typeof label === "string") return label;
  if (Array.isArray(label)) return label.map((part) => (typeof part?.value === "string" ? part.value : "")).join("");
  return "";
}

export type CallDirection = "incoming" | "outgoing";
export type CallHierarchyItem = { name: string; detail?: string; uri: string; selectionRange?: { start: { line: number; character: number } }; range: { start: { line: number; character: number } } };

export function callHierarchyChildren(result: unknown, direction: CallDirection): CallHierarchyItem[] {
  if (!Array.isArray(result)) return [];
  return result.map((call) => (direction === "incoming" ? call?.from : call?.to)).filter((item): item is CallHierarchyItem => Boolean(item?.name && item?.uri));
}

export function lspLensIsReferences(command: unknown): boolean {
  return typeof command === "string" && /(^|\.)show_?references$|showReferences$|\.references$/i.test(command);
}

export function registerLspProviders(
  editor: MonacoEditorApi.IStandaloneCodeEditor,
  monaco: MonacoApi,
  lsp: LspTarget,
  callbacks: () => LspProviderCallbacks,
  ensureModel: (relativePath: string) => Promise<Uri | null>,
): IDisposable {
  const bridge = getBridge();
  const disposables: IDisposable[] = [];
  let disposed = false;
  if (!bridge?.workspaceLspRequest) return { dispose() {} };
  const { language } = lsp;
  const request = (method: LspMethod, position: { lineNumber: number; column: number }, extra?: Record<string, unknown>) => bridge.workspaceLspRequest!(
    lsp.root,
    lsp.path,
    lsp.language,
    method,
    { line: position.lineNumber - 1, character: position.column - 1 },
    extra,
  );
  const origin = { lineNumber: 1, column: 1 };
  const tooLarge = (model: MonacoEditorApi.ITextModel) => model.getLineCount() > LARGE_DOCUMENT_LINES;
  const isEditorModel = (model: { uri: { toString: () => string } }) => model.uri.toString() === editor.getModel()?.uri.toString();
  const toMonacoRange = (range: LspRange) => new monaco.Range(range.start.line + 1, range.start.character + 1, range.end.line + 1, range.end.character + 1);
  const toLspRange = (range: { startLineNumber: number; startColumn: number; endLineNumber: number; endColumn: number }) => ({
    start: { line: range.startLineNumber - 1, character: range.startColumn - 1 },
    end: { line: range.endLineNumber - 1, character: range.endColumn - 1 },
  });
  const locations = (result: any): any[] => result ? (Array.isArray(result) ? result : [result]) : [];
  const convertLocation = async (location: any): Promise<MonacoLanguagesApi.Location | null> => {
    const uri = location?.targetUri ?? location?.uri;
    const range = location?.targetSelectionRange ?? location?.targetRange ?? location?.range;
    if (typeof uri !== "string" || !range) return null;
    const relativePath = lspUriToRelativePath(uri, lsp.root);
    if (!relativePath) return null;
    const target = relativePath === lsp.path ? editor.getModel()?.uri : await ensureModel(relativePath);
    return target ? { uri: target, range: toMonacoRange(range) } : null;
  };
  const convertLocations = async (result: unknown) => (await Promise.all(locations(result).map(convertLocation))).filter((location): location is MonacoLanguagesApi.Location => location !== null);
  const locationProvider = (method: LspMethod) => async (model: MonacoEditorApi.ITextModel, position: { lineNumber: number; column: number }) => isEditorModel(model)
    ? convertLocations((await request(method, position).catch(() => null))?.result)
    : null;
  const ownEdits = (edit: unknown) => {
    const ordered = lspWorkspaceEditSteps(edit, lsp.root);
    if (ordered.steps.some((step) => step.kind !== "edit") && callbacks().onApplyWorkspaceSteps) {
      callbacks().onApplyWorkspaceSteps?.(ordered.steps, ordered.skipped);
      return [];
    }
    const { files, skipped } = lspWorkspaceEditFiles(edit, lsp.root);
    const others = files.filter((file) => file.path !== lsp.path);
    if (others.length > 0 || skipped > 0) callbacks().onApplyWorkspaceEdit?.(others, skipped);
    return files.find((file) => file.path === lsp.path)?.edits ?? [];
  };

  disposables.push(monaco.languages.registerHoverProvider(language, {
    provideHover: async (model, position) => {
      if (!isEditorModel(model)) return null;
      const hover = (await request("hover", position).catch(() => null))?.result as any;
      if (!hover?.contents) return null;
      return {
        contents: (Array.isArray(hover.contents) ? hover.contents : [hover.contents]).map((content: any) => ({
          value: typeof content === "string" ? content : content.value ?? String(content),
        })),
      };
    },
  }));
  disposables.push(monaco.languages.registerDefinitionProvider(language, { provideDefinition: locationProvider("definition") }));
  disposables.push(monaco.languages.registerTypeDefinitionProvider(language, { provideTypeDefinition: locationProvider("typeDefinition") }));
  disposables.push(monaco.languages.registerImplementationProvider(language, { provideImplementation: locationProvider("implementation") }));
  disposables.push(monaco.languages.registerReferenceProvider(language, { provideReferences: locationProvider("references") }));
  disposables.push(monaco.editor.registerLinkOpener({
    open: (resource) => {
      if (resource.scheme !== "file") return false;
      const target = lspUriToRelativePath(resource.with({ fragment: "" }).toString(), lsp.root);
      if (!target) return false;
      const match = /L(\d+)(?:[,:C](\d+))?/.exec(resource.fragment);
      callbacks().onNavigateLocation?.(target, match ? Number(match[1]) : 1, match?.[2] ? Number(match[2]) : 1);
      return true;
    },
  }));
  disposables.push(monaco.languages.registerRenameProvider(language, {
    provideRenameEdits: async (model, position, newName) => {
      if (!isEditorModel(model)) return { edits: [] };
      const response = await request("rename", position, { newName }).catch((reason) => ({ result: null, reason: reason instanceof Error ? reason.message : String(reason) }));
      if (!response?.result) return { edits: [], rejectReason: (response as { reason?: string })?.reason ?? "The language server cannot rename this symbol." };
      return { edits: ownEdits(response.result).map((edit) => ({ resource: model.uri, textEdit: { range: toMonacoRange(edit.range), text: edit.newText }, versionId: model.getVersionId() })) };
    },
    resolveRenameLocation: async (model, position) => {
      const word = model.getWordAtPosition(position);
      const fallback = word ? { range: new monaco.Range(position.lineNumber, word.startColumn, position.lineNumber, word.endColumn), text: word.word } : null;
      const result = (await request("prepareRename", position).catch(() => null))?.result as any;
      if (result === null) return { range: new monaco.Range(position.lineNumber, position.column, position.lineNumber, position.column), text: "", rejectReason: "This symbol cannot be renamed." };
      if (result?.range && typeof result.placeholder === "string") return { range: toMonacoRange(result.range), text: result.placeholder };
      if (result?.start && result?.end) {
        const range = toMonacoRange(result);
        return { range, text: model.getValueInRange(range) };
      }
      return fallback ?? { range: new monaco.Range(position.lineNumber, position.column, position.lineNumber, position.column), text: "", rejectReason: "Nothing to rename here." };
    },
  }));
  const applyEditCommand = `zorai.lsp.applyEdit.${editor.getId()}`;
  disposables.push(monaco.editor.registerCommand(applyEditCommand, (_accessor, edit: unknown) => {
    const edits = ownEdits(edit);
    if (edits.length === 0) return;
    editor.pushUndoStop();
    editor.executeEdits("lsp.codeAction", edits.map((textEdit) => ({ range: toMonacoRange(textEdit.range), text: textEdit.newText, forceMoveMarkers: true })));
    editor.pushUndoStop();
  }));
  const lspSeverity = (severity: number) => severity === monaco.MarkerSeverity.Error ? 1 : severity === monaco.MarkerSeverity.Warning ? 2 : severity === monaco.MarkerSeverity.Info ? 3 : 4;
  disposables.push(monaco.languages.registerCodeActionProvider(language, {
    provideCodeActions: async (model, range, context) => {
      if (!isEditorModel(model)) return { actions: [], dispose: () => {} };
      const diagnostics = context.markers.map((marker) => ({
        range: toLspRange(marker),
        message: marker.message,
        severity: lspSeverity(marker.severity),
        source: marker.source,
        code: typeof marker.code === "string" ? marker.code : marker.code?.value,
      }));
      const response = await request("codeAction", { lineNumber: range.startLineNumber, column: range.startColumn }, { range: toLspRange(range), diagnostics }).catch(() => null);
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
  const textEdits = (result: unknown) => (Array.isArray(result) ? result : []).map((edit: any) => ({ range: toMonacoRange(edit.range), text: edit.newText }));
  disposables.push(monaco.languages.registerDocumentFormattingEditProvider(language, {
    provideDocumentFormattingEdits: async (model, options) => isEditorModel(model)
      ? textEdits((await request("formatting", origin, { tabSize: options.tabSize, insertSpaces: options.insertSpaces }).catch(() => null))?.result)
      : [],
  }));
  disposables.push(monaco.languages.registerDocumentRangeFormattingEditProvider(language, {
    provideDocumentRangeFormattingEdits: async (model, range, options) => isEditorModel(model)
      ? textEdits((await request("rangeFormatting", origin, { range: toLspRange(range), tabSize: options.tabSize, insertSpaces: options.insertSpaces }).catch(() => null))?.result)
      : [],
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
  disposables.push(monaco.languages.registerDocumentSymbolProvider(language, {
    provideDocumentSymbols: async (model) => {
      if (!isEditorModel(model)) return [];
      const symbols: any[] = (await request("documentSymbol", origin).catch(() => null))?.result as any[] ?? [];
      return Array.isArray(symbols) ? symbols.filter((symbol) => symbol?.range || symbol?.location?.range).map(toDocumentSymbol) : [];
    },
  }));
  disposables.push(monaco.languages.registerInlayHintsProvider(language, {
    provideInlayHints: async (model, range) => {
      if (!isEditorModel(model) || tooLarge(model)) return { hints: [], dispose: () => {} };
      const result = (await request("inlayHint", origin, { range: toLspRange(range) }).catch(() => null))?.result;
      const hints = (Array.isArray(result) ? result : []).filter((hint: any) => hint?.position).map((hint: any) => ({
        label: lspInlayLabel(hint.label),
        position: { lineNumber: hint.position.line + 1, column: hint.position.character + 1 },
        kind: hint.kind === 2 ? monaco.languages.InlayHintKind.Parameter : monaco.languages.InlayHintKind.Type,
        paddingLeft: hint.paddingLeft === true,
        paddingRight: hint.paddingRight === true,
        tooltip: lspMarkup(hint.tooltip),
      }));
      return { hints, dispose: () => {} };
    },
  }));

  void request("serverCapabilities", origin).then((response) => {
    if (disposed) return;
    const capabilities = (response?.result ?? {}) as any;
    const completion = capabilities.completionProvider ?? {};
    const rawItems = new WeakMap<object, any>();
    disposables.push(monaco.languages.registerCompletionItemProvider(language, {
      triggerCharacters: Array.isArray(completion.triggerCharacters) && completion.triggerCharacters.length > 0 ? completion.triggerCharacters : [".", ":"],
      provideCompletionItems: async (model, position, context) => {
        if (!isEditorModel(model)) return { suggestions: [] };
        const response = await request("completion", position, context.triggerCharacter ? { triggerCharacter: context.triggerCharacter } : undefined).catch(() => null);
        const raw: any = response?.result;
        const items: any[] = Array.isArray(raw) ? raw : Array.isArray(raw?.items) ? raw.items : [];
        const word = model.getWordUntilPosition(position);
        const wordRange = new monaco.Range(position.lineNumber, word.startColumn, position.lineNumber, word.endColumn);
        const suggestions = items.slice(0, 500).map((item) => {
          const insert = lspCompletionInsert(item);
          const editRange = item.textEdit?.range ?? item.textEdit?.insert;
          const suggestion: MonacoLanguagesApi.CompletionItem = {
            label: typeof item.label === "string" ? item.label : item.label?.label ?? "completion",
            kind: Math.max(0, Math.min(27, Number(item.kind) || 1)),
            detail: item.detail,
            documentation: lspMarkup(item.documentation),
            insertText: insert.text,
            insertTextRules: insert.snippet ? monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet : undefined,
            range: editRange ? toMonacoRange(editRange) : wordRange,
            sortText: typeof item.sortText === "string" ? item.sortText : undefined,
            filterText: typeof item.filterText === "string" ? item.filterText : undefined,
            preselect: item.preselect === true,
            additionalTextEdits: Array.isArray(item.additionalTextEdits) ? item.additionalTextEdits.map((edit: any) => ({ range: toMonacoRange(edit.range), text: edit.newText })) : undefined,
          };
          rawItems.set(suggestion, item);
          return suggestion;
        });
        return { suggestions, incomplete: raw?.isIncomplete === true };
      },
      resolveCompletionItem: completion.resolveProvider ? async (suggestion) => {
        const raw = rawItems.get(suggestion);
        if (!raw) return suggestion;
        const resolved = (await request("completionResolve", origin, { item: raw }).catch(() => null))?.result as any;
        if (!resolved) return suggestion;
        return {
          ...suggestion,
          detail: resolved.detail ?? suggestion.detail,
          documentation: lspMarkup(resolved.documentation) ?? suggestion.documentation,
          additionalTextEdits: Array.isArray(resolved.additionalTextEdits)
            ? resolved.additionalTextEdits.map((edit: any) => ({ range: toMonacoRange(edit.range), text: edit.newText }))
            : suggestion.additionalTextEdits,
        };
      } : undefined,
    }));
    const signature = capabilities.signatureHelpProvider;
    if (signature) {
      disposables.push(monaco.languages.registerSignatureHelpProvider(language, {
        signatureHelpTriggerCharacters: signature.triggerCharacters ?? ["(", ","],
        signatureHelpRetriggerCharacters: signature.retriggerCharacters ?? [],
        provideSignatureHelp: async (model, position, _token, context) => {
          if (!isEditorModel(model)) return null;
          const result = (await request("signatureHelp", position, {
            context: { triggerKind: context.triggerKind, triggerCharacter: context.triggerCharacter, isRetrigger: context.isRetrigger },
          }).catch(() => null))?.result as any;
          if (!result || !Array.isArray(result.signatures) || result.signatures.length === 0) return null;
          return {
            value: {
              signatures: result.signatures.map((entry: any) => ({
                label: String(entry.label ?? ""),
                documentation: lspMarkup(entry.documentation),
                parameters: (Array.isArray(entry.parameters) ? entry.parameters : []).map((parameter: any) => ({ label: parameter.label, documentation: lspMarkup(parameter.documentation) })),
                activeParameter: typeof entry.activeParameter === "number" ? entry.activeParameter : undefined,
              })),
              activeSignature: Number(result.activeSignature) || 0,
              activeParameter: Number(result.activeParameter) || 0,
            },
            dispose: () => {},
          };
        },
      }));
    }
    if (capabilities.foldingRangeProvider) {
      disposables.push(monaco.languages.registerFoldingRangeProvider(language, {
        provideFoldingRanges: async (model) => {
          if (!isEditorModel(model) || tooLarge(model)) return null;
          const result = (await request("foldingRange", origin).catch(() => null))?.result;
          return (Array.isArray(result) ? result : []).filter((range: any) => Number.isInteger(range?.startLine) && range.endLine > range.startLine).map((range: any) => ({
            start: range.startLine + 1,
            end: range.endLine + 1,
            kind: range.kind === "comment" ? monaco.languages.FoldingRangeKind.Comment : range.kind === "imports" ? monaco.languages.FoldingRangeKind.Imports : range.kind === "region" ? monaco.languages.FoldingRangeKind.Region : undefined,
          }));
        },
      }));
    }
    if (capabilities.linkedEditingRangeProvider && LINKED_EDITING_LANGUAGES.has(language)) {
      disposables.push(monaco.languages.registerLinkedEditingRangeProvider(language, {
        provideLinkedEditingRanges: async (model, position) => {
          if (!isEditorModel(model)) return null;
          const result = (await request("linkedEditingRange", position).catch(() => null))?.result as any;
          if (!Array.isArray(result?.ranges) || result.ranges.length === 0) return null;
          let wordPattern: RegExp | undefined;
          try { wordPattern = typeof result.wordPattern === "string" ? new RegExp(result.wordPattern) : undefined; } catch { wordPattern = undefined; }
          return { ranges: result.ranges.map(toMonacoRange), wordPattern };
        },
      }));
    }
    const onType = capabilities.documentOnTypeFormattingProvider;
    if (onType?.firstTriggerCharacter) {
      disposables.push(monaco.languages.registerOnTypeFormattingEditProvider(language, {
        autoFormatTriggerCharacters: [onType.firstTriggerCharacter, ...(Array.isArray(onType.moreTriggerCharacter) ? onType.moreTriggerCharacter : [])],
        provideOnTypeFormattingEdits: async (model, position, ch, options) => isEditorModel(model)
          ? textEdits((await request("onTypeFormatting", position, { ch, tabSize: options.tabSize, insertSpaces: options.insertSpaces }).catch(() => null))?.result)
          : [],
      }));
    }
    if (capabilities.codeLensProvider && callbacks().codeLens === true) {
      const serverCommands = new Set<string>(Array.isArray(capabilities.executeCommandProvider?.commands) ? capabilities.executeCommandProvider.commands : []);
      const lensCommand = `zorai.lsp.codeLens.${editor.getId()}`;
      disposables.push(monaco.editor.registerCommand(lensCommand, (_accessor, command: { command: string; arguments?: unknown[] }, line: number, column: number) => {
        if (lspLensIsReferences(command.command)) {
          editor.setPosition({ lineNumber: line, column });
          editor.focus();
          editor.trigger("zorai.codeLens", "editor.action.goToReferences", null);
          return;
        }
        void request("executeCommand", origin, { command: command.command, arguments: command.arguments ?? [] }).catch(() => null);
      }));
      const toLens = (lens: any): MonacoLanguagesApi.CodeLens | null => {
        if (!lens?.range) return null;
        const command = lens.command;
        const usable = command && (lspLensIsReferences(command.command) || serverCommands.has(command.command));
        if (command && !usable) return null;
        return {
          range: toMonacoRange(lens.range),
          command: command ? { id: lensCommand, title: String(command.title ?? ""), arguments: [command, lens.range.start.line + 1, lens.range.start.character + 1] } : undefined,
        };
      };
      const rawLenses = new WeakMap<object, any>();
      disposables.push(monaco.languages.registerCodeLensProvider(language, {
        provideCodeLenses: async (model) => {
          if (!isEditorModel(model) || tooLarge(model)) return { lenses: [], dispose: () => {} };
          const result = (await request("codeLens", origin).catch(() => null))?.result;
          const lenses: MonacoLanguagesApi.CodeLens[] = [];
          for (const raw of Array.isArray(result) ? result : []) {
            const lens = toLens(raw);
            if (!lens) continue;
            rawLenses.set(lens, raw);
            lenses.push(lens);
          }
          return { lenses, dispose: () => {} };
        },
        resolveCodeLens: capabilities.codeLensProvider.resolveProvider ? async (_model, lens) => {
          const raw = rawLenses.get(lens);
          if (!raw || lens.command) return lens;
          const resolved = (await request("codeLensResolve", origin, { lens: raw }).catch(() => null))?.result;
          return toLens(resolved) ?? { ...lens, command: { id: "", title: "" } };
        } : undefined,
      }));
    }
    const legend = capabilities.semanticTokensProvider?.legend;
    if (legend && capabilities.semanticTokensProvider?.full) {
      disposables.push(monaco.languages.registerDocumentSemanticTokensProvider(language, {
        getLegend: () => ({ tokenTypes: legend.tokenTypes ?? [], tokenModifiers: legend.tokenModifiers ?? [] }),
        provideDocumentSemanticTokens: async (model) => {
          if (!isEditorModel(model) || tooLarge(model)) return null;
          const result = (await request("semanticTokens", origin).catch(() => null))?.result as any;
          if (!Array.isArray(result?.data)) return null;
          return { data: new Uint32Array(result.data), resultId: typeof result.resultId === "string" ? result.resultId : undefined };
        },
        releaseDocumentSemanticTokens: () => {},
      }));
    }
  }).catch(() => undefined);

  return {
    dispose() {
      disposed = true;
      disposables.splice(0).forEach((disposable) => disposable.dispose());
    },
  };
}
