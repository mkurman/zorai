import { describe, expect, it } from "vitest";
import { fitColumnWidths, isNumericCell, layoutTable, opaqueColor, timestampedFileName, wrapText } from "./tablePng";

const charWidth = (text: string) => text.length * 10;

describe("table png export", () => {
  it("keeps narrow columns at their natural width and squeezes only the wide ones", () => {
    expect(fitColumnWidths([100, 900, 800], 1000)).toEqual([100, 450, 450]);
    expect(fitColumnWidths([100, 200], 1000)).toEqual([100, 200]);
  });

  it("wraps long cells on words and splits words that cannot fit a line", () => {
    expect(wrapText("matched five million params", 120, charWidth)).toEqual(["matched five", "million", "params"]);
    expect(wrapText("abcdefghij", 40, charWidth)).toEqual(["abcd", "efgh", "ij"]);
    expect(wrapText("", 40, charWidth)).toEqual([""]);
  });

  it("produces a fixed-width share image no shorter than a link-preview card, however wide the table is", () => {
    const wide = Array.from({ length: 6 }, (_, index) => `Column ${index} with a very long descriptive header`);
    const layout = layoutTable(wide, [wide], (text) => charWidth(text));
    expect(layout.width).toBe(1200);
    expect(layout.widths.reduce((sum, width) => sum + width, 0)).toBeLessThanOrEqual(1200 - 96);
    expect(layout.height).toBeGreaterThanOrEqual(628);
    expect(layout.rows[0].cells[0].length).toBeGreaterThan(1);
  });

  it("drops alpha so translucent background-theme panels export as solid colour", () => {
    expect(opaqueColor("rgba(25, 25, 25, 0.85)")).toBe("rgb(25, 25, 25)");
    expect(opaqueColor("#191919")).toBe("#191919");
  });
});

describe("export file names", () => {
  it("names exports after the message time so tables and diagrams from different replies never collide", () => {
    const time = new Date(2026, 10, 21, 9, 5, 7).getTime();
    expect(timestampedFileName("table", time)).toBe("table-2026-11-21-090507");
    expect(timestampedFileName("diagram", time)).toBe("diagram-2026-11-21-090507");
    expect(timestampedFileName("table", null)).toMatch(/^table-\d{4}-\d{2}-\d{2}-\d{6}$/);
  });
});

describe("table png styling", () => {
  it("right-aligns numbers so values line up in the exported image, while text stays left-aligned", () => {
    for (const value of ["85.37", "1,197.584", "100.0%", "5.13M", "−0.115", "19.68 h", "$12", "4×"]) expect(isNumericCell(value), value).toBe(true);
    for (const value of ["BrainCell v2", "English", "v1.2 beta", "HRM (fixed)", ""]) expect(isNumericCell(value), value).toBe(false);
  });
});
