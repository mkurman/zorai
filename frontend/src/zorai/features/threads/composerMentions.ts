import type { CodeProblem } from "@/zorai/features/code/codeProblems";

export type MentionKind = "file" | "problems" | "diff" | "staged" | "terminal" | "folder" | "symbol" | "commit" | "web" | "codebase" | "prefix";

export type MentionOption = { kind: MentionKind; id: string; label: string; detail: string; path?: string; line?: number };

export type MentionCommit = { hash: string; shortHash: string; subject: string; date: string };
export type MentionSymbol = { name: string; kind: string; path: string; line: number };

export const CONTEXT_MENTIONS: MentionOption[] = [
  { kind: "problems", id: "@problems", label: "@problems", detail: "Errors and warnings from the code editor" },
  { kind: "diff", id: "@diff", label: "@diff", detail: "Uncommitted changes in the working tree" },
  { kind: "staged", id: "@staged", label: "@staged", detail: "Changes staged for the next commit" },
  { kind: "terminal", id: "@terminal", label: "@terminal", detail: "Recent output from this thread's terminal" },
  { kind: "prefix", id: "folder:", label: "@folder", detail: "Files inside a folder" },
  { kind: "prefix", id: "symbol:", label: "@symbol", detail: "A function, class or type and its code" },
  { kind: "prefix", id: "commit:", label: "@commit", detail: "A commit's message and changed files" },
  { kind: "prefix", id: "web:", label: "@web", detail: "A web page or docs URL for the agent to read" },
  { kind: "prefix", id: "codebase:", label: "@codebase", detail: "Code that matches a meaning, e.g. @codebase:retry-failed-uploads" },
];

const FOLDER_LISTING_MAX = 300;

const MAX_FILE_OPTIONS = 8;
export const MENTION_CONTENT_MAX_CHARS = 40_000;

export type MentionQuery = { query: string; start: number; end: number };

export function mentionQueryAt(text: string, caret: number): MentionQuery | null {
  const before = text.slice(0, caret);
  const match = /(^|\s)@([^\s@]*)$/.exec(before);
  if (!match) return null;
  const start = caret - match[2].length - 1;
  return { query: match[2], start, end: caret };
}

export function removeMentionQuery(text: string, mention: MentionQuery): { text: string; caret: number } {
  const after = text.slice(mention.end).replace(/^ /, "");
  return { text: `${text.slice(0, mention.start)}${after}`, caret: mention.start };
}

function fileScore(path: string, query: string): number {
  const lowerPath = path.toLowerCase();
  const name = lowerPath.slice(lowerPath.lastIndexOf("/") + 1);
  const lowerQuery = query.toLowerCase();
  if (name.startsWith(lowerQuery)) return 0;
  if (name.includes(lowerQuery)) return 1;
  if (lowerPath.includes(lowerQuery)) return 2;
  let index = 0;
  for (const char of lowerPath) {
    if (char === lowerQuery[index]) index += 1;
    if (index === lowerQuery.length) return 3;
  }
  return -1;
}

export function folderPaths(files: string[]): string[] {
  const folders = new Set<string>();
  for (const file of files) {
    let slash = file.lastIndexOf("/");
    while (slash > 0) {
      folders.add(file.slice(0, slash));
      slash = file.lastIndexOf("/", slash - 1);
    }
  }
  return [...folders].sort();
}

function rankPaths(paths: string[], query: string): string[] {
  if (!query) return paths.slice(0, MAX_FILE_OPTIONS);
  return paths
    .map((path) => ({ path, score: fileScore(path, query) }))
    .filter((entry) => entry.score >= 0)
    .sort((a, b) => a.score - b.score || a.path.length - b.path.length)
    .slice(0, MAX_FILE_OPTIONS)
    .map((entry) => entry.path);
}

