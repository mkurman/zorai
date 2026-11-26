import { describe, expect, it } from "vitest";
import { flattenLspSymbols, parseQuickOpenMode, quickOpenLineTarget, symbolAtLine } from "./codeQuickOpenProviders";

describe("Quick Open prefixes", () => {
  it("switches mode on the VS Code prefixes and keeps plain text as a file search", () => {
    expect(parseQuickOpenMode(">format")).toEqual({ mode: "commands", text: "format" });
    expect(parseQuickOpenMode("@ render")).toEqual({ mode: "symbols", text: "render" });
    expect(parseQuickOpenMode("#Clamp")).toEqual({ mode: "workspaceSymbols", text: "Clamp" });
    expect(parseQuickOpenMode(":42")).toEqual({ mode: "line", text: "42" });
    expect(parseQuickOpenMode("edt main")).toEqual({ mode: "editors", text: "main" });
    expect(parseQuickOpenMode("src/app.ts:12")).toEqual({ mode: "files", text: "src/app.ts:12" });
  });

  it("accepts line and line:column for Go to Line, and rejects anything else", () => {
    expect(quickOpenLineTarget("42")).toEqual({ line: 42, column: 1 });
    expect(quickOpenLineTarget("42:7")).toEqual({ line: 42, column: 7 });
    expect(quickOpenLineTarget("abc")).toBeNull();
  });

  it("reads both LSP symbol shapes, nesting containers and skipping files outside the workspace", () => {
    const documentSymbols = [{ name: "Panel", kind: 5, range: { start: { line: 3, character: 0 } }, selectionRange: { start: { line: 3, character: 13 } }, children: [{ name: "render", kind: 6, range: { start: { line: 9, character: 2 } } }] }];
    expect(flattenLspSymbols(documentSymbols, "src/panel.ts", "/repo")).toEqual([
      { name: "Panel", kind: "class", container: "", path: "src/panel.ts", line: 4, column: 14 },
      { name: "render", kind: "method", container: "Panel", path: "src/panel.ts", line: 10, column: 3 },
    ]);
    const workspaceSymbols = [
      { name: "clamp", kind: 12, location: { uri: "file:///repo/src/math.ts", range: { start: { line: 92, character: 16 } } } },
      { name: "outside", kind: 12, location: { uri: "file:///other/x.ts", range: { start: { line: 0, character: 0 } } } },
    ];
    expect(flattenLspSymbols(workspaceSymbols, "", "/repo").map((symbol) => `${symbol.path}:${symbol.line}`)).toEqual(["src/math.ts:93"]);
  });

  it("finds the innermost symbol around the cursor for the breadcrumb, or the nearest one above when only start lines are known", () => {
    const ranged = [
      { name: "Panel", kind: "class", container: "", path: "a.ts", line: 4, column: 1, endLine: 40 },
      { name: "render", kind: "method", container: "Panel", path: "a.ts", line: 10, column: 3, endLine: 20 },
    ];
    expect(symbolAtLine(ranged, 15)?.name).toBe("render");
    expect(symbolAtLine(ranged, 30)?.name).toBe("Panel");
    expect(symbolAtLine(ranged, 2)).toBeNull();
    const scanned = [{ name: "a", kind: "function", container: "", path: "a.ts", line: 3, column: 1 }, { name: "b", kind: "function", container: "", path: "a.ts", line: 12, column: 1 }];
    expect(symbolAtLine(scanned, 14)?.name).toBe("b");
  });
});
