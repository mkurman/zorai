export const MAX_CLOSED_EDITORS = 30;

export function rememberClosedEditor(stack: readonly string[], path: string): string[] {
  return [...stack.filter((entry) => entry !== path), path].slice(-MAX_CLOSED_EDITORS);
}

export function takeReopenableEditor(stack: readonly string[], openFiles: readonly string[]): { path: string | null; rest: string[] } {
  const open = new Set(openFiles);
  const rest = [...stack];
  while (rest.length > 0) {
    const path = rest.pop()!;
    if (!open.has(path)) return { path, rest };
  }
  return { path: null, rest };
}
