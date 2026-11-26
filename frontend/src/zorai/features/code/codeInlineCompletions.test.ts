import { describe, expect, it } from "vitest";
import { cachedCompletion, completionCacheKey, rememberCompletion, shouldRequestCompletion } from "./codeInlineCompletions";

describe("inline completions", () => {
  it("does not ask the model while the cursor sits inside a word, where a suggestion would split it", () => {
    expect(shouldRequestCompletion("const tot", "al = 1;")).toBe(false);
    expect(shouldRequestCompletion("const total = ", ";")).toBe(true);
    expect(shouldRequestCompletion("fn main() {\n    ", "\n}")).toBe(true);
  });

  it("never asks for an empty file, so opening a new file costs nothing", () => {
    expect(shouldRequestCompletion("   \n", "")).toBe(false);
  });

  it("reuses an answer for the same spot so retyping does not pay for a second request", () => {
    const key = completionCacheKey("src/a.ts", "const x = ", ";");
    rememberCompletion(key, "1");
    expect(cachedCompletion(key)).toBe("1");
    expect(cachedCompletion(completionCacheKey("src/b.ts", "const x = ", ";"))).toBeUndefined();
  });

  it("keys the cache on text near the cursor, not on distant edits", () => {
    const far = "x".repeat(1000);
    expect(completionCacheKey("a.ts", `A${far}\nconst y = `, "")).toBe(completionCacheKey("a.ts", `B${far}\nconst y = `, ""));
  });
});
