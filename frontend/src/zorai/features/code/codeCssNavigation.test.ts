import { describe, expect, it } from "vitest";
import { cssClassAtPosition, cssClassSelectorPattern } from "./codeCssNavigation";

const at = (line: string, marker: string) => cssClassAtPosition(line, line.indexOf(marker) + 2);

describe("Ctrl+click on CSS class names", () => {
  it("recognises class names inside className strings, conditionals and helpers", () => {
    expect(at('<div className="zorai-code-split__pane is-focused">', "zorai-code-split__pane")?.name).toBe("zorai-code-split__pane");
    expect(at('<div className="a is-focused">', "is-focused")?.name).toBe("is-focused");
    expect(at('<b className={open ? "panel panel--open" : "panel"}>', "panel--open")?.name).toBe("panel--open");
    expect(at("<i className={cn('icon', active && 'icon--on')}>", "icon--on")?.name).toBe("icon--on");
    expect(at('<p class="lead">', "lead")?.name).toBe("lead");
  });

  it("ignores identifiers that only sit next to a className, so normal Go to Definition still wins", () => {
    expect(at('<b className={open ? "panel" : fallbackClass}>', "fallbackClass")).toBeNull();
    expect(at('const label = "panel";', "panel")).toBeNull();
  });

  it("matches the exact selector, not longer class names that share the prefix", () => {
    const pattern = new RegExp(cssClassSelectorPattern("panel"));
    expect(pattern.test(".panel {")).toBe(true);
    expect(pattern.test(".panel:hover,")).toBe(true);
    expect(pattern.test(".panel--open {")).toBe(false);
    expect(pattern.test(".panel_x {")).toBe(false);
  });
});
