import { describe, expect, it } from "vitest";
import type { AgentMessage } from "@/lib/agentStore";
import { messagesFromRewindPoint, rewindIsUndoable, type ThreadRewind } from "./threadRewind";

const message = (id: string): AgentMessage => ({
  id,
  threadId: "t1",
  createdAt: 1,
  role: "user",
  content: id,
  inputTokens: 0,
  outputTokens: 0,
  totalTokens: 0,
  isCompactionSummary: false,
});

describe("thread rewind", () => {
  it("takes the chosen message and everything after it, leaving earlier turns alone", () => {
    const messages = ["a", "b", "c", "d"].map(message);
    expect(messagesFromRewindPoint(messages, "b").map((entry) => entry.id)).toEqual(["b", "c", "d"]);
    expect(messagesFromRewindPoint(messages, "missing")).toEqual([]);
  });

  it("offers undo only while the thread still ends where the rewind left it, so restored messages never interleave with newer ones", () => {
    const rewind: ThreadRewind = { threadId: "t1", anchorId: "a", messages: [message("b")], draftBefore: "" };
    expect(rewindIsUndoable(rewind, "t1", [message("a")])).toBe(true);
    expect(rewindIsUndoable(rewind, "t1", [message("a"), message("new")])).toBe(false);
    expect(rewindIsUndoable(rewind, "other", [message("a")])).toBe(false);
    expect(rewindIsUndoable({ ...rewind, anchorId: null }, "t1", [])).toBe(true);
  });
});
