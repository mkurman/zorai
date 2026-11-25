import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { claimLanguageServerPrompt } from "./codeLanguageServers";

describe("language server install prompt", () => {
  const previous = (globalThis as { localStorage?: unknown }).localStorage;
  beforeEach(() => {
    const stored = new Map<string, string>();
    (globalThis as { localStorage?: unknown }).localStorage = { getItem: (key: string) => stored.get(key) ?? null, setItem: (key: string, value: string) => stored.set(key, value) };
  });
  afterEach(() => {
    (globalThis as { localStorage?: unknown }).localStorage = previous;
  });

  it("asks once per language server so opening more files never nags again", () => {
    expect(claimLanguageServerPrompt("python")).toBe(true);
    expect(claimLanguageServerPrompt("python")).toBe(false);
    expect(claimLanguageServerPrompt("rust")).toBe(true);
  });
});
