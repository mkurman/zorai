import { describe, expect, it } from "vitest";
import { groupCodeSearchHits, previewCodeSearchReplacement, splitCodeSearchPreview } from "./codeSearchResults";

describe("code search results", () => {
  it("groups matches per file in path order with matches in reading order, so the tree reads like an editor search view", () => {
    const groups = groupCodeSearchHits([
      { path: "src/b.ts", line: 9, column: 1, preview: "x" },
      { path: "src/a.ts", line: 4, column: 7, preview: "x" },
      { path: "src/a.ts", line: 4, column: 2, preview: "x" },
      { path: "src/a.ts", line: 1, column: 1, preview: "x" },
    ]);
    expect(groups.map((group) => [group.name, group.dir, group.hits.map((hit) => `${hit.line}:${hit.column}`)])).toEqual([
      ["a.ts", "src", ["1:1", "4:2", "4:7"]],
      ["b.ts", "src", ["9:1"]],
    ]);
  });

  it("highlights exactly the matched text and clamps ranges that run past a truncated preview", () => {
    expect(splitCodeSearchPreview({ path: "a", line: 1, column: 1, preview: "const foo = 1", matchStart: 6, matchLength: 3 })).toEqual({ before: "const ", match: "foo", after: " = 1" });
    expect(splitCodeSearchPreview({ path: "a", line: 1, column: 1, preview: "ab", matchStart: 1, matchLength: 9 })).toEqual({ before: "a", match: "b", after: "" });
  });

  it("previews regex replacements with capture groups but keeps literal replacements verbatim", () => {
    expect(previewCodeSearchReplacement("foo", "(f)oo", "$1x", { caseSensitive: false, wholeWord: false, regex: true })).toBe("fx");
    expect(previewCodeSearchReplacement("foo", "foo", "$1x", { caseSensitive: false, wholeWord: false, regex: false })).toBe("$1x");
  });
});
