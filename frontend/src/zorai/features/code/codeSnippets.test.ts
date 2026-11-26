import { describe, expect, it } from "vitest";
import { snippetsForLanguage, type CodeSnippet } from "./codeSnippets";

const snippet = (prefix: string, scope: string[]): CodeSnippet => ({ name: prefix, prefix, body: prefix, description: prefix, scope, source: "test" });

describe("snippets", () => {
  it("offers global snippets everywhere and scoped ones only in their languages, treating TSX as TypeScript", () => {
    const all = [snippet("any", []), snippet("ts", ["typescript"]), snippet("py", ["python"])];
    expect(snippetsForLanguage(all, "typescriptreact").map((entry) => entry.prefix)).toEqual(["any", "ts"]);
    expect(snippetsForLanguage(all, "python").map((entry) => entry.prefix)).toEqual(["any", "py"]);
  });
});
