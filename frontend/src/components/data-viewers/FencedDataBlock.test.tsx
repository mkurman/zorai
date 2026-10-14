import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { MarkdownContent } from "@/components/agent-chat-panel/chat-view/markdown";

describe("chat markdown data fences", () => {
  it("renders csv fences as a table the operator can sort, with a raw toggle", () => {
    const html = renderToStaticMarkup(
      <MarkdownContent content={"Results:\n\n```csv\ncase,score\nA,10\nB,9\n```\n"} />,
    );
    expect(html).toContain("zorai-data-block");
    expect(html).toContain("2 rows × 2 columns");
    expect(html).toContain(">Raw<");
  });

  it("routes mermaid fences to the diagram viewer instead of a code block", () => {
    const html = renderToStaticMarkup(
      <MarkdownContent content={"```mermaid\ngraph TD\n  A-->B\n```\n"} />,
    );
    expect(html).toContain("zorai-mermaid");
    expect(html).toContain(">Source<");
    expect(html).not.toContain("<pre><code class=\"language-mermaid\"");
  });

  it("leaves ordinary code fences untouched", () => {
    const html = renderToStaticMarkup(<MarkdownContent content={"```rust\nfn main() {}\n```\n"} />);
    expect(html).not.toContain("zorai-data-block");
    expect(html).toContain("fn main() {}");
  });
});
