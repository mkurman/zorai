export type CodeNavLocation = { path: string; line: number; column: number };

export const NAV_HISTORY_LIMIT = 50;
export const NAV_JUMP_LINES = 15;

export type CodeNavHistory = { back: CodeNavLocation[]; forward: CodeNavLocation[] };

export const emptyNavHistory = (): CodeNavHistory => ({ back: [], forward: [] });

export function isNavJump(from: CodeNavLocation | null, to: CodeNavLocation): boolean {
  return Boolean(from) && (from!.path !== to.path || Math.abs(from!.line - to.line) >= NAV_JUMP_LINES);
}

export function recordNavLocation(history: CodeNavHistory, location: CodeNavLocation): CodeNavHistory {
  const last = history.back[history.back.length - 1];
  const back = last && !isNavJump(last, location) ? [...history.back.slice(0, -1), location] : [...history.back, location];
  return { back: back.slice(-NAV_HISTORY_LIMIT), forward: [] };
}

export function navigateBack(history: CodeNavHistory, current: CodeNavLocation): { history: CodeNavHistory; target: CodeNavLocation | null } {
  const back = [...history.back];
  let target = back.pop() ?? null;
  while (target && !isNavJump(current, target) && back.length > 0) target = back.pop() ?? null;
  if (!target || !isNavJump(current, target)) return { history, target: null };
  return { history: { back, forward: [...history.forward, current].slice(-NAV_HISTORY_LIMIT) }, target };
}

export function navigateForward(history: CodeNavHistory, current: CodeNavLocation): { history: CodeNavHistory; target: CodeNavLocation | null } {
  const forward = [...history.forward];
  const target = forward.pop() ?? null;
  if (!target) return { history, target: null };
  return { history: { back: [...history.back, current].slice(-NAV_HISTORY_LIMIT), forward }, target };
}
