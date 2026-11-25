import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { MAX_VIEW_STATES, codeViewStateKey, loadCodeViewState, saveCodeViewState } from "./codeViewState";

describe("editor view state", () => {
  const previous = (globalThis as { localStorage?: unknown }).localStorage;
  beforeEach(() => {
    const stored = new Map<string, string>();
    (globalThis as { localStorage?: unknown }).localStorage = { getItem: (key: string) => stored.get(key) ?? null, setItem: (key: string, value: string) => stored.set(key, value) };
  });
  afterEach(() => {
    (globalThis as { localStorage?: unknown }).localStorage = previous;
  });

  it("brings a file back to the same cursor and scroll after switching away or reloading", () => {
    const key = codeViewStateKey("/repo/", "src/a.ts");
    saveCodeViewState(key, { cursorState: [{ position: { lineNumber: 42, column: 7 } }], viewState: { scrollTop: 900 } });
    expect(loadCodeViewState(key)).toEqual({ cursorState: [{ position: { lineNumber: 42, column: 7 } }], viewState: { scrollTop: 900 } });
    expect(loadCodeViewState(codeViewStateKey("/repo", "src/b.ts"))).toBeNull();
  });

  it("keeps only the most recent files so storage cannot grow without bound", () => {
    for (let index = 0; index <= MAX_VIEW_STATES; index += 1) saveCodeViewState(codeViewStateKey("/r", `f${index}`), { index });
    expect(loadCodeViewState(codeViewStateKey("/r", "f0"))).toBeNull();
    expect(loadCodeViewState(codeViewStateKey("/r", `f${MAX_VIEW_STATES}`))).toEqual({ index: MAX_VIEW_STATES });
  });
});
