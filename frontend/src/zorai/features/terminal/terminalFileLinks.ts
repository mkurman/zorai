export type TerminalFileLink = { start: number; end: number; path: string; line: number; column: number };

type FileLinkHandler = (link: { path: string; line: number; column: number }) => void;

let handler: FileLinkHandler | null = null;

export function setTerminalFileLinkHandler(next: FileLinkHandler | null): () => void {
  handler = next;
  return () => {
    if (handler === next) handler = null;
  };
}

export function terminalFileLinkHandler(): FileLinkHandler | null {
  return handler;
}

const PATH = String.raw`(?:[A-Za-z]:[\\/]|\.{0,2}[\\/])?(?:[\w@~.-]+[\\/])*[\w@~.-]*[\w-]\.[A-Za-z][A-Za-z0-9]{0,7}`;
const PATTERNS: Array<{ regex: RegExp; path: number; line: number; column: number }> = [
  { regex: new RegExp(String.raw`File "(${PATH})", line (\d+)`, "g"), path: 1, line: 2, column: 0 },
  { regex: new RegExp(String.raw`(${PATH})\((\d+),(\d+)\)`, "g"), path: 1, line: 2, column: 3 },
  { regex: new RegExp(String.raw`(${PATH}):(\d+)(?::(\d+))?`, "g"), path: 1, line: 2, column: 3 },
];
const URL = /\b[a-z][a-z0-9+.-]*:\/\/\S+/gi;

export function findTerminalFileLinks(text: string): TerminalFileLink[] {
  const blocked: Array<[number, number]> = [...text.matchAll(URL)].map((match) => [match.index ?? 0, (match.index ?? 0) + match[0].length]);
  const links: TerminalFileLink[] = [];
  const overlaps = (start: number, end: number) => [...blocked, ...links.map((link) => [link.start, link.end] as [number, number])]
    .some(([from, to]) => start < to && end > from);
  for (const pattern of PATTERNS) {
    for (const match of text.matchAll(pattern.regex)) {
      const start = match.index ?? 0;
      const end = start + match[0].length;
      const line = Number(match[pattern.line]);
      if (!Number.isFinite(line) || line < 1 || overlaps(start, end)) continue;
      const column = pattern.column && match[pattern.column] ? Number(match[pattern.column]) : 1;
      links.push({ start, end, path: match[pattern.path], line, column: Math.max(1, column) });
    }
  }
  return links.sort((a, b) => a.start - b.start);
}

export function workspaceRelativeLinkPath(path: string, root: string): string | null {
  const normalized = path.replace(/\\/g, "/");
  const normalizedRoot = root.replace(/\\/g, "/").replace(/\/+$/, "");
  if (/^(?:[A-Za-z]:)?\//.test(normalized)) {
    return normalized.startsWith(`${normalizedRoot}/`) ? normalized.slice(normalizedRoot.length + 1) : null;
  }
  const parts: string[] = [];
  for (const part of normalized.split("/")) {
    if (!part || part === ".") continue;
    if (part === "..") {
      if (parts.length === 0) return null;
      parts.pop();
    } else parts.push(part);
  }
  return parts.join("/") || null;
}
