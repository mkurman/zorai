import { describe, expect, it } from "vitest";
import { findMergeConflicts, resolveConflictText } from "./codeMergeConflicts";

const text = [
  "keep",
  "<<<<<<< HEAD",
  "ours",
  "||||||| base",
  "original",
  "=======",
  "theirs 1",
  "theirs 2",
  ">>>>>>> feature",
  "tail",
  "<<<<<<< HEAD",
  "a",
  "=======",
  ">>>>>>> other",
].join("\n");

describe("merge conflicts", () => {
  it("finds two-way and diff3 conflict blocks but ignores unterminated markers", () => {
    expect(findMergeConflicts(text)).toEqual([
      { start: 2, base: 4, separator: 6, end: 9 },
      { start: 11, base: null, separator: 13, end: 14 },
    ]);
    expect(findMergeConflicts("<<<<<<< HEAD\nx\n=======\ny")).toEqual([]);
  });

  it("each choice keeps only real code, never the markers or the diff3 base", () => {
    const lines = text.split("\n");
    const [diff3, plain] = findMergeConflicts(text);
    expect(resolveConflictText(lines, diff3, "current")).toBe("ours");
    expect(resolveConflictText(lines, diff3, "incoming")).toBe("theirs 1\ntheirs 2");
    expect(resolveConflictText(lines, diff3, "both")).toBe("ours\ntheirs 1\ntheirs 2");
    expect(resolveConflictText(lines, plain, "incoming")).toBe("");
  });
});
