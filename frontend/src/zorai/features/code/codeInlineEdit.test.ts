import { describe, expect, it } from "vitest";
import { computeLineHunks, streamingPreviewText } from "./codeInlineEdit";

describe("inline edit diff", () => {
  it("shows only the changed lines, so unchanged context is not marked red and green", () => {
    expect(computeLineHunks("a\nb\nc\nd", "a\nB\nc\nd")).toEqual([{ baseStart: 1, start: 1, removed: ["b"], added: ["B"] }]);
  });

  it("splits separate changes into separate hunks so each can be kept or undone on its own", () => {
    expect(computeLineHunks("a\nb\nc\nd\ne", "A\nb\nc\nd\nE")).toEqual([
      { baseStart: 0, start: 0, removed: ["a"], added: ["A"] },
      { baseStart: 4, start: 4, removed: ["e"], added: ["E"] },
    ]);
  });

  it("tracks where each hunk sits after earlier hunks change the line count", () => {
    expect(computeLineHunks("a\nb\nc", "a\nx\ny\nb\nc\nz")).toEqual([
      { baseStart: 1, start: 1, removed: [], added: ["x", "y"] },
      { baseStart: 3, start: 5, removed: [], added: ["z"] },
    ]);
  });

  it("handles pure insertions and pure deletions", () => {
    expect(computeLineHunks("a\nd", "a\nb\nc\nd")).toEqual([{ baseStart: 1, start: 1, removed: [], added: ["b", "c"] }]);
    expect(computeLineHunks("a\nb\nc", "a\nc")).toEqual([{ baseStart: 1, start: 1, removed: ["b"], added: [] }]);
  });

  it("treats code generated on an empty line as all added", () => {
    expect(computeLineHunks("", "const x = 1;\nexport { x };")).toEqual([{ baseStart: 0, start: 0, removed: [""], added: ["const x = 1;", "export { x };"] }]);
  });

  it("reports nothing when the proposal is identical", () => {
    expect(computeLineHunks("x\ny", "x\ny")).toEqual([]);
  });

  it("hides a model's code fence and target tags while streaming, but never real code", () => {
    expect(streamingPreviewText("```ts\nconst a = 1;\n")).toBe("const a = 1;\n");
    expect(streamingPreviewText("```ts")).toBe("");
    expect(streamingPreviewText("const a = 1;\n``")).toBe("const a = 1;");
    expect(streamingPreviewText("<target>\n  b();\n</tar")).toBe("  b();");
    expect(streamingPreviewText("if (a < b) {")).toBe("if (a < b) {");
    expect(streamingPreviewText("x = a <")).toBe("x = a <");
    expect(streamingPreviewText("<think>The user wants")).toBe("");
    expect(streamingPreviewText("<think>plan</think>\nconst a = 1;")).toBe("const a = 1;");
  });
});

