import type { Monaco } from "@monaco-editor/react";
import { getBridge } from "@/lib/bridge";
import { lspCompletionInsert, lspMarkup } from "./lspProviders";
import type { NotebookLspSession } from "./notebookLsp";

type CellContext = { root: string; path: string; python: () => string | null; cell: number; lsp: NotebookLspSession | null };
type LspPosition = { line: number; character: number };
type LspRangeLike = { start: LspPosition; end: LspPosition };
type KernelCompletion = { matches: string[]; cursor_start: number; cursor_end: number; types: Array<string | null> };
type KindName = "Function" | "Class" | "Module" | "Keyword" | "Property" | "Variable";

const cellContexts = new Map<string, CellContext>();
const registeredLanguages = new Set<string>();

export function completionKindName(type: string | null | undefined): KindName {
  if (type === "function" || type === "method") return "Function";
  if (type === "class" || type === "type") return "Class";
  if (type === "module") return "Module";
  if (type === "keyword" || type === "magic") return "Keyword";
  if (type === "property" || type === "attribute" || type === "path") return "Property";
  return "Variable";
}

export function notebookSuggestions(result: KernelCompletion): Array<{ label: string; kind: KindName; sortText: string }> {
  const seen = new Set<string>();
  return result.matches.flatMap((match, index) => {
    if (seen.has(match)) return [];
    seen.add(match);
    return [{ label: match, kind: completionKindName(result.types[index]), sortText: String(index).padStart(5, "0") }];
  });
}

async function cellLsp(context: CellContext, position: { lineNumber: number; column: number }) {
  const request = getBridge()?.workspaceLspRequest;
  if (!context.lsp || !request) return null;
  const shadow = await context.lsp.sync();
  const start = shadow?.starts[context.cell] ?? -1;
  if (start < 0) return null;
  const at = { line: start + position.lineNumber - 1, character: position.column - 1 };
  const toCellRange = (range: LspRangeLike | undefined) => {
    if (!range || range.start.line < start || range.end.line < start) return undefined;
    return { startLineNumber: range.start.line - start + 1, startColumn: range.start.character + 1, endLineNumber: range.end.line - start + 1, endColumn: range.end.character + 1 };
  };
  const ask = (method: "completion" | "hover" | "signatureHelp", extra?: Record<string, unknown>) => request(context.root, context.lsp!.path, "python", method, at, extra).then((response: any) => response?.result ?? null).catch(() => null);
  return { ask, toCellRange };
}

export function registerNotebookCell(monaco: Monaco, modelUri: string, language: string, context: CellContext): () => void {
  cellContexts.set(modelUri, context);
  if (!registeredLanguages.has(language)) {
    registeredLanguages.add(language);
    monaco.languages.registerCompletionItemProvider(language, {
      triggerCharacters: [".", "(", "[", "'", '"'],
      provideCompletionItems: async (model, position, completionContext) => {
        const cell = cellContexts.get(model.uri.toString());
        if (!cell) return { suggestions: [] };
        const word = model.getWordUntilPosition(position);
        const wordRange = { startLineNumber: position.lineNumber, startColumn: word.startColumn, endLineNumber: position.lineNumber, endColumn: word.endColumn };
        const fromLsp = (async () => {
          const lsp = await cellLsp(cell, position);
          if (!lsp) return [];
          const raw: any = await lsp.ask("completion", completionContext.triggerCharacter ? { triggerCharacter: completionContext.triggerCharacter } : undefined);
          const items: any[] = Array.isArray(raw) ? raw : Array.isArray(raw?.items) ? raw.items : [];
          return items.slice(0, 300).map((item) => {
            const insert = lspCompletionInsert(item);
            return {
              label: typeof item.label === "string" ? item.label : item.label?.label ?? "completion",
              kind: Math.max(0, Math.min(27, Number(item.kind) || 1)),
              detail: item.detail,
              documentation: lspMarkup(item.documentation),
              insertText: insert.text,
              insertTextRules: insert.snippet ? monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet : undefined,
              range: lsp.toCellRange(item.textEdit?.range ?? item.textEdit?.insert) ?? wordRange,
              sortText: typeof item.sortText === "string" ? `0${item.sortText}` : undefined,
              filterText: typeof item.filterText === "string" ? item.filterText : undefined,
            };
          });
        })().catch(() => []);
        const fromKernel = (async () => {
          const complete = getBridge()?.notebookComplete;
          if (!complete) return [];
          const result = await complete(cell.root, cell.path, model.getValue(), model.getOffsetAt(position), cell.python());
          const start = model.getPositionAt(result.cursor_start);
          const end = model.getPositionAt(result.cursor_end);
          const range = { startLineNumber: start.lineNumber, startColumn: start.column, endLineNumber: end.lineNumber, endColumn: end.column };
          return notebookSuggestions(result).map((item) => ({ label: item.label, insertText: item.label, kind: monaco.languages.CompletionItemKind[item.kind], sortText: `1${item.sortText}`, range }));
        })().catch(() => []);
        const [lspItems, kernelItems] = await Promise.all([fromLsp, fromKernel]);
        const seen = new Set(lspItems.map((item) => item.label));
        return { suggestions: [...lspItems, ...kernelItems.filter((item) => !seen.has(item.label.split(".").pop() ?? item.label) && !seen.has(item.label))] };
      },
    });
    monaco.languages.registerHoverProvider(language, {
      provideHover: async (model, position) => {
        const cell = cellContexts.get(model.uri.toString());
        const lsp = cell ? await cellLsp(cell, position) : null;
        const result: any = lsp ? await lsp.ask("hover") : null;
        if (!result?.contents) return null;
        const contents = (Array.isArray(result.contents) ? result.contents : [result.contents]).map((part: any) => ({ value: typeof part === "string" ? part : part.language ? `\`\`\`${part.language}\n${part.value}\n\`\`\`` : lspMarkup(part) ?? "" }));
        return { contents, range: lsp?.toCellRange(result.range) };
      },
    });
    monaco.languages.registerSignatureHelpProvider(language, {
      signatureHelpTriggerCharacters: ["(", ","],
      signatureHelpRetriggerCharacters: [")"],
      provideSignatureHelp: async (model, position) => {
        const cell = cellContexts.get(model.uri.toString());
        const lsp = cell ? await cellLsp(cell, position) : null;
        const result: any = lsp ? await lsp.ask("signatureHelp") : null;
        if (!result?.signatures?.length) return null;
        return { value: { signatures: result.signatures, activeSignature: result.activeSignature ?? 0, activeParameter: result.activeParameter ?? 0 }, dispose() {} };
      },
    });
  }
  return () => { cellContexts.delete(modelUri); };
}
