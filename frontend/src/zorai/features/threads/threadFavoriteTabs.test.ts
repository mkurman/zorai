import { describe, expect, it } from "vitest";
import { fixedThreadTabs, searchThreadTabs, visibleThreadTabs, type ThreadFilterTab } from "./threadFilterModel";

const allTabs: Array<{ id: ThreadFilterTab; label: string }> = [
  ...fixedThreadTabs,
  { id: "agent:atlas", label: "Atlas Researcher" },
  { id: "agent:coder", label: "Coder" },
];

describe("thread rail favorites", () => {
  it("shows only pinned tabs, in the order the operator pinned them", () => {
    const visible = visibleThreadTabs(allTabs, ["agent:coder", "rarog"], "rarog");
    expect(visible.map((item) => item.id)).toEqual(["agent:coder", "rarog"]);
  });

  it("lets built-in tabs like Svarog be unpinned without hiding the filter that is currently applied", () => {
    const visible = visibleThreadTabs(allTabs, ["goals"], "svarog");
    expect(visible.map((item) => item.id)).toEqual(["goals", "svarog"]);
  });

  it("skips pinned subagents that no longer exist instead of rendering empty chips", () => {
    const visible = visibleThreadTabs(allTabs, ["agent:deleted", "weles"], "weles");
    expect(visible.map((item) => item.id)).toEqual(["weles"]);
  });

  it("finds agents by display name regardless of case", () => {
    expect(searchThreadTabs(allTabs, "  atlas ").map((item) => item.id)).toEqual(["agent:atlas"]);
    expect(searchThreadTabs(allTabs, "")).toHaveLength(allTabs.length);
  });
});
