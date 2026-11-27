import { getBridge } from "@/lib/bridge";

export type NotebookShadow = { text: string; starts: number[]; lineCounts: number[] };
export type ShadowDiagnostic = { cell: number; startLine: number; startColumn: number; endLine: number; endColumn: number; message: string; severity: number; source: string | null };

const CELL_GAP = 2;

export function notebookShadowPath(notebookPath: string): string {
  return `${notebookPath}.zorai-notebook.py`;
}

function sanitizeLine(line: string): string {
  const match = /^(\s*)([%!?]|\w+\?\??\s*$)/.exec(line);
  return match ? `${match[1]}pass` : line;
}

export function buildNotebookShadow(cells: Array<{ code: boolean; source: string }>): NotebookShadow {
  const lines: string[] = [];
  const starts: number[] = [];
  const lineCounts: number[] = [];
  for (const cell of cells) {
    if (!cell.code) {
      starts.push(-1);
      lineCounts.push(0);
      continue;
    }
    const cellLines = cell.source.split("\n").map(sanitizeLine);
    starts.push(lines.length);
    lineCounts.push(cellLines.length);
    lines.push(...cellLines, ...Array<string>(CELL_GAP).fill(""));
  }
  return { text: lines.join("\n"), starts, lineCounts };
}

export function shadowLineToCell(shadow: NotebookShadow, line: number): { cell: number; line: number } | null {
  for (let cell = 0; cell < shadow.starts.length; cell += 1) {
    const start = shadow.starts[cell];
    if (start >= 0 && line >= start && line < start + shadow.lineCounts[cell]) return { cell, line: line - start };
  }
  return null;
}

export function mapShadowDiagnostics(shadow: NotebookShadow, diagnostics: ZoraiLspDiagnostic[]): ShadowDiagnostic[] {
  return diagnostics.flatMap((diagnostic) => {
    const start = shadowLineToCell(shadow, diagnostic.startLine - 1);
    if (!start) return [];
    const end = shadowLineToCell(shadow, diagnostic.endLine - 1);
    const sameCell = end && end.cell === start.cell;
    return [{
      cell: start.cell,
      startLine: start.line + 1,
      startColumn: diagnostic.startColumn,
      endLine: sameCell ? end.line + 1 : start.line + 1,
      endColumn: sameCell ? diagnostic.endColumn : diagnostic.startColumn + 1,
      message: diagnostic.message,
      severity: diagnostic.severity,
      source: diagnostic.source,
    }];
  });
}

export type NotebookLspSession = {
  path: string;
  sync: () => Promise<NotebookShadow | null>;
  current: () => NotebookShadow | null;
  close: () => void;
};

export function createNotebookLspSession(root: string, notebookPath: string, readCells: () => Array<{ code: boolean; source: string }>): NotebookLspSession {
  const path = notebookShadowPath(notebookPath);
  let version = 0;
  let sent: string | null = null;
  let shadow: NotebookShadow | null = null;
  let available = true;
  let inflight: Promise<NotebookShadow | null> | null = null;
  const sync = async (): Promise<NotebookShadow | null> => {
    const open = getBridge()?.workspaceLspOpen;
    if (!open || !available) return null;
    const next = buildNotebookShadow(readCells());
    if (next.text === sent && shadow) return shadow;
    if (inflight) await inflight;
    inflight = (async () => {
      version += 1;
      const result = await open(root, path, "python", next.text, version).catch(() => null);
      if (!result?.available) {
        available = false;
        return null;
      }
      sent = next.text;
      shadow = next;
      return next;
    })();
    try {
      return await inflight;
    } finally {
      inflight = null;
    }
  };
  return {
    path,
    sync,
    current: () => shadow,
    close: () => { if (sent !== null) void getBridge()?.workspaceLspClose?.(root, path, "python"); },
  };
}
