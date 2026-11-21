import { describe, expect, it } from "vitest";
import { mergeSearchItems } from "./projectSearchItems";

describe("unified file search results", () => {
  it("lists file-name matches before text matches so Enter opens the file you named", () => {
    const items = mergeSearchItems(["src/config.ts"], [{ path: "src/app.ts", line: 3, column: 1, preview: "import config" }]);
    expect(items.map((item) => item.kind)).toEqual(["file", "text"]);
  });

  it("caps file-name matches so text matches stay reachable", () => {
    const files = Array.from({ length: 80 }, (_, index) => `f${index}.ts`);
    expect(mergeSearchItems(files, []).length).toBe(30);
  });
});
