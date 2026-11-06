import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { AgentMessage } from "@/lib/agentStore";
import { NativeThreadMessageBubble } from "./NativeThreadMessageBubble";

function userMessage(overrides: Partial<AgentMessage> = {}): AgentMessage {
  return {
    id: "user-1",
    threadId: "thread-1",
    createdAt: 1_700_000_000_000,
    role: "user",
    content: "",
    inputTokens: 0,
    outputTokens: 0,
    totalTokens: 0,
    isStreaming: false,
    ...overrides,
  };
}

function render(message: AgentMessage): string {
  return renderToStaticMarkup(
    <NativeThreadMessageBubble
      message={message}
      onPin={async () => {}}
      onUnpin={async () => {}}
      ttsEnabled={false}
      speaking={false}
      speechLoading={false}
      speechQueued={false}
      onSpeak={() => {}}
    />,
  );
}

const imageBlock = { type: "image", mime_type: "image/png", data_url: "data:image/png;base64,AAAA" } as NonNullable<AgentMessage["contentBlocks"]>[number];

describe("NativeThreadMessageBubble attachments", () => {
  it("shows only the attachment tiles, without an empty text bubble, when a message is attachments only", () => {
    const html = render(userMessage({ contentBlocks: [imageBlock] }));

    expect(html).toContain("zorai-attachment-tiles");
    expect(html).not.toContain("zorai-message__content");
  });

  it("still renders the text bubble when the attachments come with a message", () => {
    const html = render(userMessage({ content: "look at this", contentBlocks: [imageBlock] }));

    expect(html).toContain("zorai-attachment-tiles");
    expect(html).toContain("zorai-message__content");
    expect(html).toContain("look at this");
  });

  it("never shows an empty bubble for a user message that has no text", () => {
    expect(render(userMessage({ content: "   " }))).not.toContain("zorai-message__content");
  });
});
