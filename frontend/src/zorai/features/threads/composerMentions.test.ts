import { describe, expect, it } from "vitest";
import { clampMentionContent, folderListing, folderPaths, formatProblems, mentionOptions, mentionQueryAt, MENTION_CONTENT_MAX_CHARS, removeMentionQuery, symbolSnippet, webMentionUrl } from "./composerMentions";

describe("composer mentions", () => {
  it("opens only for an @ that starts a word, so emails and decorators in prose do not trigger it", () => {
    expect(mentionQueryAt("look at @src/ma", 15)).toEqual({ query: "src/ma", start: 8, end: 15 });
    expect(mentionQueryAt("@", 1)).toEqual({ query: "", start: 0, end: 1 });
    expect(mentionQueryAt("mail me at a@b.com", 18)).toBeNull();
    expect(mentionQueryAt("@done and more", 14)).toBeNull();
  });

  it("removes the typed @query once picked, leaving the rest of the message intact", () => {
    expect(removeMentionQuery("fix @ma please", { query: "ma", start: 4, end: 7 })).toEqual({ text: "fix please", caret: 4 });
  });

  it("puts context mentions first and ranks files by how well their name matches", () => {
    const files = ["docs/main-notes.md", "src/main.rs", "src/lib/domain.rs"];
    expect(mentionOptions("ma", files).map((option) => option.id)).toEqual(["src/main.rs", "docs/main-notes.md", "src/lib/domain.rs"]);
    expect(mentionOptions("pro", files)[0].id).toBe("@problems");
    expect(mentionOptions("", files).slice(0, 4).map((option) => option.kind)).toEqual(["problems", "diff", "staged", "terminal"]);
  });

  it("formats problems with location so the agent can jump straight to them", () => {
    expect(formatProblems([{ path: "src/a.ts", line: 3, column: 5, message: "x is unused", severity: "warning", source: "ts", code: null }])).toBe("src/a.ts:3:5 warning: x is unused (ts)");
    expect(formatProblems([])).toBe("No problems reported.");
  });

  it("caps huge context so one @diff cannot blow the model's context window", () => {
    const clamped = clampMentionContent("x".repeat(MENTION_CONTENT_MAX_CHARS + 10));
    expect(clamped.length).toBeLessThan(MENTION_CONTENT_MAX_CHARS + 100);
    expect(clamped).toContain("truncated, 10 more characters");
  });

  it("offers folders, commits, symbols and URLs once their prefix is typed", () => {
    const files = ["src/app/main.ts", "src/lib/util.ts", "README.md"];
    expect(folderPaths(files)).toEqual(["src", "src/app", "src/lib"]);
    expect(mentionOptions("folder:lib", files).map((option) => option.id)).toEqual(["src/lib"]);
    const commits = [{ hash: "abc123", shortHash: "abc123", subject: "Fix login", date: "today" }, { hash: "def456", shortHash: "def456", subject: "Add docs", date: "today" }];
    expect(mentionOptions("commit:login", files, { commits }).map((option) => option.id)).toEqual(["abc123"]);
    expect(mentionOptions("commit:def", files, { commits }).map((option) => option.id)).toEqual(["def456"]);
    expect(mentionOptions("symbol:par", files, { symbols: [{ name: "parse", kind: "function", path: "src/lib/util.ts", line: 4 }] })[0]).toMatchObject({ kind: "symbol", path: "src/lib/util.ts", line: 4 });
    expect(mentionOptions("web:docs.rs/tokio", files)[0]).toMatchObject({ kind: "web", id: "https://docs.rs/tokio" });
  });

  it("only accepts real web URLs, so a stray word is never sent as a page to fetch", () => {
    expect(webMentionUrl("example.com/a")).toBe("https://example.com/a");
    expect(webMentionUrl("notaurl")).toBeNull();
    expect(webMentionUrl("file:///etc/passwd")).toBeNull();
  });

  it("lists a folder's files relative to it and shows the symbol's code with line numbers", () => {
    expect(folderListing("src", ["src/a.ts", "src/b/c.ts", "lib/d.ts"])).toBe("Files in src/ (2):\na.ts\nb/c.ts");
    expect(symbolSnippet("a.ts", 2, "one\ntwo\nthree", 1, 2)).toBe("a.ts:1-3\n    1  one\n    2  two\n    3  three");
  });
});

describe("@codebase mentions", () => {
  it("turns a hyphenated mention into a natural-language query and formats ranked code", async () => {
    const { codebaseQuery, formatCodebaseHits, mentionOptions } = await import("./composerMentions");
    expect(codebaseQuery("retry-failed_uploads")).toBe("retry failed uploads");
    expect(mentionOptions("codebase:retry-uploads", [])).toEqual([{ kind: "codebase", id: "retry uploads", label: "codebase: retry uploads", detail: "Semantic matches from this workspace's code index" }]);
    expect(mentionOptions("codebase:", [])).toEqual([]);
    const text = formatCodebaseHits("retry uploads", [{ title: "src/up.rs:10 function retry", snippet: "fn retry() {}", score: 0.83 }]);
    expect(text).toContain("### src/up.rs:10 function retry (83%)");
    expect(formatCodebaseHits("x", [])).toContain("No code");
  });
});
