import { describe, expect, it, vi } from "vitest";
import { describePickedElement, distinctiveClasses, locatePickedElement, screenshotFile, sourcePathFromDebug, type PickedElement } from "./domToCode";

const picked: PickedElement = {
  url: "http://localhost:5173/",
  tag: "button",
  id: "",
  classes: ["zorai-mission__card", "is-active", "p-2"],
  text: "Open worktree",
  html: "<button class=\"zorai-mission__card is-active\">Open worktree</button>",
  component: "WorktreeMissionControl",
  source: null,
  styles: { display: "flex", color: "rgb(255, 255, 255)", margin: "0px", position: "static" },
};

describe("DOM to code", () => {
  it("ignores state classes like is-active when looking for the element's source", () => {
    expect(distinctiveClasses(picked.classes)).toEqual(["zorai-mission__card"]);
  });

  it("maps React debug sources from the dev server back into the workspace", () => {
    expect(sourcePathFromDebug("/repo/src/App.tsx?t=123", "/repo")).toBe("src/App.tsx");
    expect(sourcePathFromDebug("/@fs/repo/src/a.tsx", "/repo")).toBe("src/a.tsx");
    expect(sourcePathFromDebug("/elsewhere/x.css", "/repo")).toBeNull();
  });

  it("finds the component by name first and the CSS rule by class", async () => {
    const search = vi.fn(async (query: string, options: { include: string }) => (
      query.includes("WorktreeMissionControl") ? [{ path: "src/WorktreeMissionControl.tsx", line: 6 }]
        : options.include.includes("css") ? [{ path: "src/zorai.css", line: 120 }] : []
    ));
    const located = await locatePickedElement("/repo", picked, search);
    expect(located).toEqual({
      component: { path: "src/WorktreeMissionControl.tsx", line: 6, reason: "component WorktreeMissionControl" },
      style: { path: "src/zorai.css", line: 120, reason: ".zorai-mission__card" },
    });
    const prompt = describePickedElement(picked, located);
    expect(prompt).toContain("`button.zorai-mission__card.is-active.p-2` (React <WorktreeMissionControl>)");
    expect(prompt).toContain("Component: src/WorktreeMissionControl.tsx:6");
    expect(prompt).toContain("display: flex");
    expect(prompt).not.toContain("position:");
    expect(prompt).not.toContain("margin:");
    expect(prompt.endsWith("Change: ")).toBe(true);
  });
});

describe("picked element screenshot", () => {
  it("becomes a PNG attachment named after the element, and is skipped when capture failed", () => {
    const file = screenshotFile({ ...picked, screenshot: "data:image/png;base64,iVBORw0KGgo=" });
    expect(file?.name).toBe("button-zorai-mission__card.png");
    expect(file?.type).toBe("image/png");
    expect(file?.size).toBe(8);
    expect(screenshotFile({ ...picked, screenshot: null })).toBeNull();
  });
});