export function webMentionUrl(text: string): string | null {
  const candidate = text.trim();
  if (!candidate) return null;
  try {
    const url = new URL(/^[a-z][a-z0-9+.-]*:\/\//i.test(candidate) ? candidate : `https://${candidate}`);
    return (url.protocol === "https:" || url.protocol === "http:") && url.hostname.includes(".") ? url.toString() : null;
  } catch {
    return null;
  }
}

export function mentionOptions(query: string, files: string[], extra: { commits?: MentionCommit[]; symbols?: MentionSymbol[] } = {}): MentionOption[] {
  const colon = query.indexOf(":");
  if (colon > 0) {
    const mode = query.slice(0, colon).toLowerCase();
    const rest = query.slice(colon + 1);
    if (mode === "folder") return rankPaths(folderPaths(files), rest).map((path) => ({ kind: "folder", id: path, label: `${path}/`, detail: "" }));
    if (mode === "commit") {
      const needle = rest.toLowerCase();
      return (extra.commits ?? [])
        .filter((commit) => !needle || commit.hash.startsWith(needle) || commit.subject.toLowerCase().includes(needle))
        .slice(0, MAX_FILE_OPTIONS)
        .map((commit) => ({ kind: "commit", id: commit.hash, label: `${commit.shortHash} ${commit.subject}`, detail: commit.date }));
    }
    if (mode === "symbol") return (extra.symbols ?? []).slice(0, MAX_FILE_OPTIONS).map((symbol) => ({ kind: "symbol", id: `${symbol.path}:${symbol.line}:${symbol.name}`, label: symbol.name, detail: `${symbol.kind} · ${symbol.path}:${symbol.line}`, path: symbol.path, line: symbol.line }));
    if (mode === "codebase") {
      const meaning = codebaseQuery(rest);
      return meaning ? [{ kind: "codebase", id: meaning, label: `codebase: ${meaning}`, detail: "Semantic matches from this workspace's code index" }] : [];
    }
    if (mode === "web") {
      const url = webMentionUrl(rest);
      return url ? [{ kind: "web", id: url, label: url, detail: "The agent will read this page" }] : [];
    }
  }
  const lowerQuery = query.toLowerCase();
  const contexts = CONTEXT_MENTIONS.filter((option) => option.label.slice(1).startsWith(lowerQuery));
  if (!query) return [...contexts, ...files.slice(0, MAX_FILE_OPTIONS).map(fileOption)];
  return [...contexts, ...rankPaths(files, query).map(fileOption)];
}

export function codebaseQuery(raw: string): string {
  return raw.replace(/[-_]+/g, " ").trim();
}

export function formatCodebaseHits(query: string, hits: Array<{ title: string; snippet: string; score: number }>): string {
  if (hits.length === 0) return `No code in the semantic index matched "${query}".`;
  return [`Code matching "${query}" (semantic index):`, ...hits.map((hit) => `\n### ${hit.title} (${Math.round(hit.score * 100)}%)\n\`\`\`\n${hit.snippet}\n\`\`\``)].join("\n");
}

export function folderListing(folder: string, files: string[]): string {
  const inside = files.filter((file) => file.startsWith(`${folder}/`));
  if (inside.length === 0) return `${folder}/ has no indexed files.`;
  const shown = inside.slice(0, FOLDER_LISTING_MAX).map((file) => file.slice(folder.length + 1));
  const more = inside.length > FOLDER_LISTING_MAX ? `\n… and ${inside.length - FOLDER_LISTING_MAX} more files` : "";
  return `Files in ${folder}/ (${inside.length}):\n${shown.join("\n")}${more}`;
}

export function symbolSnippet(path: string, line: number, content: string, contextBefore = 3, contextAfter = 40): string {
  const lines = content.split("\n");
  const start = Math.max(0, line - 1 - contextBefore);
  const end = Math.min(lines.length, line - 1 + contextAfter);
  return `${path}:${start + 1}-${end}\n${lines.slice(start, end).map((text, index) => `${String(start + index + 1).padStart(5)}  ${text}`).join("\n")}`;
}

function fileOption(path: string): MentionOption {
  const slash = path.lastIndexOf("/");
  return { kind: "file", id: path, label: path.slice(slash + 1), detail: slash > 0 ? path.slice(0, slash) : "" };
}

export function formatProblems(problems: CodeProblem[]): string {
  if (problems.length === 0) return "No problems reported.";
  return problems
    .map((problem) => `${problem.path}:${problem.line}:${problem.column} ${problem.severity}: ${problem.message}${problem.source ? ` (${problem.source})` : ""}`)
    .join("\n");
}

export function clampMentionContent(content: string): string {
  if (content.length <= MENTION_CONTENT_MAX_CHARS) return content;
  return `${content.slice(0, MENTION_CONTENT_MAX_CHARS)}\n… (truncated, ${content.length - MENTION_CONTENT_MAX_CHARS} more characters)`;
}
