import { describe, expect, it } from "vitest";
import { pluginUiSummary } from "./pluginUiSummary";

const web = { id: "studio", title: "Studio", kind: "web", url: "zorai-plugin://x/panel/index.html", permissions: [] } as ZoraiPluginUiPanel;
const markdown = { id: "issues", title: "Issues", kind: "markdown", endpoint: "list", params: {}, inputs: [] } as ZoraiPluginUiPanel;
const command = { id: "c" } as ZoraiPluginUiCommand;

describe("plugin view summary", () => {
  it("tells interactive panels apart from read-only views so people know what they get", () => {
    expect(pluginUiSummary({ panels: [web], commands: [command, command] })).toBe("1 interactive panel · 2 commands");
    expect(pluginUiSummary({ panels: [markdown, markdown], commands: [] })).toBe("2 views");
    expect(pluginUiSummary({ panels: [], commands: [] })).toBe("");
  });
});
