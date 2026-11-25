const CLASS_CONTEXT = /\bclass(?:Name)?\s*=\s*\{?|\b(?:clsx|cn|classNames|classnames|twMerge|cva)\(|\bclassList\.\w+\(/g;

export function cssClassAtPosition(lineText: string, column: number): { name: string; startColumn: number; endColumn: number } | null {
  const index = Math.max(0, column - 1);
  let start = index;
  let end = index;
  while (start > 0 && /[\w-]/.test(lineText[start - 1])) start -= 1;
  while (end < lineText.length && /[\w-]/.test(lineText[end])) end += 1;
  const name = lineText.slice(start, end);
  if (!/^-?[A-Za-z_][\w-]*$/.test(name)) return null;
  const prefix = lineText.slice(0, start);
  let contextEnd = -1;
  for (const match of prefix.matchAll(CLASS_CONTEXT)) contextEnd = (match.index ?? 0) + match[0].length;
  if (contextEnd < 0) return null;
  const segment = prefix.slice(contextEnd);
  const insideString = ['"', "'", "`"].some((quote) => segment.split(quote).length % 2 === 0);
  return insideString ? { name, startColumn: start + 1, endColumn: end + 1 } : null;
}

export function cssClassSelectorPattern(name: string): string {
  return `\\.${name.replace(/[-\\^$*+?.()|[\]{}]/g, "\\$&")}(?![\\w-])`;
}
