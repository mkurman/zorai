import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { ShortcutsPanel } from "./ShortcutsPanel";
import { splitBindingKeys } from "./KeymapTable";

describe("app keyboard shortcuts table", () => {
  it("shows app shortcuts in the same keymap table as Code, with key caps and their source", () => {
    const html = renderToStaticMarkup(<ShortcutsPanel />);
    expect(html).toContain('class="zorai-keymap__table"');
    expect(html).toContain("<kbd>Ctrl</kbd><kbd>Shift</kbd><kbd>P</kbd>");
    expect(html).toContain(">Default<");
  });

  it("keeps the plus key as its own key cap", () => {
    expect(splitBindingKeys("Ctrl++")).toEqual(["Ctrl", "+"]);
    expect(splitBindingKeys("Ctrl+Shift+P")).toEqual(["Ctrl", "Shift", "P"]);
  });
});
