import { describe, expect, it } from "vitest";
import { KATEX_DELIMITERS, isRelativeFileHref, localFilePathFromHref, markdownRenderMode, protectMathSegments } from "./markdown";

describe("markdownRenderMode", () => {
  it("skips markdown while streaming so each token does not re-parse the whole bubble", () => {
    expect(markdownRenderMode(true)).toBe("plain");
    expect(markdownRenderMode(false)).toBe("markdown");
    expect(markdownRenderMode(undefined)).toBe("markdown");
  });
});

describe("protectMathSegments", () => {
  it("protects the entire multiline formula shown in the GUI regression", () => {
    const formula = [
      "\\[ \\widehat{\\Delta}_{p,c,g}",
      "= \\alpha_c \\Delta^{K562}_{p,g} + \\beta_c + R\\theta(p,c,g) \\]",
    ].join("\n");
    const protectedMath = protectMathSegments(`Baseline:\n\n${formula}\n\nwhere:`);

    expect(protectedMath.segments).toEqual([formula]);
    expect(protectedMath.content).not.toContain("\\widehat");
    expect(protectedMath.content).not.toContain("_{p,c,g}");
    expect(protectedMath.content).toContain("Baseline:");
    expect(protectedMath.content).toContain("where:");
  });

  it("does not extract inline code or fenced code blocks", () => {
    const source = [
      "Use `\\(literal\\)` then \\(rendered\\).",
      "```tex",
      "\\[literal\\]",
      "```",
      "\\[rendered\\]",
    ].join("\n");
    const protectedMath = protectMathSegments(source);
    expect(protectedMath.segments).toEqual(["\\(rendered\\)", "\\[rendered\\]"]);
    expect(protectedMath.content).toContain("`\\(literal\\)`");
    expect(protectedMath.content).toContain("```tex\n\\[literal\\]\n```");
  });
});

describe("KaTeX delimiters", () => {
  it("supports common model-generated inline and display LaTeX forms", () => {
    expect(KATEX_DELIMITERS).toEqual(expect.arrayContaining([
      { left: "$", right: "$", display: false },
      { left: "$$", right: "$$", display: true },
      { left: "\\(", right: "\\)", display: false },
      { left: "\\[", right: "\\]", display: true },
      { left: "\\begin{align}", right: "\\end{align}", display: true },
      { left: "\\begin{align*}", right: "\\end{align*}", display: true },
    ]));
  });

  it("checks double-dollar display math before single-dollar inline math", () => {
    const displayIndex = KATEX_DELIMITERS.findIndex(({ left }) => left === "$$");
    const inlineIndex = KATEX_DELIMITERS.findIndex(({ left }) => left === "$");
    expect(displayIndex).toBeGreaterThanOrEqual(0);
    expect(inlineIndex).toBeGreaterThan(displayIndex);
  });
});

describe("localFilePathFromHref", () => {
  it("opens local files from thread links in the Zorai preview instead of the OS", () => {
    expect(localFilePathFromHref("/home/op/report.md")).toBe("/home/op/report.md");
    expect(localFilePathFromHref("file:///home/op/My%20Notes.md")).toBe("/home/op/My Notes.md");
    expect(localFilePathFromHref("/mnt/e/repo/src/main.rs:42")).toBe("/mnt/e/repo/src/main.rs");
    expect(localFilePathFromHref("/mnt/e/repo/README.md#setup")).toBe("/mnt/e/repo/README.md");
    expect(localFilePathFromHref("C:\\work\\plan.md")).toBe("C:\\work\\plan.md");
    expect(localFilePathFromHref("file:///C:/work/plan.md")).toBe("C:/work/plan.md");
  });

  it("leaves web links and unresolvable relative paths to the browser", () => {
    expect(localFilePathFromHref("https://example.com/a.md")).toBeNull();
    expect(localFilePathFromHref("//cdn.example.com/a.js")).toBeNull();
    expect(localFilePathFromHref("mailto:op@example.com")).toBeNull();
    expect(localFilePathFromHref("docs/plan.md")).toBeNull();
    expect(localFilePathFromHref(undefined)).toBeNull();
  });

  it("resolves agent links written relative to the thread workspace", () => {
    const root = "/mnt/e/gitlab/ai/clinical-gym";
    expect(localFilePathFromHref(
      "knowledge/batches/sixth/_audit/clinical-review-validation.parent.json",
      root,
    )).toBe("/mnt/e/gitlab/ai/clinical-gym/knowledge/batches/sixth/_audit/clinical-review-validation.parent.json");
    expect(localFilePathFromHref("./docs/../README.md#usage", `${root}/`)).toBe("/mnt/e/gitlab/ai/clinical-gym/README.md");
    expect(localFilePathFromHref("src/main.rs:12", root)).toBe("/mnt/e/gitlab/ai/clinical-gym/src/main.rs");
    expect(localFilePathFromHref("My%20Notes.md", root)).toBe("/mnt/e/gitlab/ai/clinical-gym/My Notes.md");
  });

  it("treats only scheme-less, non-anchor links as relative files", () => {
    expect(isRelativeFileHref("knowledge/review.md")).toBe(true);
    expect(isRelativeFileHref("../review.md")).toBe(true);
    expect(isRelativeFileHref("#section")).toBe(false);
    expect(isRelativeFileHref("https://example.com")).toBe(false);
    expect(isRelativeFileHref("mailto:op@example.com")).toBe(false);
    expect(isRelativeFileHref("/abs/path.md")).toBe(false);
  });
});
