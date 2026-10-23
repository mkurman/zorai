import { describe, expect, it } from "vitest";
import { getDefaultZoraiSettingsTab, zoraiSettingsGroups, zoraiSettingsTabs, zoraiSettingsTabsByGroup } from "./settingsTabs";

describe("Zorai settings tabs", () => {
  it("opens provider settings by default like the TUI settings overlay", () => {
    expect(getDefaultZoraiSettingsTab()).toBe("auth");
  });

  it("groups every tab under a sidebar heading so nothing is unreachable from the rail", () => {
    const grouped = zoraiSettingsTabsByGroup();
    expect(grouped.map((group) => group.id)).toEqual(zoraiSettingsGroups.map((group) => group.id));
    expect(grouped.every((group) => group.tabs.length > 0)).toBe(true);
    expect(grouped.flatMap((group) => group.tabs.map((tab) => tab.id)).sort()).toEqual(zoraiSettingsTabs.map((tab) => tab.id).sort());
  });

  it("never reuses a tab id, title, or icon, so every rail entry is unambiguous", () => {
    const unique = (values: string[]) => new Set(values).size === values.length;
    expect(unique(zoraiSettingsTabs.map((tab) => tab.id))).toBe(true);
    expect(unique(zoraiSettingsTabs.map((tab) => tab.title))).toBe(true);
    expect(unique(zoraiSettingsTabs.map((tab) => tab.icon))).toBe(true);
  });

  it("orders the sidebar from agents to system settings", () => {
    expect(zoraiSettingsTabsByGroup().map((group) => [group.title, group.tabs.map((tab) => tab.title)])).toEqual([
      ["Agents", ["Providers", "Svarog", "Rarog", "Sub-agents"]],
      ["Capabilities", ["Tools", "Skills", "Web", "Audio & Images", "Memory"]],
      ["Automation", ["Heartbeat", "Proactivity", "Workflows", "Gateway"]],
      ["Runtime", ["Chat", "Compaction", "Execution", "Safety", "MLflow"]],
      ["System", ["Storage", "Plugins", "Interface", "Import", "About"]],
    ]);
  });
});
