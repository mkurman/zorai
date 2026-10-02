import { describe, expect, it } from "vitest";
import { shouldShowConversationSkeleton } from "./threadLoadingStore";

describe("conversation skeleton visibility", () => {
  it("covers an in-flight open before the thread exists locally", () => {
    expect(shouldShowConversationSkeleton({
      pending: 1,
      hasActiveThread: false,
      loadedMessageCount: 0,
      knownHistory: false,
    })).toBe(true);
  });

  it("covers known history that has not been loaded yet", () => {
    expect(shouldShowConversationSkeleton({
      pending: 0,
      hasActiveThread: true,
      loadedMessageCount: 0,
      knownHistory: true,
    })).toBe(true);
  });

  it("does not hide already loaded messages behind a later in-flight fetch", () => {
    expect(shouldShowConversationSkeleton({
      pending: 1,
      hasActiveThread: true,
      loadedMessageCount: 4,
      knownHistory: true,
    })).toBe(false);
  });

  it("shows a loader while the opened thread's messages are still in flight", () => {
    expect(shouldShowConversationSkeleton({
      pending: 0,
      activeThreadLoading: true,
      hasActiveThread: true,
      loadedMessageCount: 0,
      knownHistory: false,
    })).toBe(true);
  });

  it("does not treat a brand-new empty thread as loading", () => {
    expect(shouldShowConversationSkeleton({
      pending: 0,
      hasActiveThread: true,
      loadedMessageCount: 0,
      knownHistory: false,
    })).toBe(false);
  });
});
