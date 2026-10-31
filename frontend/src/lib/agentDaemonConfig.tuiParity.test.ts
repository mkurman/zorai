import { describe, expect, it } from "vitest";
import { buildDaemonAgentConfig, diffDaemonConfigEntries } from "./agentDaemonConfig";
import { DEFAULT_AGENT_SETTINGS, normalizeAgentSettingsFromSource } from "./agentStore/settings";

const reload = (settings: typeof DEFAULT_AGENT_SETTINGS) =>
  normalizeAgentSettingsFromSource(buildDaemonAgentConfig(settings) as never);

describe("settings the GUI gained from the TUI", () => {
  it("turning off a tool the GUI used to force on reaches the daemon and survives a reload", () => {
    const off = { ...DEFAULT_AGENT_SETTINGS, enable_file_operations_tool: false, enable_system_info_tool: false, enable_gateway_messaging_tool: false };
    expect(diffDaemonConfigEntries(buildDaemonAgentConfig(DEFAULT_AGENT_SETTINGS), buildDaemonAgentConfig(off))).toEqual([
      { keyPath: "/tools/gateway_messaging", value: false },
      { keyPath: "/tools/file_operations", value: false },
      { keyPath: "/tools/system_info", value: false },
    ]);
    const reloaded = reload(off);
    expect([reloaded.enable_file_operations_tool, reloaded.enable_system_info_tool, reloaded.enable_gateway_messaging_tool]).toEqual([false, false, false]);
  });

  it("writes consolidation under the keys the daemon reads, not the TUI's dropped decay_half_life_hours", () => {
    const config = buildDaemonAgentConfig({ ...DEFAULT_AGENT_SETTINGS, consolidation_memory_decay_half_life_hours: 120 }) as Record<string, any>;
    expect(config.consolidation.memory_decay_half_life_hours).toBe(120);
    expect(config.consolidation).not.toHaveProperty("decay_half_life_hours");
    expect(reload({ ...DEFAULT_AGENT_SETTINGS, consolidation_memory_decay_half_life_hours: 120 }).consolidation_memory_decay_half_life_hours).toBe(120);
  });

  it("automatic tier and default Claude permissions clear the daemon override instead of storing an empty string", () => {
    const config = buildDaemonAgentConfig(DEFAULT_AGENT_SETTINGS) as Record<string, any>;
    expect(config.tier.user_override).toBeNull();
    expect(config.claude_permission_mode).toBeNull();
    const chosen = reload({ ...DEFAULT_AGENT_SETTINGS, tier_user_override: "power_user", claude_permission_mode: "plan" });
    expect(chosen.tier_user_override).toBe("power_user");
    expect(chosen.claude_permission_mode).toBe("plan");
  });

  it("unknown tier or permission values from a hand-edited config fall back to automatic and CLI default", () => {
    const reloaded = normalizeAgentSettingsFromSource({ tier: { user_override: "wizard" }, claude_permission_mode: "sudo" } as never);
    expect(reloaded.tier_user_override).toBe("");
    expect(reloaded.claude_permission_mode).toBe("");
  });
});

describe("per-provider reasoning effort", () => {
  it("changing Svarog's effort does not overwrite the effort stored for the embedding provider", () => {
    const daemon = {
      provider: "openai",
      reasoning_effort: "high",
      providers: { anthropic: { model: "claude-x", reasoning_effort: "low" } },
      semantic: { embedding: { provider: "anthropic" } },
    };
    const settings = normalizeAgentSettingsFromSource(daemon as never);
    const before = buildDaemonAgentConfig(settings) as Record<string, any>;
    const after = buildDaemonAgentConfig({ ...settings, reasoning_effort: "max" }) as Record<string, any>;

    expect(after.providers.openai.reasoning_effort).toBe("max");
    expect(after.providers.anthropic.reasoning_effort).toBe("low");
    expect(diffDaemonConfigEntries(before, after).map((entry) => entry.keyPath)).not.toContain("/providers/anthropic/reasoning_effort");
  });
});

describe("subagent budget extension cap", () => {
  it("an unlimited cap of 0 reaches the daemon and is not replaced by the default 5 on reload", () => {
    const unlimited = { ...DEFAULT_AGENT_SETTINGS, max_subagent_budget_extensions: 0 };
    expect(diffDaemonConfigEntries(buildDaemonAgentConfig(DEFAULT_AGENT_SETTINGS), buildDaemonAgentConfig(unlimited))).toEqual([
      { keyPath: "/max_subagent_budget_extensions", value: 0 },
    ]);
    expect(reload(unlimited).max_subagent_budget_extensions).toBe(0);
    expect(normalizeAgentSettingsFromSource({} as never).max_subagent_budget_extensions).toBe(5);
  });
});
