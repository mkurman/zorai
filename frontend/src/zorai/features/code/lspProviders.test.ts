import { describe, expect, it } from "vitest";
import { lspCompletionInsert, lspInlayLabel, lspMarkup } from "./lspProviders";

describe("LSP provider conversions", () => {
  it("prefers the server's text edit and keeps snippet placeholders intact so auto-import completions insert what the server meant", () => {
    expect(lspCompletionInsert({ label: "useState", insertText: "useState", textEdit: { newText: "useState($1)" }, insertTextFormat: 2 })).toEqual({ text: "useState($1)", snippet: true });
    expect(lspCompletionInsert({ label: { label: "HashMap" } })).toEqual({ text: "HashMap", snippet: false });
  });

  it("flattens markup and inlay label parts into display text", () => {
    expect(lspMarkup({ kind: "markdown", value: "**doc**" })).toBe("**doc**");
    expect(lspMarkup(42)).toBeUndefined();
    expect(lspInlayLabel([{ value: ": " }, { value: "Vec<u8>" }])).toBe(": Vec<u8>");
    expect(lspInlayLabel("count:")).toBe("count:");
  });
});

describe("LSP code lens commands", () => {
  it("recognises the reference-count lenses servers send so clicking them opens references instead of doing nothing", async () => {
    const { lspLensIsReferences } = await import("./lspProviders");
    expect(lspLensIsReferences("editor.action.showReferences")).toBe(true);
    expect(lspLensIsReferences("rust-analyzer.showReferences")).toBe(true);
    expect(lspLensIsReferences("rust-analyzer.runSingle")).toBe(false);
    expect(lspLensIsReferences(undefined)).toBe(false);
  });
});
