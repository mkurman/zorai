export type LspPosition = { line: number; character: number };
export type LspRange = { start: LspPosition; end: LspPosition };
export type LspTextEdit = { range: LspRange; newText: string };
export type LspFileEdits = { path: string; edits: LspTextEdit[] };

export function lspUriToRelativePath(uri: string, root: string): string | null {
  let absolute: string;
  try {
    absolute = decodeURIComponent(new URL(uri).pathname).replace(/^\/[A-Za-z]:\//, (value) => value.slice(1));
  } catch {
    return null;
  }
  const rootNormalized = root.replace(/\\/g, "/").replace(/\/$/, "");
  absolute = absolute.replace(/\\/g, "/");
  if (!absolute.startsWith(`${rootNormalized}/`)) return null;
  return absolute.slice(rootNormalized.length + 1);
}

export function lspWorkspaceEditFiles(edit: unknown, root: string): { files: LspFileEdits[]; skipped: number } {
  const byPath = new Map<string, LspTextEdit[]>();
  let skipped = 0;
  const add = (uri: unknown, edits: unknown) => {
    const path = typeof uri === "string" ? lspUriToRelativePath(uri, root) : null;
    if (!path || !Array.isArray(edits)) {
      skipped += 1;
      return;
    }
    byPath.set(path, [...(byPath.get(path) ?? []), ...(edits as LspTextEdit[])]);
  };
  const value = edit as { changes?: Record<string, unknown>; documentChanges?: Array<{ textDocument?: { uri?: string }; edits?: unknown }> } | null;
  if (Array.isArray(value?.documentChanges)) {
    for (const change of value.documentChanges) {
      if (change?.textDocument && "edits" in change) add(change.textDocument.uri, change.edits);
      else skipped += 1;
    }
  } else if (value?.changes) {
    for (const [uri, edits] of Object.entries(value.changes)) add(uri, edits);
  }
  return { files: [...byPath].map(([path, edits]) => ({ path, edits })), skipped };
}

export function applyLspTextEdits(content: string, edits: LspTextEdit[]): string {
  const lineStarts = [0];
  for (let index = 0; index < content.length; index += 1) if (content[index] === "\n") lineStarts.push(index + 1);
  const offset = ({ line, character }: LspPosition) => {
    if (line >= lineStarts.length) return content.length;
    const lineEnd = line + 1 < lineStarts.length ? lineStarts[line + 1] - 1 : content.length;
    const contentEnd = lineEnd > lineStarts[line] && content[lineEnd - 1] === "\r" ? lineEnd - 1 : lineEnd;
    return Math.min(lineStarts[line] + character, contentEnd);
  };
  return edits
    .map((edit) => ({ start: offset(edit.range.start), end: offset(edit.range.end), text: edit.newText }))
    .sort((a, b) => b.start - a.start || b.end - a.end)
    .reduce((text, edit) => text.slice(0, edit.start) + edit.text + text.slice(edit.end), content);
}
