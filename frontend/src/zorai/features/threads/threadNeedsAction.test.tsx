import { describe, expect, it } from "vitest";
import { approvalShownHere, threadApprovalNeedsBanner } from "@/components/AgentApprovalOverlay";
import { threadIdsNeedingAction } from "./threadNeedsAction";

const pending = { status: "pending", handledAt: null, threadId: "thread-a" };

describe("thread approvals stay with their thread", () => {
  it("renders a thread approval inline only in its own thread", () => {
    expect(approvalShownHere(pending, { inline: true, threadId: "thread-a", inlineHosts: 1 })).toBe(true);
    expect(approvalShownHere(pending, { inline: true, threadId: "thread-b", inlineHosts: 1 })).toBe(false);
  });

  it("never covers the whole screen for a thread approval, even when no thread view is open", () => {
    expect(approvalShownHere(pending, { inline: false, threadId: null, inlineHosts: 1 })).toBe(false);
    expect(approvalShownHere(pending, { inline: false, threadId: null, inlineHosts: 0 })).toBe(false);
  });

  it("points to the waiting thread with a banner only when no thread view can show it", () => {
    expect(threadApprovalNeedsBanner(pending, 0)).toBe(true);
    expect(threadApprovalNeedsBanner(pending, 1)).toBe(false);
    expect(threadApprovalNeedsBanner({ ...pending, threadId: null }, 0)).toBe(false);
  });

  it("still shows terminal approvals without a thread in the global modal", () => {
    expect(approvalShownHere({ ...pending, threadId: null }, { inline: false, threadId: null, inlineHosts: 2 })).toBe(true);
  });

  it("hides resolved or handled approvals everywhere", () => {
    expect(approvalShownHere({ ...pending, status: "denied" }, { inline: true, threadId: "thread-a", inlineHosts: 1 })).toBe(false);
    expect(approvalShownHere({ ...pending, handledAt: 1 }, { inline: false, threadId: null, inlineHosts: 0 })).toBe(false);
  });
});

describe("threads that need the operator", () => {
  it("flags threads with a pending approval or an open question, and nothing else", () => {
    const ids = threadIdsNeedingAction(
      [pending, { status: "approved-once", handledAt: 5, threadId: "thread-done" }, { status: "pending", handledAt: null, threadId: null }],
      "thread-q",
    );
    expect([...ids].sort()).toEqual(["thread-a", "thread-q"]);
  });
});
