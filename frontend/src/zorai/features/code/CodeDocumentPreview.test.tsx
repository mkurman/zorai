import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { CodeDocumentPreview, codePreviewKind } from "./CodeDocumentPreview";

describe("CodeDocumentPreview", () => {
  it("offers a preview only for formats that render differently from their source", () => {
    expect(codePreviewKind("docs/README.md")).toBe("markdown");
    expect(codePreviewKind("flow.MMD")).toBe("mermaid");
    expect(codePreviewKind("data/rows.jsonl")).toBe("table");
    expect(codePreviewKind("src/main.rs")).toBeNull();
    expect(codePreviewKind("config.json")).toBeNull();
  });

  it("renders the unsaved buffer, so edits show up without saving", () => {
    const html = renderToStaticMarkup(<CodeDocumentPreview path="notes.csv" content={"drug,dose\nAspirin,81 mg\n"} />);
    expect(html).toContain("Aspirin");
    expect(html).toContain("81 mg");
  });
});
