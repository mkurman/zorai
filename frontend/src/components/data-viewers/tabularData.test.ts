import { describe, expect, it } from "vitest";
import { markdownTableModel } from "./MarkdownTable";
import { parseTabular, sortRowIndexes, tabularFormatForFence, tabularFormatForPath, tableToCsv } from "./tabularData";

describe("parseTabular", () => {
  it("keeps quoted commas inside one CSV cell so columns do not shift", () => {
    const table = parseTabular('name,notes\nAspirin,"low dose, daily"\n', "csv");
    expect(table?.columns).toEqual(["name", "notes"]);
    expect(table?.rows).toEqual([["Aspirin", "low dose, daily"]]);
  });

  it("drops the cut-off last line of a truncated preview instead of showing a broken row", () => {
    const table = parseTabular('{"id":1}\n{"id":2}\n{"id":', "jsonl", true);
    expect(table?.rows).toEqual([["1"], ["2"]]);
    expect(table?.errors).toEqual([]);
  });

  it("unions JSONL keys so records with extra fields still line up", () => {
    const table = parseTabular('{"a":1}\n{"a":2,"b":{"x":true}}\nnot json\n', "jsonl");
    expect(table?.columns).toEqual(["a", "b"]);
    expect(table?.rows).toEqual([["1", ""], ["2", '{"x":true}']]);
    expect(table?.errors).toEqual(["Line 3 is not valid JSON."]);
  });

  it("only treats JSON arrays as tables so config objects keep the raw view", () => {
    expect(parseTabular('{"setting":true}', "json")).toBeNull();
    expect(parseTabular('[{"id":1}]', "json")?.rows).toEqual([["1"]]);
  });
});

describe("table helpers", () => {
  it("sorts numbers numerically rather than as text", () => {
    const rows = [["10"], ["9"], ["100"]];
    expect(sortRowIndexes(rows, 0, "asc")).toEqual([1, 0, 2]);
    expect(sortRowIndexes(rows, 0, "desc")).toEqual([2, 0, 1]);
  });

  it("detects formats from extensions and fences", () => {
    expect(tabularFormatForPath("/data/out.NDJSON")).toBe("jsonl");
    expect(tabularFormatForPath("/data/notes.txt")).toBeNull();
    expect(tabularFormatForFence("TSV")).toBe("tsv");
    expect(tabularFormatForFence("json")).toBeNull();
  });

  it("exports CSV with quoting so copied tables paste back correctly", () => {
    expect(tableToCsv(["a", "b"], [["x, y", 'say "hi"']])).toBe('a,b\r\n"x, y","say ""hi"""');
  });

  it("reads markdown table cells from the hast tree, including formatted text", () => {
    const cell = (tagName: string, text: string) => ({
      type: "element",
      tagName,
      children: [{ type: "element", tagName: "strong", children: [{ type: "text", value: text }] }],
    });
    const node = {
      type: "element",
      tagName: "table",
      children: [
        { type: "element", tagName: "thead", children: [{ type: "element", tagName: "tr", children: [cell("th", "Drug")] }] },
        { type: "element", tagName: "tbody", children: [{ type: "element", tagName: "tr", children: [cell("td", "Aspirin")] }] },
      ],
    };
    expect(markdownTableModel(node)).toEqual({ columns: ["Drug"], rows: [["Aspirin"]] });
  });
});
