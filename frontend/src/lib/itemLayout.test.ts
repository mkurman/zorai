import { describe, expect, it } from "vitest";
import { moveItem, orderedItemIds, setItemHidden, visibleItemIds } from "./itemLayout";

const defaults = ["code", "threads", "goals", "tools"] as const;

describe("item layout", () => {
  it("applies the saved order and hides what the user hid", () => {
    expect(visibleItemIds(defaults, { order: ["tools", "code"], hidden: ["goals"] })).toEqual(["tools", "code", "threads"]);
  });

  it("keeps items added in a later app version visible instead of losing them behind an old saved order", () => {
    expect(orderedItemIds([...defaults, "skills"], { order: ["tools", "code", "threads", "goals"], hidden: [] })).toEqual(["tools", "code", "threads", "goals", "skills"]);
  });

  it("ignores ids that no longer exist or appear twice in a hand-edited settings file", () => {
    expect(orderedItemIds(defaults, { order: ["gone", "goals", "goals"], hidden: [] })).toEqual(["goals", "code", "threads", "tools"]);
    expect(visibleItemIds(defaults, undefined)).toEqual([...defaults]);
  });

  it("moves an item one step and stops at the ends", () => {
    expect(moveItem(defaults, undefined, "threads", -1).order).toEqual(["threads", "code", "goals", "tools"]);
    expect(moveItem(defaults, undefined, "code", -1).order).toEqual([...defaults]);
    expect(moveItem(defaults, { order: [], hidden: ["goals"] }, "tools", 1)).toEqual({ order: [...defaults], hidden: ["goals"] });
  });

  it("hides and shows an item without touching the order", () => {
    const hidden = setItemHidden({ order: ["tools"], hidden: [] }, "goals", true);
    expect(hidden).toEqual({ order: ["tools"], hidden: ["goals"] });
    expect(setItemHidden(hidden, "goals", false)).toEqual({ order: ["tools"], hidden: [] });
  });
});
