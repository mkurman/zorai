import { describe, expect, it } from "vitest";
import { commandParams, enabledPluginUi, fillPromptTemplate } from "./pluginUiStore";

const manifest = (name: string) => ({ name, version: "1", description: "", dirName: name, commands: [], panels: [], problems: [] }) as ZoraiPluginUiManifest;

describe("plugin ui store", () => {
  it("only shows UI from plugins the daemon has enabled, and lists the rest as disabled", () => {
    expect(enabledPluginUi([manifest("github"), manifest("jira")], [{ name: "github", enabled: true }, { name: "jira", enabled: false }])).toEqual({ plugins: [manifest("github")], disabled: ["jira"] });
    expect(enabledPluginUi([manifest("github")], [])).toEqual({ plugins: [], disabled: ["github"] });
  });

  it("falls back to showing everything when the daemon list is unavailable, so calls fail visibly instead of the UI vanishing", () => {
    expect(enabledPluginUi([manifest("github")], null).plugins).toHaveLength(1);
  });

  it("merges typed inputs over the declared params, ignoring empty fields", () => {
    const command = { id: "c", title: "C", endpoint: "e", params: { state: "open", per_page: "20" }, inputs: [], output: "panel", placement: [] } as ZoraiPluginUiCommand;
    expect(commandParams(command, { state: "closed", repo: "zorai", per_page: "  " })).toEqual({ state: "closed", per_page: "20", repo: "zorai" });
  });

  it("fills prompt templates from the typed inputs and leaves no unreplaced placeholders behind", () => {
    expect(fillPromptTemplate("Use the pubmed-database plugin to: {{ request }} (limit {{limit}})", { request: " find CRISPR reviews ", limit: "10" })).toBe("Use the pubmed-database plugin to: find CRISPR reviews (limit 10)");
    expect(fillPromptTemplate("Look up {{gene}} {{missing}}", { gene: "BRCA1" })).toBe("Look up BRCA1");
  });
});
