import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { AgentMessage } from "@/lib/agentStore";
import { NativeThreadMessageBubble } from "./NativeThreadMessageBubble";

function compactionMessage(overrides: Partial<AgentMessage> = {}): AgentMessage {
  return {
    id: "compaction-1",
    threadId: "thread-1",
    createdAt: 1_700_000_000_000,
    role: "assistant",
    content:
      "Pre-compaction context: ~843,287 / 1,048,576 tokens (threshold 838,860)\nTrigger: token-threshold\nStrategy: custom model generated",
    compactionPayload: "# Checkpoint\n\n- preserved task state",
    inputTokens: 0,
    outputTokens: 0,
    totalTokens: 0,
    isCompactionSummary: true,
    messageKind: "compaction_artifact",
    isStreaming: false,
    ...overrides,
  };
}

describe("NativeThreadMessageBubble compaction artifacts", () => {
  it("renders the trigger header and expandable checkpoint payload", () => {
    const html = renderToStaticMarkup(
      <NativeThreadMessageBubble
        message={compactionMessage()}
        onPin={async () => {}}
        onUnpin={async () => {}}
        ttsEnabled={false}
        speaking={false}
        speechLoading={false}
        speechQueued={false}
        onSpeak={() => {}}
      />,
    );

    expect(html).toContain("Auto compaction");
    expect(html).toContain("Pre-compaction context: ~843,287 / 1,048,576 tokens");
    expect(html).toContain("Show compaction checkpoint");
    expect(html).toContain("Checkpoint");
    expect(html).toContain("preserved task state");
  });

  it("recognizes legacy compaction rows that only expose the header prefix", () => {
    const html = renderToStaticMarkup(
      <NativeThreadMessageBubble
        message={compactionMessage({
          messageKind: "normal",
          isCompactionSummary: false,
          compactionPayload: undefined,
        })}
        onPin={async () => {}}
        onUnpin={async () => {}}
        ttsEnabled={false}
        speaking={false}
        speechLoading={false}
        speechQueued={false}
        onSpeak={() => {}}
      />,
    );

    expect(html).toContain("Auto compaction");
    expect(html).not.toContain("Show compaction checkpoint");
  });
});
