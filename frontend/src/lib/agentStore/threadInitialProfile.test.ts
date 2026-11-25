import { describe, expect, it } from "vitest";
import type { AgentThread } from "./types";
import { setPendingThreadGenerationSettings, takeInitialThreadExecutionProfile } from "./threadProfileMerge";

const thread = (id: string, extra: Partial<AgentThread> = {}) => ({ id, ...extra }) as AgentThread;

describe("initial thread execution profile", () => {
  it("carries generation settings chosen before the first message, so the first request already samples with them", () => {
    setPendingThreadGenerationSettings("t1", { temperature: 0.2 } as never);
    const profile = takeInitialThreadExecutionProfile(thread("t1", { profileProvider: "local-vllm", profileModel: "m" }));
    expect(profile).toMatchObject({ provider: "local-vllm", model: "m", generation_settings: { temperature: 0.2 } });
  });

  it("is applied once, so the later thread-created sync cannot overwrite it without the generation settings", () => {
    setPendingThreadGenerationSettings("t2", { temperature: 0.5 } as never);
    expect(takeInitialThreadExecutionProfile(thread("t2"))).not.toBeNull();
    expect(takeInitialThreadExecutionProfile(thread("t2", { profileModel: "m" }))).toBeNull();
  });

  it("sends nothing for a thread with no overrides, so the agent defaults stay in charge", () => {
    expect(takeInitialThreadExecutionProfile(thread("t3"))).toBeNull();
  });
});
