import type { ApprovalSource, RiskLevel } from "@/lib/agent-mission-store/types";

export type PreviewApprovalKind = "routine" | "high-impact" | "thread";

export type PreviewApproval = {
  id: string;
  paneId: string;
  threadId: string | null;
  source: ApprovalSource;
  command: string;
  reasons: string[];
  riskLevel: RiskLevel;
  blastRadius: string;
};

export function previewApproval(kind: PreviewApprovalKind, threadId: string | null, now = Date.now()): PreviewApproval {
  const base = { id: `developer-preview-${kind}-${now}`, paneId: "developer-preview", source: "developer-preview" as const };
  if (kind === "high-impact") {
    return { ...base, threadId: null, command: "git push --force origin main", reasons: ["Rewrites shared history on the remote", "Preview only: nothing will run"], riskLevel: "critical", blastRadius: "remote repository" };
  }
  return {
    ...base,
    threadId: kind === "thread" ? threadId : null,
    command: "rm -rf ./build && npm run build",
    reasons: ["Deletes a directory", "Preview only: nothing will run"],
    riskLevel: "medium",
    blastRadius: "workspace",
  };
}
