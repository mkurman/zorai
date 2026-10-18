import { beforeEach, describe, expect, it, vi } from "vitest";
import { useAgentStore, type AgentThread } from "@/lib/agentStore";
import { clearThreadRuntimeOverlay } from "@/lib/agentStore/threadProfileMerge";
import { applyThreadProviderModel } from "./threadRuntimeActions";

const agentGetThreadExecutionProfile = vi.fn();
const agentSetThreadExecutionProfile = vi.fn();
const agentSetProviderModel = vi.fn();
const agentSetTargetAgentProviderModel = vi.fn();

vi.mock("@/lib/bridge", () => ({
  getBridge: () => ({
    agentGetThreadExecutionProfile,
    agentSetThreadExecutionProfile,
    agentSetProviderModel,
    agentSetTargetAgentProviderModel,
  }),
}));

function thread(id: string, daemonThreadId: string): AgentThread {
  return {
    id,
    daemonThreadId,
    workspaceId: null,
    surfaceId: null,
    paneId: null,
    agent_name: "Svarog",
    title: id,
    createdAt: 1,
    updatedAt: 1,
    messageCount: 0,
    totalInputTokens: 0,
    totalOutputTokens: 0,
    totalTokens: 0,
    compactionCount: 0,
    lastMessagePreview: "",
    upstreamThreadId: null,
    upstreamTransport: undefined,
    upstreamProvider: null,
    upstreamModel: null,
    upstreamAssistantId: null,
  };
}

describe("thread runtime actions", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    clearThreadRuntimeOverlay("thread-a");
    clearThreadRuntimeOverlay("thread-b");
    agentGetThreadExecutionProfile.mockResolvedValue({ profile: { reasoning_effort: "high" } });
    agentSetThreadExecutionProfile.mockResolvedValue({});
    useAgentStore.setState({
      threads: [thread("thread-a", "daemon-a"), thread("thread-b", "daemon-b")],
      activeThreadId: "thread-a",
    } as any);
  });

  it("changes provider and model only on the selected thread profile", async () => {
    const beforeSettings = useAgentStore.getState().agentSettings;
    const selected = useAgentStore.getState().threads[0];

    await applyThreadProviderModel(selected, "openrouter", "model-thread-a");

    expect(agentSetThreadExecutionProfile).toHaveBeenCalledWith("daemon-a", {
      reasoning_effort: "high",
      provider: "openrouter",
      model: "model-thread-a",
    });
    expect(agentSetProviderModel).not.toHaveBeenCalled();
    expect(agentSetTargetAgentProviderModel).not.toHaveBeenCalled();
    expect(useAgentStore.getState().agentSettings).toBe(beforeSettings);
    expect(useAgentStore.getState().threads[0]).toMatchObject({
      profileProvider: "openrouter",
      profileModel: "model-thread-a",
    });
    expect(useAgentStore.getState().threads[1]).not.toHaveProperty("profileProvider");
    expect(useAgentStore.getState().threads[1]).not.toHaveProperty("profileModel");
  });

  it("replaces the previous model's context window with the fetched model's window", async () => {
    agentGetThreadExecutionProfile.mockResolvedValue({
      profile: { provider: "openai", model: "gpt-5.5", context_window_tokens: 128_000 },
    });
    const selected = useAgentStore.getState().threads[0];

    await applyThreadProviderModel(selected, "openrouter", "deepseek/deepseek-v4.1-flash", 1_048_576);

    expect(agentSetThreadExecutionProfile).toHaveBeenCalledWith("daemon-a", {
      provider: "openrouter",
      model: "deepseek/deepseek-v4.1-flash",
      context_window_tokens: 1_048_576,
    });
    expect(useAgentStore.getState().threads[0]).toMatchObject({ profileContextWindowTokens: 1_048_576 });
  });

  it("updates the local thread profile before the daemon round-trip finishes", async () => {
    let finishGet: (value: unknown) => void = () => {};
    agentGetThreadExecutionProfile.mockImplementation(
      () => new Promise((resolve) => {
        finishGet = resolve;
      }),
    );
    const selected = useAgentStore.getState().threads[0];
    const pending = applyThreadProviderModel(selected, "z.ai-coding-plan", "glm-5.3");

    expect(useAgentStore.getState().threads[0]).toMatchObject({
      profileProvider: "z.ai-coding-plan",
      profileModel: "glm-5.3",
    });
    expect(agentSetThreadExecutionProfile).not.toHaveBeenCalled();

    finishGet({ profile: { reasoning_effort: "high" } });
    await pending;
    expect(agentSetThreadExecutionProfile).toHaveBeenCalled();
  });

  it("restores the overlay when the daemon rejects a provider change", async () => {
    agentSetThreadExecutionProfile.mockResolvedValue({ error: "daemon rejected profile" });
    const selected = useAgentStore.getState().threads[0];
    await expect(applyThreadProviderModel(selected, "openrouter", "model-thread-a")).rejects.toThrow(
      "daemon rejected profile",
    );
    expect(useAgentStore.getState().threads[0].profileProvider).toBeNull();
  });

  it("keeps the local profile when the thread is not linked to the daemon yet", async () => {
    useAgentStore.setState({
      threads: [thread("thread-a", "")],
      activeThreadId: "thread-a",
    } as any);
    const selected = useAgentStore.getState().threads[0];
    await applyThreadProviderModel(selected, "openrouter", "model-thread-a");
    expect(agentSetThreadExecutionProfile).not.toHaveBeenCalled();
    expect(useAgentStore.getState().threads[0]).toMatchObject({
      profileProvider: "openrouter",
      profileModel: "model-thread-a",
    });
  });
});
