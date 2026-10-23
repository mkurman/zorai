import { describe, expect, it } from "vitest";
import { buildDaemonAgentConfig, diffDaemonConfigEntries } from "./agentDaemonConfig";
import { DEFAULT_AGENT_SETTINGS, normalizeAgentSettingsFromSource } from "./agentStore/settings";

describe("heartbeat settings persistence", () => {
  it("heartbeats start off, and turning them on survives a reload through the daemon's heartbeat_enabled key", () => {
    expect(DEFAULT_AGENT_SETTINGS.heartbeat_enabled).toBe(false);
    expect(normalizeAgentSettingsFromSource({ heartbeat_interval_mins: 30 } as never).heartbeat_enabled).toBe(false);
    const config = buildDaemonAgentConfig({ ...DEFAULT_AGENT_SETTINGS, heartbeat_enabled: true, heartbeat_interval_mins: 120 });
    expect(config).toMatchObject({ heartbeat_enabled: true, heartbeat_interval_mins: 120 });
    const reloaded = normalizeAgentSettingsFromSource(config as never);
    expect(reloaded.heartbeat_enabled).toBe(true);
    expect(reloaded.heartbeat_interval_mins).toBe(120);
  });

  it("an empty cron box clears the override instead of sending a schedule the daemon cannot parse", () => {
    const config = buildDaemonAgentConfig({ ...DEFAULT_AGENT_SETTINGS, heartbeat_cron: "   " }) as Record<string, unknown>;
    expect(config.heartbeat_cron).toBeNull();
    expect(normalizeAgentSettingsFromSource({ ...config, heartbeat_cron: null } as never).heartbeat_cron).toBe("");
  });

  it("toggling one check only patches that check, leaving daemon-side thresholds and weights alone", () => {
    const before = buildDaemonAgentConfig(DEFAULT_AGENT_SETTINGS);
    const after = buildDaemonAgentConfig({
      ...DEFAULT_AGENT_SETTINGS,
      heartbeat_checks: { ...DEFAULT_AGENT_SETTINGS.heartbeat_checks, repo_changes_enabled: false },
    });
    expect(diffDaemonConfigEntries(before, after)).toEqual([
      { keyPath: "/heartbeat_checks/repo_changes_enabled", value: false },
    ]);
  });

  it("quiet hours outside 0-23 from a hand-edited config are treated as off", () => {
    const reloaded = normalizeAgentSettingsFromSource({ quiet_hours_start: 25, quiet_hours_end: 7 } as never);
    expect(reloaded.quiet_hours_start).toBeNull();
    expect(reloaded.quiet_hours_end).toBe(7);
  });
});
