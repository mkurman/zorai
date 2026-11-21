export type CodeSearchHit = { path: string; line: number; column: number; preview: string };

export type SearchItem = { kind: "file"; path: string } | { kind: "text"; hit: CodeSearchHit };

export const FILE_RESULTS_SHOWN = 30;

export function mergeSearchItems(files: string[], hits: CodeSearchHit[]): SearchItem[] {
  return [
    ...files.slice(0, FILE_RESULTS_SHOWN).map((path): SearchItem => ({ kind: "file", path })),
    ...hits.map((hit): SearchItem => ({ kind: "text", hit })),
  ];
}
