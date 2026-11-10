import { describe, expect, it } from "vitest";
import { getDefaultZoraiView, zoraiNavItems } from "./navigation";

describe("Zorai navigation", () => {
  it("opens to threads by default", () => {
    expect(getDefaultZoraiView()).toBe("threads");
  });

  it("opens the chosen startup tab, or the last opened one, and falls back to threads for anything unknown", () => {
    expect(getDefaultZoraiView("code")).toBe("code");
    expect(getDefaultZoraiView("last", "goals")).toBe("goals");
    expect(getDefaultZoraiView("last", null)).toBe("threads");
    expect(getDefaultZoraiView("last", "nowhere")).toBe("threads");
    expect(getDefaultZoraiView("database")).toBe("threads");
  });

  it("exposes the agent-centric top-level destinations with Code above Threads", () => {
    expect(zoraiNavItems.map((item) => item.id)).toEqual([
      "code",
      "threads",
      "goals",
      "workspaces",
      "database",
      "tools",
      "skills",
      "activity",
      "settings",
    ]);
  });

  it("defines shell-facing labels for every destination", () => {
    for (const item of zoraiNavItems) {
      expect(item.label.trim().length).toBeGreaterThan(0);
      expect(item.railLabel.trim().length).toBeGreaterThan(0);
      expect(item.description.trim().length).toBeGreaterThan(0);
    }
  });

  it("uses icon identifiers instead of text abbreviations in the global rail", () => {
    for (const item of zoraiNavItems) {
      expect(Object.hasOwn(item, "icon")).toBe(true);
      expect(Object.hasOwn(item, "shortLabel")).toBe(false);
    }
  });
});
