import { describe, expect, it } from "vitest";
import type { AgentMessage } from "./types";
import {
  applyRendererMessageBudget,
  boundRendererText,
  MAX_RENDERER_STORE_CHARS,
  MAX_RENDERER_TEXT_CHARS,
  MAX_RENDERER_THREAD_CHARS,
} from "./rendererText";

function message(id: string, content: string): AgentMessage {
  return {
    id,
    threadId: "thread",
    createdAt: 1,
    role: "assistant",
    content,
    inputTokens: 0,
    outputTokens: 0,
    totalTokens: 0,
    isCompactionSummary: false,
  };
}

describe("boundRendererText", () => {
  it("keeps a normal tool result intact", () => {
    expect(boundRendererText("exit 0")).toBe("exit 0");
  });

  it("drops the tail of a multi-megabyte tool payload before it enters the renderer heap", () => {
    const payload = "x".repeat(MAX_RENDERER_TEXT_CHARS + 50_000);
    const bounded = boundRendererText(payload);
    expect(bounded.length).toBeLessThan(payload.length);
    expect(bounded.startsWith("x".repeat(MAX_RENDERER_TEXT_CHARS))).toBe(true);
    expect(bounded.endsWith("memory limit]")).toBe(true);
  });

  it("drops the oldest messages once a thread exceeds the renderer budget", () => {
    const oversized = "y".repeat(MAX_RENDERER_THREAD_CHARS);
    const kept = applyRendererMessageBudget({
      active: [message("old", oversized), message("new", "latest")],
    }, "active");
    expect(kept.active.map((entry) => entry.id)).toEqual(["new"]);
  });

  it("releases inactive thread transcripts before the active thread when the store budget is exceeded", () => {
    const bulky = "z".repeat(MAX_RENDERER_STORE_CHARS);
    const kept = applyRendererMessageBudget({
      active: [message("visible", "hello")],
      background: [message("hidden", bulky)],
      other: [message("other", bulky)],
    }, "active");
    expect(kept.active.map((entry) => entry.id)).toEqual(["visible"]);
    expect(kept.background).toEqual([]);
    expect(kept.other).toEqual([]);
  });
});
