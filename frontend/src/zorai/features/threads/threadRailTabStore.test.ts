import { beforeEach, describe, expect, it } from "vitest";
import { fixedThreadTabs } from "./threadFilterModel";
import { useThreadRailTabStore } from "./threadRailTabStore";

beforeEach(() => {
  useThreadRailTabStore.setState({ tab: "svarog", favorites: fixedThreadTabs.map((item) => item.id) });
});

describe("thread rail favorite store", () => {
  it("defaults to the built-in tabs so existing users see the same rail after upgrading", () => {
    expect(useThreadRailTabStore.getState().favorites).toEqual(fixedThreadTabs.map((item) => item.id));
  });

  it("toggles a tab in and out of favorites", () => {
    useThreadRailTabStore.getState().toggleFavorite("agent:coder");
    expect(useThreadRailTabStore.getState().favorites).toContain("agent:coder");
    useThreadRailTabStore.getState().toggleFavorite("svarog");
    expect(useThreadRailTabStore.getState().favorites).not.toContain("svarog");
    useThreadRailTabStore.getState().toggleFavorite("agent:coder");
    expect(useThreadRailTabStore.getState().favorites).not.toContain("agent:coder");
  });
});
