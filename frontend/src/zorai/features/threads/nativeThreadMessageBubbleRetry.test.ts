import { describe, expect, it } from "vitest";
import type { AgentMessage } from "@/lib/agentStore";
import {
  isRetryableErrorMessage,
  shouldOfferMessageRetry,
} from "./NativeThreadMessageBubble";

function message(overrides: Partial<AgentMessage> = {}): AgentMessage {
  return {
    id: "asst-1",
    threadId: "thread-1",
    createdAt: 1_700_000_000_000,
    role: "assistant",
    content: "done",
    inputTokens: 0,
    outputTokens: 0,
    totalTokens: 0,
    isCompactionSummary: false,
    ...overrides,
  };
}

describe("isRetryableErrorMessage", () => {
  it("treats daemon provider failures as retryable", () => {
    // Why: request failures are written as `Error: ...` by the event/runtime path.
    expect(isRetryableErrorMessage(message({
      content: "Error: 429 rate limit exceeded",
    }))).toBe(true);
  });

  it("does not treat a successful reply that mentions quota as a rate-limit failure", () => {
    // Why: the retry prompt used to match /quota/ anywhere, so a later answer
    // discussing quota kept a "Provider rate limit" box after Yes, retry.
    expect(isRetryableErrorMessage(message({
      content: "The quota concern is about call-site wiring, not a provider 429.",
    }))).toBe(false);
  });
});

describe("shouldOfferMessageRetry", () => {
  const mountedAt = 1_700_000_000_000;

  it("keeps stop and retry on the latest provider failure after the view was opened", () => {
    const failed = message({
      id: "failed",
      createdAt: mountedAt - 60_000,
      content: "Error: openai API returned 429: The usage limit has been reached",
    });
    expect(shouldOfferMessageRetry(failed, "failed", mountedAt, true)).toBe(true);
  });

  it("hides the prompt once a later assistant message exists", () => {
    // Why: retrying leaves the original Error: bubble in history. The prompt
    // is only meaningful on the current failed turn.
    const failed = message({ id: "failed", content: "Error: provider rate limit" });
    expect(shouldOfferMessageRetry(failed, "later-ok", mountedAt, true)).toBe(false);
    expect(shouldOfferMessageRetry(failed, "failed", mountedAt, true)).toBe(true);
  });
});
