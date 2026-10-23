import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { SettingsTabPanel } from "./SettingsPanels";
import { SettingsRail } from "./SettingsView";
import { zoraiSettingsTabs } from "./settingsTabs";

describe("settings tab panels", () => {
  it.each(zoraiSettingsTabs.map((tab) => [tab.id, tab.title] as const))("renders the %s tab without throwing", (tabId) => {
    const html = renderToStaticMarkup(<SettingsTabPanel activeTab={tabId} />);
    expect(html.length).toBeGreaterThan(0);
  });

  it("shows only the selected search provider's key instead of every provider's key at once", () => {
    const html = renderToStaticMarkup(<SettingsTabPanel activeTab="search" />);
    const keyRows = ["Firecrawl Key", "Exa Key", "Tavily Key"].filter((label) => html.includes(label));
    expect(keyRows.length).toBeLessThanOrEqual(1);
  });

  it("marks only the active tab in the sidebar and gives every entry an icon and label", () => {
    const html = renderToStaticMarkup(<SettingsRail activeTab="heartbeat" onSelectTab={() => undefined} />);
    expect(html.match(/aria-current="page"/g)).toHaveLength(1);
    expect(html.match(/zorai-settings-icon/g)).toHaveLength(zoraiSettingsTabs.length);
    for (const tab of zoraiSettingsTabs) {
      expect(html).toContain(`<span>${tab.title.replace("&", "&amp;")}</span>`);
    }
  });
});
