export type ExtraRootMatch = { root: string; relative: string };

function trimRoot(root: string): string {
  return root.replace(/[\\/]+$/, "");
}

export function extraRootOf(absolutePath: string, roots: readonly string[]): ExtraRootMatch | null {
  let best: ExtraRootMatch | null = null;
  for (const root of roots) {
    const base = trimRoot(root);
    if (!base || !absolutePath.startsWith(`${base}/`)) continue;
    if (!best || base.length > best.root.length) best = { root: base, relative: absolutePath.slice(base.length + 1) };
  }
  return best;
}

export function joinExtraRoot(root: string, relative: string): string {
  return `${trimRoot(root)}/${relative.replace(/^[\\/]+/, "")}`;
}
