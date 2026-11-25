import { describe, expect, it } from "vitest";
import { DEFAULT_AGENT_SETTINGS } from "@/lib/agentStore/settings";
import type { AgentThread, SubAgentDefinition } from "@/lib/agentStore";
import { resolveThreadOwnerRuntimeProfile, snapshotThreadOwnerRuntimeProfile, draftThreadForOwnerSnapshot } from "./threadOwnerRuntime";

function thread(partial: Partial<AgentThread> = {}): AgentThread {
  return {
    id: "t1",
    daemonThreadId: null,
    workspaceId: null,
    surfaceId: null,
    paneId: null,
    agent_name: "Svarog",
    title: "Chat",
    createdAt: 0,
    updatedAt: 0,
    messageCount: 0,
    totalInputTokens: 0,
    totalOutputTokens: 0,
    totalTokens: 0,
    compactionCount: 0,
    lastMessagePreview: "",
    ...partial,
  };
}

describe("thread owner runtime profile", () => {
  it("pins Leszy's own model on a new Leszy thread instead of Svarog's", () => {
    const settings = { ...DEFAULT_AGENT_SETTINGS, active_provider: "openai" as const, openai: { ...DEFAULT_AGENT_SETTINGS.openai, model: "gpt-5.5" } };
    const draft = draftThreadForOwnerSnapshot({ agentId: "leszy", agentName: "Leszy" }, "Svarog");
    const profile = snapshotThreadOwnerRuntimeProfile(draft, [], settings, {}, { provider: "anthropic", model: "claude-sonnet-5-5", reasoning_effort: "low" });
    expect(profile).toMatchObject({ profileProvider: "anthropic", profileModel: "claude-sonnet-5-5", profileReasoningEffort: "low" });
  });

  it("lets an unconfigured Leszy inherit Svarog, as its settings promise", () => {
    const settings = { ...DEFAULT_AGENT_SETTINGS, active_provider: "openai" as const, openai: { ...DEFAULT_AGENT_SETTINGS.openai, model: "gpt-5.5" } };
    const profile = resolveThreadOwnerRuntimeProfile(thread({ agent_name: "Leszy" }), [], settings, {}, {});
    expect(profile).toMatchObject({ ownerId: "leszy", provider: "openai", model: "gpt-5.5" });
  });

  it("loads the selected subagent provider instead of Svarog defaults", () => {
    const settings = {
      ...DEFAULT_AGENT_SETTINGS,
      active_provider: "openai" as const,
      openai: { ...DEFAULT_AGENT_SETTINGS.openai, model: "gpt-5.5" },
    };
    const subAgents = [{
      id: "mokosh",
      name: "Mokosh",
      provider: "z.ai-coding-plan",
      model: "glm-5",
      reasoning_effort: "medium",
      context_window_tokens: 202_752,
      enabled: true,
      created_at: 1,
    } as SubAgentDefinition];

    const profile = resolveThreadOwnerRuntimeProfile(
      thread({ agent_name: "Mokosh", targetAgentId: "mokosh" }),
      subAgents,
      settings,
      {},
    );

    expect(profile.ownerId).toBe("mokosh");
    expect(profile.provider).toBe("z.ai-coding-plan");
    expect(profile.model).toBe("glm-5");
    expect(profile.effort).toBe("medium");
    expect(profile.contextWindowTokens).toBe(202_752);
  });

  it("keeps an explicit thread profile overlay after the owner is selected", () => {
    const profile = resolveThreadOwnerRuntimeProfile(
      thread({
        agent_name: "Mokosh",
        targetAgentId: "mokosh",
        profileProvider: "alibaba-coding-plan",
        profileModel: "qwen3.6-plus",
      }),
      [{
        id: "mokosh",
        name: "Mokosh",
        provider: "z.ai-coding-plan",
        model: "glm-5",
        enabled: true,
        created_at: 1,
      } as SubAgentDefinition],
      DEFAULT_AGENT_SETTINGS,
      {},
    );

    expect(profile.provider).toBe("alibaba-coding-plan");
    expect(profile.model).toBe("qwen3.6-plus");
  });

  it("uses Rarog concierge settings when that agent owns the new thread", () => {
    const profile = resolveThreadOwnerRuntimeProfile(
      thread({ agent_name: "Rarog", targetAgentId: "rarog" }),
      [],
      DEFAULT_AGENT_SETTINGS,
      { provider: "anthropic", model: "claude-opus-4-7", reasoning_effort: "low" },
    );

    expect(profile.ownerId).toBe("rarog");
    expect(profile.provider).toBe("anthropic");
    expect(profile.model).toBe("claude-opus-4-7");
    expect(profile.effort).toBe("low");
  });

  it("prefers an explicit thread selection over the previous turn's upstream runtime", () => {
    const profile = resolveThreadOwnerRuntimeProfile(
      thread({
        agent_name: "muse",
        profileProvider: "opencode-go",
        profileModel: "meta/muse-spark-1.2-contributor-free",
        upstreamProvider: "openrouter",
        upstreamModel: "meta/muse-spark-1.2-contributor",
      }),
      [],
      DEFAULT_AGENT_SETTINGS,
      {},
    );

    expect(profile.provider).toBe("opencode-go");
    expect(profile.model).toBe("meta/muse-spark-1.2-contributor-free");
  });

  it("snapshots the selected agent profile instead of a previously visited thread overlay", () => {
    const settings = {
      ...DEFAULT_AGENT_SETTINGS,
      active_provider: "openai" as const,
      openai: { ...DEFAULT_AGENT_SETTINGS.openai, model: "gpt-5.5" },
    };
    const subAgents = [{
      id: "mokosh",
      name: "Mokosh",
      provider: "z.ai-coding-plan",
      model: "glm-5",
      reasoning_effort: "medium",
      context_window_tokens: 202_752,
      enabled: true,
      created_at: 1,
    } as SubAgentDefinition];

    const snapshot = snapshotThreadOwnerRuntimeProfile(
      draftThreadForOwnerSnapshot({ agentId: "mokosh", agentName: "Mokosh" }, "Svarog"),
      subAgents,
      settings,
      {},
    );

    expect(snapshot.profileProvider).toBe("z.ai-coding-plan");
    expect(snapshot.profileModel).toBe("glm-5");
    expect(snapshot.profileReasoningEffort).toBe("medium");
    expect(snapshot.profileContextWindowTokens).toBe(202_752);
  });
});
