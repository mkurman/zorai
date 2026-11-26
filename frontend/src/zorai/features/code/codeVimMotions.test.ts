import { describe, expect, it } from "vitest";
import { applyMotion, operatorRange, parseVimCommand, substituteLine, textObjectRange } from "./codeVimMotions";

const lines = ["const value = compute(a, b);", "  return value;", "", "end"];

describe("vim command parsing", () => {
  it("understands counts, operators, motions and text objects, and waits for incomplete input", () => {
    expect(parseVimCommand("3j")).toEqual({ kind: "motion", count: 3, motion: "j" });
    expect(parseVimCommand("dd")).toEqual({ kind: "operator", count: 1, operator: "d", linewise: true });
    expect(parseVimCommand("2d3w")).toEqual({ kind: "operator", count: 6, operator: "d", motion: "w" });
    expect(parseVimCommand("ciw")).toEqual({ kind: "operator", count: 1, operator: "c", textObject: "iw" });
    expect(parseVimCommand("c")).toBe("pending");
    expect(parseVimCommand("ci")).toBe("pending");
    expect(parseVimCommand("g")).toBe("pending");
    expect(parseVimCommand("gg")).toEqual({ kind: "motion", count: 1, motion: "gg" });
    expect(parseVimCommand("ra")).toEqual({ kind: "action", count: 1, action: "ra" });
    expect(parseVimCommand("q")).toBe("invalid");
  });
});

describe("vim motions", () => {
  it("moves by words, line ends and file ends like Vim", () => {
    expect(applyMotion(lines, { line: 0, col: 0 }, "w")).toEqual({ line: 0, col: 6 });
    expect(applyMotion(lines, { line: 0, col: 6 }, "e")).toEqual({ line: 0, col: 10 });
    expect(applyMotion(lines, { line: 0, col: 14 }, "b")).toEqual({ line: 0, col: 12 });
    expect(applyMotion(lines, { line: 1, col: 5 }, "^")).toEqual({ line: 1, col: 2 });
    expect(applyMotion(lines, { line: 0, col: 0 }, "$")).toEqual({ line: 0, col: lines[0].length - 1 });
    expect(applyMotion(lines, { line: 0, col: 0 }, "G")).toEqual({ line: 3, col: 0 });
    expect(applyMotion(lines, { line: 0, col: 21 }, "%")).toEqual({ line: 0, col: 26 });
  });

  it("selects inner and around text objects for ciw, di( and ci\"", () => {
    expect(textObjectRange(lines, { line: 0, col: 8 }, "iw")).toEqual({ start: { line: 0, col: 6 }, end: { line: 0, col: 11 }, linewise: false });
    expect(textObjectRange(lines, { line: 0, col: 23 }, "i(")).toEqual({ start: { line: 0, col: 22 }, end: { line: 0, col: 26 }, linewise: false });
    expect(textObjectRange(['say "hi there" now'], { line: 0, col: 6 }, "i\"")).toEqual({ start: { line: 0, col: 5 }, end: { line: 0, col: 13 }, linewise: false });
  });

  it("dd and dj delete whole lines, dw stops before the next word, cw changes to the word end", () => {
    expect(operatorRange(lines, { line: 1, col: 3 }, { kind: "operator", count: 2, operator: "d", linewise: true })).toEqual({ start: { line: 1, col: 0 }, end: { line: 2, col: 0 }, linewise: true });
    expect(operatorRange(lines, { line: 0, col: 0 }, { kind: "operator", count: 1, operator: "d", motion: "w" })).toEqual({ start: { line: 0, col: 0 }, end: { line: 0, col: 6 }, linewise: false });
    expect(operatorRange(lines, { line: 0, col: 0 }, { kind: "operator", count: 1, operator: "c", motion: "w" })).toEqual({ start: { line: 0, col: 0 }, end: { line: 0, col: 5 }, linewise: false });
  });

  it("runs :s substitutions on a line", () => {
    expect(substituteLine("a-a-a", "s/a/b/g")).toBe("b-b-b");
    expect(substituteLine("a-a-a", "s/a/b")).toBe("b-a-a");
    expect(substituteLine("x", "s/(/y/")).toBeNull();
  });
});
