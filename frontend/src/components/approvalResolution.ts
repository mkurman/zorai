import type { ApprovalRequest } from "@/lib/agentMissionStore";

export type ApprovalDecisionStatus = "approved-once" | "approved-session" | "denied";

export type ApprovalBridge = {
  resolveManagedApproval?: (paneId: string, approvalId: string, decision: string) => Promise<unknown>;
  agentResolveTaskApproval?: (approvalId: string, decision: string) => Promise<unknown>;
};

function daemonDecision(status: ApprovalDecisionStatus): string {
  if (status === "approved-session") return "approve-session";
  if (status === "denied") return "deny";
  return "approve-once";
}

function bridgeResultSucceeded(result: unknown): boolean {
  return !(result && typeof result === "object" && "ok" in result && (result as { ok?: unknown }).ok === false);
}

export async function resolveApprovalAtSource(
  bridge: ApprovalBridge | null | undefined,
  approval: ApprovalRequest,
  status: ApprovalDecisionStatus,
): Promise<boolean> {
  const decision = daemonDecision(status);

  if (approval.source === "local-terminal" || approval.source === "developer-preview") {
    return true;
  }
  if (approval.source === "terminal-bridge") {
    if (!bridge?.resolveManagedApproval) return false;
    return bridgeResultSucceeded(await bridge.resolveManagedApproval(approval.paneId, approval.id, decision));
  }
  if (approval.source === "agent-task") {
    if (!bridge?.agentResolveTaskApproval) return false;
    return bridgeResultSucceeded(await bridge.agentResolveTaskApproval(approval.id, decision));
  }

  // Legacy persisted approvals predate explicit source metadata. Prefer the
  // daemon-wide task route so a stale/missing pane bridge cannot trap the UI.
  if (bridge?.agentResolveTaskApproval) {
    return bridgeResultSucceeded(await bridge.agentResolveTaskApproval(approval.id, decision));
  }
  if (bridge?.resolveManagedApproval) {
    return bridgeResultSucceeded(await bridge.resolveManagedApproval(approval.paneId, approval.id, decision));
  }
  return false;
}

export function shouldBypassApprovalPrompt(
  approval: ApprovalRequest,
  terminalSecurityLevel: string,
  managedSecurityLevel: string,
): boolean {
  if (approval.source === "developer-preview") return false;
  if (approval.source === "agent-task") return managedSecurityLevel === "yolo";
  if (approval.source === "terminal-bridge" || approval.source === "local-terminal") {
    return terminalSecurityLevel === "yolo";
  }
  return terminalSecurityLevel === "yolo" || managedSecurityLevel === "yolo";
}
