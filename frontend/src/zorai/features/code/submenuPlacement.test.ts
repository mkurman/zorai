import { describe, expect, it } from "vitest";
import { submenuPlacement, withScreenshotItem } from "./CodeToolbarMenu";

const viewport = { width: 1600, height: 900 };

describe("right-click submenu placement", () => {
  it("opens to the left when the submenu would run off the right edge", () => {
    expect(submenuPlacement({ right: 1820, bottom: 400, width: 220 }, 1390, viewport)).toEqual({ flipX: true, shiftY: 0 });
  });

  it("stays on the right when there is room", () => {
    expect(submenuPlacement({ right: 900, bottom: 400, width: 220 }, 400, viewport).flipX).toBe(false);
  });

  it("moves up by the overflow when it would run off the bottom", () => {
    expect(submenuPlacement({ right: 900, bottom: 950, width: 220 }, 400, viewport).shiftY).toBe(54);
  });
});

describe("right-click menus offer a screenshot", () => {
  it("adds Take Screenshot last, after a separator, without duplicating it", () => {
    const items = withScreenshotItem([{ key: "copy", label: "Copy" }]);
    expect(items.map((item) => item.label)).toEqual(["Copy", "Take Screenshot"]);
    expect(items[1].separatorBefore).toBe(true);
    expect(withScreenshotItem(items)).toBe(items);
  });
});
