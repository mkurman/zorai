import { describe, expect, it } from "vitest";
import { inlineValues } from "./codeDebugDecorations";

describe("inline debug values", () => {
  it("shows each local next to the nearest line above the paused line that mentions it", () => {
    const lines = ["fn main() {", "  let count = 1;", "  let total = count + 2;", "  obj.count = 9;", "  println!(total);"];
    const values = inlineValues(lines, 5, [{ name: "count", value: "1" }, { name: "total", value: "3" }, { name: "self.x", value: "?" }]);
    expect(values.get(3)).toBe("count = 1");
    expect(values.get(5)).toBe("total = 3");
    expect(values.has(4)).toBe(false);
  });
});
