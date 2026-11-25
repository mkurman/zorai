import { describe, expect, it } from "vitest";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { KATEX_DELIMITERS, MarkdownContent, advanceStreamingMarkdownBlocks, isRelativeFileHref, localFilePathFromHref, markdownRenderMode, protectMathSegments, resolveRelativeFileLink } from "./markdown";

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
      { left: "$$", right: "$$", display: true },
      { left: "\\(", right: "\\)", display: false },
      { left: "\\[", right: "\\]", display: true },
      { left: "\\begin{align}", right: "\\end{align}", display: true },
      { left: "\\begin{align*}", right: "\\end{align*}", display: true },
    ]));
  });

  it("never lets a lone dollar sign start a formula, so prices stay prose", () => {
    expect(KATEX_DELIMITERS.some(({ left }) => left === "$")).toBe(false);
    const prices = protectMathSegments("snapshot showed $0.276 metered and $0 billed, under the $28 ceiling");
    expect(prices.segments).toEqual([]);
    expect(prices.content).toBe("snapshot showed $0.276 metered and $0 billed, under the $28 ceiling");
    expect(protectMathSegments("costs $5 and $10 today").segments).toEqual([]);
  });

  it("still renders real single-dollar inline math by handing it to KaTeX as \\(...\\)", () => {
    const math = protectMathSegments("Energy $E = mc^2$ and $x$, but not \\$5 or $$ alone");
    expect(math.segments).toEqual(["\\(E = mc^2\\)", "\\(x\\)"]);
    expect(protectMathSegments("`$a$` stays code").segments).toEqual([]);
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

describe("resolveRelativeFileLink", () => {
  const workspace = "/home/me/project";
  const threadDir = "/home/me/.zorai/threads/thread_1";

  it("opens thread inventory files such as generated media even when the thread has no workspace", async () => {
    const exists = async (path: string) => path === `${threadDir}/artifacts/media/clip.mp4`;
    await expect(resolveRelativeFileLink("artifacts/media/clip.mp4", [threadDir], exists)).resolves.toBe(`${threadDir}/artifacts/media/clip.mp4`);
  });

  it("falls back to the thread folder when the workspace does not contain the linked file", async () => {
    const exists = async (path: string) => path.startsWith(threadDir);
    await expect(resolveRelativeFileLink("artifacts/media/clip.mp4", [workspace, threadDir], exists)).resolves.toBe(`${threadDir}/artifacts/media/clip.mp4`);
  });

  it("prefers the workspace when the file exists there, and uses it when neither has the file", async () => {
    await expect(resolveRelativeFileLink("src/main.rs", [workspace, threadDir], async () => true)).resolves.toBe(`${workspace}/src/main.rs`);
    await expect(resolveRelativeFileLink("src/main.rs", [workspace, threadDir], async () => false)).resolves.toBe(`${workspace}/src/main.rs`);
  });

  it("reports nothing to open when there is no folder to resolve against", async () => {
    await expect(resolveRelativeFileLink("artifacts/media/clip.mp4", [], async () => true)).resolves.toBeNull();
  });
});

describe("advanceStreamingMarkdownBlocks", () => {
  const empty = { blocks: [], consumed: 0, text: "" };
  const stream = (chunks: string[]) => {
    let state = advanceStreamingMarkdownBlocks(empty, "");
    let content = "";
    for (const chunk of chunks) {
      content += chunk;
      state = advanceStreamingMarkdownBlocks(state, content);
    }
    return { state, tail: content.slice(state.consumed) };
  };

  it("renders a paragraph as markdown once the next one starts, keeping the unfinished one as plain text", () => {
    const { state, tail } = stream(["**Bold** intro", ".\n\n", "Second para", "graph is still typ"]);
    expect(state.blocks).toEqual(["**Bold** intro.\n\n"]);
    expect(tail).toBe("Second paragraph is still typ");
  });

  it("never splits a code fence on its blank lines, so the block renders whole once the fence closes", () => {
    const open = stream(["Intro\n\n```py\n", "a = 1\n\n", "b = 2\n\n", "c"]);
    expect(open.state.blocks).toEqual(["Intro\n\n"]);
    expect(open.tail).toBe("```py\na = 1\n\nb = 2\n\nc");
    const closed = stream(["Intro\n\n```py\na = 1\n\nb = 2\n```\n\n", "After"]);
    expect(closed.state.blocks).toEqual(["Intro\n\n", "```py\na = 1\n\nb = 2\n```\n\n"]);
    expect(closed.tail).toBe("After");
  });

  it("keeps display math and indented list continuations with their block", () => {
    const math = stream(["$$\nx\n\ny\n$$\n\nNext"]);
    expect(math.state.blocks).toEqual(["$$\nx\n\ny\n$$\n\n"]);
    const list = stream(["- item\n\n  more of the item\n\n- next\n"]);
    expect(list.state.blocks).toEqual(["- item\n\n  more of the item\n\n"]);
  });

  it("starts over when the streamed text is replaced instead of extended", () => {
    const first = advanceStreamingMarkdownBlocks(empty, "One\n\nTwo");
    expect(first.blocks).toEqual(["One\n\n"]);
    expect(advanceStreamingMarkdownBlocks(first, "Other\n\nText").blocks).toEqual(["Other\n\n"]);
    expect(advanceStreamingMarkdownBlocks(first, "Short").blocks).toEqual([]);
  });

  it("shows finished paragraphs and closed fences formatted while the unfinished tail stays plain", () => {
    const html = renderToStaticMarkup(createElement(MarkdownContent, {
      content: "**Done** para\n\n```js\nconst a = 1;\n```\n\nStill **typing",
      streaming: true,
    }));
    expect(html).toContain("<strong>Done</strong>");
    expect(html).toContain("<pre");
    expect(html).toContain('<div class="acp-md acp-md--streaming">Still **typing</div>');
  });

  it("keeps finished blocks identical across tokens so they are not re-rendered", () => {
    const first = advanceStreamingMarkdownBlocks(empty, "One\n\nTwo");
    const next = advanceStreamingMarkdownBlocks(first, "One\n\nTwo more");
    expect(next).toBe(first);
  });
});

describe("MarkdownContent HTML", () => {
  const source = [
    '<div align="center"><b>Read our docs</b><img src="x.png" onerror="alert(1)"></div>',
    "",
    "<script>alert(1)</script>",
    "",
    "# Title",
  ].join("\n");

  it("renders README-style HTML for file previews while stripping scripts and event handlers", () => {
    const html = renderToStaticMarkup(createElement(MarkdownContent, { content: source, filePreview: true }));
    expect(html).toContain('<div align="center"><b>Read our docs</b>');
    expect(html).not.toContain("<script");
    expect(html).not.toContain("onerror");
    expect(html).toContain("<h1>Title</h1>");
  });

  it("keeps HTML inert in chat so model output cannot inject markup", () => {
    const html = renderToStaticMarkup(createElement(MarkdownContent, { content: source }));
    expect(html).not.toContain("<b>Read our docs</b>");
    expect(html).not.toContain("<script");
    expect(html).toContain("<h4>Title</h4>");
  });
});
