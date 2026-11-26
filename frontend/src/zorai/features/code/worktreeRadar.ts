export type LineRange = [number, number];
export type RadarWorktree = { path: string; label: string; files: string[]; ranges?: Record<string, LineRange[]> };
export type RadarConflict = { file: string; owners: string[]; overlap: LineRange | null };

export function firstOverlap(left: LineRange[] | undefined, right: LineRange[] | undefined): LineRange | null {
  for (const [leftStart, leftEnd] of left ?? []) {
    for (const [rightStart, rightEnd] of right ?? []) {
      if (leftStart <= rightEnd && rightStart <= leftEnd) return [Math.max(leftStart, rightStart), Math.min(leftEnd, rightEnd)];
    }
  }
  return null;
}

export function conflictRadar(worktrees: RadarWorktree[], mainChanged: string[], mainRanges: Record<string, LineRange[]> = {}): RadarConflict[] {
  const owners = new Map<string, Array<{ label: string; ranges?: LineRange[] }>>();
  const add = (file: string, label: string, ranges?: LineRange[]) => {
    const list = owners.get(file) ?? [];
    if (!list.some((entry) => entry.label === label)) list.push({ label, ranges });
    owners.set(file, list);
  };
  for (const worktree of worktrees) for (const file of worktree.files) add(file, worktree.label, worktree.ranges?.[file]);
  for (const file of mainChanged) if (owners.has(file)) add(file, "your working tree", mainRanges[file]);
  const conflicts: RadarConflict[] = [];
  for (const [file, list] of owners) {
    if (list.length < 2) continue;
    let overlap: LineRange | null = null;
    for (let left = 0; left < list.length && !overlap; left += 1) {
      for (let right = left + 1; right < list.length && !overlap; right += 1) overlap = firstOverlap(list[left].ranges, list[right].ranges);
    }
    conflicts.push({ file, owners: list.map((entry) => entry.label), overlap });
  }
  return conflicts.sort((left, right) => Number(Boolean(right.overlap)) - Number(Boolean(left.overlap)) || right.owners.length - left.owners.length || left.file.localeCompare(right.file));
}
