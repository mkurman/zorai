import { describe, expect, it } from "vitest";
import { dragScrollLeft, scrollEdges } from "./useDragScroll";

describe("drag to scroll the session bar", () => {
  it("scrolls opposite to the pointer, like grabbing and pulling the row", () => {
    expect(dragScrollLeft(100, 300, 250)).toBe(150);
    expect(dragScrollLeft(100, 300, 340)).toBe(60);
  });

  it("fades only the edges that actually hide more pills", () => {
    expect(scrollEdges(0, 300, 300)).toEqual({ start: false, end: false });
    expect(scrollEdges(0, 500, 300)).toEqual({ start: false, end: true });
    expect(scrollEdges(100, 500, 300)).toEqual({ start: true, end: true });
    expect(scrollEdges(200, 500, 300)).toEqual({ start: true, end: false });
  });
});
