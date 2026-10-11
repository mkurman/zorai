import { afterEach, describe, expect, it, vi } from "vitest";

const bridge = {
  agentGetConciergeConfig: vi.fn(async () => ({
    enabled: true,
    detail_level: "proactive_triage",
    provider: "anthropic",
    model: "claude-haiku-5-5",
    reasoning_effort: "low",
  })),
};

vi.mock("../bridge", () => ({ getBridge: () => bridge }));
vi.mock("../daemonConfig", () => ({ getDaemonAgentConfig: async () => null }));

import { hydrateAgentStore } from "./hydrate";
import { useAgentStore } from "./store";
import { DEFAULT_CONCIERGE_CONFIG } from "./settingsActions";
import { draftThreadForOwnerSnapshot, snapshotThreadOwnerRuntimeProfile } from "@/zorai/features/threads/threadOwnerRuntime";

afterEach(() => {
  useAgentStore.setState({ conciergeConfig: DEFAULT_CONCIERGE_CONFIG });
});

describe("startup concierge hydration", () => {
  it("loads Rarog's runtime so new Rarog threads don't pin Svarog's provider before Settings is opened", async () => {
    await hydrateAgentStore();

    const state = useAgentStore.getState();
    const profile = snapshotThreadOwnerRuntimeProfile(
      draftThreadForOwnerSnapshot({ agentId: "rarog", agentName: "Rarog" }, state.agentSettings.agent_name),
      state.subAgents,
      state.agentSettings,
      state.conciergeConfig,
    );

    expect(bridge.agentGetConciergeConfig).toHaveBeenCalled();
    expect(profile.profileProvider).toBe("anthropic");
    expect(profile.profileModel).toBe("claude-haiku-5-5");
    expect(profile.profileReasoningEffort).toBe("low");
  });
});
