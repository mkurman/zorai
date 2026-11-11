import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { PromptEditor } from "./PromptEditor";

describe("PromptEditor", () => {
  it("opens in edit mode with a preview toggle so prompts are editable first and readable on demand", () => {
    const html = renderToStaticMarkup(<PromptEditor label="Svarog system prompt" path="settings/svarog.md" value="# Identity" onChange={() => undefined} />);

    expect(html).toContain('aria-label="Svarog system prompt view"');
    expect(html).toMatch(/aria-pressed="true"[^>]*>Edit</);
    expect(html).toMatch(/aria-pressed="false"[^>]*>Preview</);
    expect(html).toContain("zorai-prompt-editor__fallback");
  });
});
