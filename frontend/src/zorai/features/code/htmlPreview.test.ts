import { describe, expect, it } from "vitest";
import { codePreviewKind, htmlPreviewDocument } from "./CodeDocumentPreview";

describe("HTML file preview", () => {
  it("offers a preview for HTML files alongside Markdown and diagrams", () => {
    expect(codePreviewKind("site/index.html")).toBe("html");
    expect(codePreviewKind("page.HTM")).toBe("html");
    expect(codePreviewKind("app.tsx")).toBeNull();
  });

  it("points relative images at the file's own folder so the preview looks like the page", () => {
    expect(htmlPreviewDocument("<html><head><title>x</title></head></html>", "zorai-media://local/w/site/"))
      .toBe('<html><head><base href="zorai-media://local/w/site/"><title>x</title></head></html>');
    expect(htmlPreviewDocument("<p>hi</p>", "zorai-media://local/w/")).toBe('<base href="zorai-media://local/w/"><p>hi</p>');
    expect(htmlPreviewDocument("<p>hi</p>", null)).toBe("<p>hi</p>");
  });
});
