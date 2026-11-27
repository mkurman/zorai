import { describe, expect, it } from "vitest";
import { notebookHtmlDocument, type NotebookHtmlTheme } from "./notebookHtmlOutput";

const theme: NotebookHtmlTheme = { text: "#eee", muted: "#999", border: "#333", surface: "#111", stripe: "#222", accent: "#fa0", fontUi: "Inter", fontMono: "JetBrains Mono" };

describe("notebookHtmlDocument", () => {
  it("wraps pandas HTML in the app's colours and fonts so tables are readable on the dark theme", () => {
    const doc = notebookHtmlDocument('<table class="dataframe"><tr><td>1</td></tr></table>', theme);
    expect(doc).toContain("background: #111");
    expect(doc).toContain("color: #eee");
    expect(doc).toContain("JetBrains Mono");
    expect(doc).toContain('<body><table class="dataframe">');
  });

  it("keeps column headers visible while scrolling long frames", () => {
    expect(notebookHtmlDocument("", theme)).toMatch(/thead th \{ position: sticky; top: 0;/);
  });
});
