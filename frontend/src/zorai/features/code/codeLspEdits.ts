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

export type LspEditStep =
  | { kind: "edit"; path: string; edits: LspTextEdit[] }
  | { kind: "create"; path: string; overwrite: boolean }
  | { kind: "rename"; path: string; newPath: string; overwrite: boolean }
  | { kind: "delete"; path: string; recursive: boolean };

export function lspWorkspaceEditSteps(edit: unknown, root: string): { steps: LspEditStep[]; skipped: number } {
  const value = edit as { documentChanges?: Array<Record<string, any>> } | null;
  if (!Array.isArray(value?.documentChanges)) {
    const { files, skipped } = lspWorkspaceEditFiles(edit, root);
    return { steps: files.map((file) => ({ kind: "edit" as const, ...file })), skipped };
  }
  const steps: LspEditStep[] = [];
  let skipped = 0;
  const local = (uri: unknown) => (typeof uri === "string" ? lspUriToRelativePath(uri, root) : null);
  for (const change of value.documentChanges) {
    if (change?.textDocument && Array.isArray(change.edits)) {
      const path = local(change.textDocument.uri);
      if (path) steps.push({ kind: "edit", path, edits: change.edits as LspTextEdit[] });
      else skipped += 1;
    } else if (change?.kind === "create" && local(change.uri)) {
      steps.push({ kind: "create", path: local(change.uri)!, overwrite: change.options?.overwrite === true });
    } else if (change?.kind === "rename" && local(change.oldUri) && local(change.newUri)) {
      steps.push({ kind: "rename", path: local(change.oldUri)!, newPath: local(change.newUri)!, overwrite: change.options?.overwrite === true });
    } else if (change?.kind === "delete" && local(change.uri)) {
      steps.push({ kind: "delete", path: local(change.uri)!, recursive: change.options?.recursive === true });
    } else {
      skipped += 1;
    }
  }
  return { steps, skipped };
}

export function describeLspEditSteps(steps: LspEditStep[]): string[] {
  return steps.filter((step) => step.kind !== "edit").map((step) => (
    step.kind === "create" ? `Create ${step.path}` : step.kind === "rename" ? `Rename ${step.path} → ${step.newPath}` : `Delete ${step.path}`
  ));
}
