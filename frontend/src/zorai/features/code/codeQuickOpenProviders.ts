import { lspUriToRelativePath } from "./codeLspEdits";

export type QuickOpenMode = "files" | "commands" | "symbols" | "workspaceSymbols" | "line" | "editors";

export type QuickOpenSymbol = { name: string; kind: string; container: string; path: string; line: number; column: number; endLine?: number };

const PREFIXES: Array<{ prefix: string; mode: QuickOpenMode }> = [
  { prefix: "edt ", mode: "editors" },
  { prefix: ">", mode: "commands" },
  { prefix: "@", mode: "symbols" },
  { prefix: "#", mode: "workspaceSymbols" },
  { prefix: ":", mode: "line" },
];

export const QUICK_OPEN_PLACEHOLDER: Record<QuickOpenMode, string> = {
  files: "Search files by name (append :line to jump) — > commands, @ symbols, # workspace symbols, : line, edt open editors",
  commands: "Type a command",
  symbols: "Go to a symbol in this file",
  workspaceSymbols: "Search symbols in the workspace",
  line: "Type a line number, optionally :column",
  editors: "Search open editors",
};

export function parseQuickOpenMode(value: string): { mode: QuickOpenMode; text: string } {
  for (const { prefix, mode } of PREFIXES) if (value.startsWith(prefix)) return { mode, text: value.slice(prefix.length).trimStart() };
  return { mode: "files", text: value };
}

export function quickOpenLineTarget(text: string): { line: number; column: number } | null {
  const match = text.trim().match(/^(\d+)(?:[:,](\d+))?$/);
  return match ? { line: Number(match[1]), column: match[2] ? Number(match[2]) : 1 } : null;
}

const SYMBOL_KINDS = ["file", "module", "namespace", "package", "class", "method", "property", "field", "constructor", "enum", "interface", "function", "variable", "constant", "string", "number", "boolean", "array", "object", "key", "null", "enum member", "struct", "event", "operator", "type parameter"];

export function lspSymbolKind(kind: unknown): string {
  return SYMBOL_KINDS[Number(kind) - 1] ?? "symbol";
}

type LspRange = { start?: { line?: number; character?: number }; end?: { line?: number; character?: number } };
type LspSymbol = {
  name?: string;
  kind?: number;
  containerName?: string;
  range?: LspRange;
  selectionRange?: LspRange;
  location?: { uri?: string; range?: LspRange };
  children?: LspSymbol[];
};

export function flattenLspSymbols(result: unknown, path: string, root: string): QuickOpenSymbol[] {
  const symbols: QuickOpenSymbol[] = [];
  const visit = (symbol: LspSymbol, container: string) => {
    if (!symbol?.name) return;
    const uriPath = symbol.location?.uri ? lspUriToRelativePath(symbol.location.uri, root) : path;
    const start = (symbol.selectionRange ?? symbol.range ?? symbol.location?.range)?.start;
    const end = (symbol.range ?? symbol.location?.range)?.end;
    if (uriPath) {
      symbols.push({
        name: symbol.name,
        kind: lspSymbolKind(symbol.kind),
        container: symbol.containerName ?? container,
        path: uriPath,
        line: (start?.line ?? 0) + 1,
        column: (start?.character ?? 0) + 1,
        ...(end?.line !== undefined ? { endLine: end.line + 1 } : {}),
      });
    }
    for (const child of symbol.children ?? []) visit(child, container ? `${container}.${symbol.name}` : symbol.name);
  };
  for (const symbol of Array.isArray(result) ? result as LspSymbol[] : []) visit(symbol, "");
  return symbols;
}

export function symbolAtLine(symbols: readonly QuickOpenSymbol[], line: number): QuickOpenSymbol | null {
  const ranged = symbols.filter((symbol) => symbol.endLine !== undefined && symbol.line <= line && line <= symbol.endLine);
  if (ranged.length > 0) return ranged.reduce((best, symbol) => ((symbol.endLine! - symbol.line) < (best.endLine! - best.line) ? symbol : best));
  if (symbols.some((symbol) => symbol.endLine !== undefined)) return null;
  return symbols.filter((symbol) => symbol.line <= line).reduce<QuickOpenSymbol | null>((best, symbol) => (!best || symbol.line >= best.line ? symbol : best), null);
}
