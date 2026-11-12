import { beforeEach, describe, expect, it } from "vitest";
import { visibleItemIds } from "@/lib/itemLayout";
import { matchesPluginSearch, monogram, pinnedRailIds, pluginNameFromRailId, sortPluginsByFavorite, usePluginPrefsStore } from "./pluginPrefsStore";

describe("plugin preferences", () => {
  beforeEach(() => usePluginPrefsStore.setState({ favorites: [], pinned: [], viewMode: "grid" }));

  it("puts favourites first and keeps everything else alphabetical", () => {
    const sorted = sortPluginsByFavorite([{ name: "uniprot" }, { name: "github" }, { name: "jira" }, { name: "chembl" }], ["jira", "uniprot"]);
    expect(sorted.map((plugin) => plugin.name)).toEqual(["jira", "uniprot", "chembl", "github"]);
  });

  it("toggles favourites and pins independently", () => {
    const { toggleFavorite, togglePinned } = usePluginPrefsStore.getState();
    toggleFavorite("github");
    togglePinned("github");
    togglePinned("jira");
    toggleFavorite("github");
    expect(usePluginPrefsStore.getState()).toMatchObject({ favorites: [], pinned: ["github", "jira"] });
  });

  it("finds plugins by name, description, panel or command titles, with every word required", () => {
    const pubmed = { name: "pubmed-database", description: "Literature search", panels: [], commands: [{ title: "Ask with PubMed" }] };
    expect(matchesPluginSearch(pubmed, "literature")).toBe(true);
    expect(matchesPluginSearch(pubmed, "ask pubmed")).toBe(true);
    expect(matchesPluginSearch(pubmed, "pubmed jira")).toBe(false);
    expect(matchesPluginSearch(pubmed, "  ")).toBe(true);
  });

  it("gives every plugin a stable two-letter monogram and colour for when it has no icon", () => {
    expect(monogram("pubmed-database").letters).toBe("PD");
    expect(monogram("github").letters).toBe("GI");
    expect(monogram("github")).toEqual(monogram("github"));
  });

  it("places pinned plugins in the same rail order as views, appending new pins and forgetting unpinned ones", () => {
    const views = ["code", "threads", "plugins"];
    const ids = [...views, ...pinnedRailIds(["gmail", "jira"])];
    expect(visibleItemIds(ids, { order: ["plugin:gmail", "code", "threads", "plugins"], hidden: [] })).toEqual(["plugin:gmail", "code", "threads", "plugins", "plugin:jira"]);
    expect(visibleItemIds([...views, ...pinnedRailIds(["jira"])], { order: ["plugin:gmail", "code", "threads", "plugins", "plugin:jira"], hidden: [] })).toEqual(["code", "threads", "plugins", "plugin:jira"]);
    expect(pluginNameFromRailId("plugin:pubmed-database")).toBe("pubmed-database");
    expect(pluginNameFromRailId("plugins")).toBeNull();
  });
});
