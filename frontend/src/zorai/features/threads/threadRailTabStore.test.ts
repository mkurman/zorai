import { beforeEach, describe, expect, it } from "vitest";
import { fixedThreadTabs } from "./threadFilterModel";
import { persistedThreadRailState, useThreadRailTabStore } from "./threadRailTabStore";

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

describe("thread list filters remembered across restarts", () => {
  it("saves the picked agent tab, open filter panel and date range", () => {
    const store = useThreadRailTabStore.getState();
    store.setTab("rarog");
    store.setFiltersOpen((open) => !open);
    store.setDateFilter("custom");
    store.setFromDate("2026-11-01");
    store.setToDate("2026-11-18");
    const saved = persistedThreadRailState(useThreadRailTabStore.getState());
    expect(saved).toMatchObject({ tab: "rarog", filtersOpen: true, dateFilter: "custom", fromDate: "2026-11-01", toDate: "2026-11-18" });
  });
});
