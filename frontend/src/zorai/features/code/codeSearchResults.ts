export type CodeSearchHit = { path: string; line: number; column: number; preview: string; matchStart?: number; matchLength?: number };

export type CodeSearchOptions = { caseSensitive: boolean; wholeWord: boolean; regex: boolean; preserveCase?: boolean };

export type CodeSearchGroup = { path: string; name: string; dir: string; hits: CodeSearchHit[] };

export function groupCodeSearchHits(hits: CodeSearchHit[]): CodeSearchGroup[] {
  const groups = new Map<string, CodeSearchHit[]>();
  for (const hit of hits) groups.set(hit.path, [...(groups.get(hit.path) ?? []), hit]);
  return [...groups.entries()]
    .sort(([left], [right]) => left.localeCompare(right))
    .map(([path, fileHits]) => {
      const parts = path.split(/[\\/]/);
      return {
        path,
        name: parts[parts.length - 1] ?? path,
        dir: parts.slice(0, -1).join("/"),
        hits: [...fileHits].sort((left, right) => left.line - right.line || left.column - right.column),
      };
    });
}

export function splitCodeSearchPreview(hit: CodeSearchHit): { before: string; match: string; after: string } {
  const start = Math.max(0, Math.min(hit.matchStart ?? 0, hit.preview.length));
  const end = Math.min(hit.preview.length, start + Math.max(0, hit.matchLength ?? 0));
  return { before: hit.preview.slice(0, start), match: hit.preview.slice(start, end), after: hit.preview.slice(end) };
}

export function applyReplacementCase(source: string, text: string): string {
  if (!/[a-z]/i.test(source)) return text;
  if (source === source.toUpperCase()) return text.toUpperCase();
  if (source === source.toLowerCase()) return text.toLowerCase();
  if (source[0] === source[0].toUpperCase() && source.slice(1) === source.slice(1).toLowerCase()) return text.charAt(0).toUpperCase() + text.slice(1);
  return text;
}

export function previewCodeSearchReplacement(match: string, query: string, replacement: string, options: CodeSearchOptions): string {
  const cased = (text: string) => (options.preserveCase ? applyReplacementCase(match, text) : text);
  if (!options.regex) return cased(replacement);
  try {
    const source = options.wholeWord ? `\\b(?:${query})\\b` : query;
    return cased(match.replace(new RegExp(source, options.caseSensitive ? "" : "i"), replacement));
  } catch {
    return cased(replacement);
  }
}

export function searchSeedFromSelection(selected: string | null | undefined): string {
  if (!selected || /[\r\n]/.test(selected) || !selected.trim()) return "";
  return selected;
}
