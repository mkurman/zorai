import { describe, expect, it } from "vitest";
import { buildDaemonAgentConfig } from "./agentDaemonConfig";
import { DEFAULT_AGENT_SETTINGS, normalizeAgentSettingsFromSource } from "./agentStore/settings";

describe("code review settings persistence", () => {
  it("sends the chosen reviewer to the daemon config so it survives a reload instead of reverting to the default", () => {
    const config = buildDaemonAgentConfig({ ...DEFAULT_AGENT_SETTINGS, code_review_agent: "deepseekorrr-baby", code_review_commit_window: 9 });
    const reloaded = normalizeAgentSettingsFromSource(config as never);
    expect(reloaded.code_review_agent).toBe("deepseekorrr-baby");
    expect(reloaded.code_review_commit_window).toBe(9);
  });
});
