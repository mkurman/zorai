const STORAGE_KEY = "zorai.code.viewState";
export const MAX_VIEW_STATES = 200;

type ViewStateMap = Record<string, unknown>;

function read(): ViewStateMap {
  try {
    const parsed = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "{}");
    return parsed && typeof parsed === "object" ? parsed : {};
  } catch {
    return {};
  }
}

export function codeViewStateKey(root: string, path: string): string {
  return `${root.replace(/[\\/]+$/, "")}::${path}`;
}

export function loadCodeViewState(key: string): unknown {
  return read()[key] ?? null;
}

export function saveCodeViewState(key: string, state: unknown): void {
  if (!state) return;
  const current = read();
  delete current[key];
  const entries = [...Object.entries(current), [key, state] as const].slice(-MAX_VIEW_STATES);
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(Object.fromEntries(entries)));
  } catch {
    return;
  }
}
