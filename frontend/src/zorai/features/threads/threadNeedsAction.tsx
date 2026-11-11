import { useMemo } from "react";
import { useAgentMissionStore } from "@/lib/agentMissionStore";
import { useOperatorQuestionStore } from "@/lib/operatorQuestionStore";

export function threadIdsNeedingAction(
  approvals: ReadonlyArray<{ status: string; handledAt: number | null; threadId?: string | null }>,
  questionThreadId: string | null,
): Set<string> {
  const ids = new Set(
    approvals
      .filter((approval) => approval.status === "pending" && approval.handledAt === null && approval.threadId)
      .map((approval) => approval.threadId as string),
  );
  if (questionThreadId) ids.add(questionThreadId);
  return ids;
}

export function useThreadIdsNeedingAction(): ReadonlySet<string> {
  const approvals = useAgentMissionStore((state) => state.approvals);
  const questionThreadId = useOperatorQuestionStore((state) => state.question?.threadId ?? null);
  return useMemo(() => threadIdsNeedingAction(approvals, questionThreadId), [approvals, questionThreadId]);
}

export function ThreadNeedsActionIcon() {
  return (
    <svg className="zorai-thread-needs-action" viewBox="0 0 16 16" aria-hidden="true">
      <path fill="currentColor" d="M7.13 1.5a1 1 0 0 1 1.74 0l6.5 11.5A1 1 0 0 1 14.5 14.5h-13a1 1 0 0 1-.87-1.5zM8 5.25a.75.75 0 0 0-.75.75v3.5a.75.75 0 0 0 1.5 0V6A.75.75 0 0 0 8 5.25M8 11a.9.9 0 1 0 0 1.8A.9.9 0 0 0 8 11" />
    </svg>
  );
}
