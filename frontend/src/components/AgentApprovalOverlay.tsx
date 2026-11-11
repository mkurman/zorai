import { useEffect, useRef, type CSSProperties } from "react";
import { create } from "zustand";
import { getBridge } from "@/lib/bridge";
import { useAgentStore } from "@/lib/agentStore";
import { useSettingsStore } from "@/lib/settingsStore";
import { useAgentMissionStore } from "../lib/agentMissionStore";
import { useAgentChatPanelRuntime } from "@/components/agent-chat-panel/runtime/context";
import { openThreadTarget } from "@/zorai/features/threads/openThreadTarget";
import { ThreadNeedsActionIcon } from "@/zorai/features/threads/threadNeedsAction";
import { navigateZorai } from "@/zorai/shell/zoraiNavigationEvents";
import { resolveApprovalAtSource, shouldBypassApprovalPrompt, type ApprovalDecisionStatus } from "./approvalResolution";

type AgentApprovalOverlayProps = {
  style?: CSSProperties;
  className?: string;
  inline?: boolean;
  threadId?: string | null;
};

export function approvalShownHere(
  entry: { status: string; handledAt: number | null; threadId?: string | null },
  placement: { inline: boolean; threadId: string | null; inlineHosts: number },
): boolean {
  if (entry.status !== "pending" || entry.handledAt !== null) return false;
  if (placement.inline) return Boolean(placement.threadId) && entry.threadId === placement.threadId;
  return !entry.threadId;
}

export function threadApprovalNeedsBanner(
  entry: { status: string; handledAt: number | null; threadId?: string | null },
  inlineHosts: number,
): boolean {
  return entry.status === "pending" && entry.handledAt === null && Boolean(entry.threadId) && inlineHosts === 0;
}

const useApprovalInlineHosts = create<{ hosts: number; register: () => () => void }>((set) => ({
  hosts: 0,
  register: () => {
    set((state) => ({ hosts: state.hosts + 1 }));
    return () => set((state) => ({ hosts: Math.max(0, state.hosts - 1) }));
  },
}));

type ApprovalDecision = ApprovalDecisionStatus;

export function AgentApprovalOverlay({ style, className, inline = false, threadId = null }: AgentApprovalOverlayProps = {}) {
  const inlineHosts = useApprovalInlineHosts((s) => s.hosts);
  const registerInlineHost = useApprovalInlineHosts((s) => s.register);
  useEffect(() => (inline ? registerInlineHost() : undefined), [inline, registerInlineHost]);
  const pendingApproval = useAgentMissionStore((s) =>
    s.approvals.find((entry) => entry.status === "pending" && entry.handledAt === null)
  );
  const approval = useAgentMissionStore((s) =>
    s.approvals.find((entry) => approvalShownHere(entry, { inline, threadId, inlineHosts }))
  );
  const terminalSecurityLevel = useSettingsStore((s) => s.settings.securityLevel);
  const managedSecurityLevel = useAgentStore((s) => s.agentSettings.managed_security_level);
  const resolveApproval = useAgentMissionStore((s) => s.resolveApproval);
  const markApprovalHandled = useAgentMissionStore((s) => s.markApprovalHandled);
  const bypassingApprovalIdsRef = useRef(new Set<string>());
  const bypassApproval = approval ? shouldBypassApprovalPrompt(
    approval,
    terminalSecurityLevel,
    managedSecurityLevel,
  ) : false;
  const bypassPending = !inline && pendingApproval ? shouldBypassApprovalPrompt(
    pendingApproval,
    terminalSecurityLevel,
    managedSecurityLevel,
  ) : false;
  const visibleApproval = approval && !bypassApproval ? approval : null;

  useEffect(() => {
    if (!pendingApproval || !bypassPending || bypassingApprovalIdsRef.current.has(pendingApproval.id)) return;
    bypassingApprovalIdsRef.current.add(pendingApproval.id);
    void resolveApprovalAtSource(getBridge(), pendingApproval, "approved-once")
      .then((resolved) => {
        if (!resolved) return;
        resolveApproval(pendingApproval.id, "approved-once");
        if (pendingApproval.source !== "local-terminal") {
          markApprovalHandled(pendingApproval.id);
        }
      })
      .finally(() => {
        bypassingApprovalIdsRef.current.delete(pendingApproval.id);
      });
  }, [pendingApproval, bypassPending, markApprovalHandled, resolveApproval]);

  async function handleDecision(status: ApprovalDecision) {
    if (!visibleApproval) return;

    const resolved = await resolveApprovalAtSource(getBridge(), visibleApproval, status);
    if (!resolved) return;
    resolveApproval(visibleApproval.id, status);
    if (visibleApproval.source !== "local-terminal") {
      markApprovalHandled(visibleApproval.id);
    }
  }

  if (!visibleApproval) return inline ? null : <ThreadApprovalBanner inlineHosts={inlineHosts} />;

  const overlayClassName = ["zorai-approval-overlay", className ?? ""].filter(Boolean).join(" ");
  const riskClassName = ["zorai-approval-risk", `zorai-approval-risk--${visibleApproval.riskLevel}`].join(" ");
  const highImpact = visibleApproval.riskLevel === "high" || visibleApproval.riskLevel === "critical";
  const titleId = inline ? "zorai-approval-title-inline" : "zorai-approval-title";

  const dialog = (
      <section className={["zorai-approval-dialog", inline ? "zorai-approval-dialog--inline" : ""].filter(Boolean).join(" ")} role="dialog" aria-modal={!inline} aria-labelledby={titleId}>
        <header className="zorai-approval-header">
          <div>
            <div className="zorai-approval-kicker">
              <span />
              Approval required
            </div>
            <h2 id={titleId}>{highImpact ? "High-impact command intercepted" : "Zorai wants to run a command"}</h2>
          </div>
          <div className={riskClassName}>{visibleApproval.riskLevel}</div>
        </header>

        <div className="zorai-approval-body">
          <div className="zorai-approval-section">
            <div className="zorai-approval-label">Command</div>
            <pre className="zorai-approval-command">{visibleApproval.command}</pre>
          </div>

          <div className="zorai-approval-grid">
            <InfoCard label="Blast radius" value={visibleApproval.blastRadius} />
            {inline ? null : <InfoCard label="Scope" value={visibleApproval.sessionId ?? visibleApproval.paneId} mono />}
          </div>

          <div className="zorai-approval-section">
            <div className="zorai-approval-label">Why it's asking</div>
            <div className="zorai-approval-chip-list">
              {visibleApproval.reasons.length === 0 ? <span>No specific factors reported.</span> : visibleApproval.reasons.map((reason) => (
                <span key={reason}>{reason}</span>
              ))}
            </div>
          </div>
        </div>

        <footer className="zorai-approval-actions">
          <button type="button" className="zorai-approval-button zorai-approval-button--deny" onClick={() => void handleDecision("denied")}>
            Deny
          </button>
          <button type="button" className="zorai-approval-button" onClick={() => void handleDecision("approved-once")}>
            Allow once
          </button>
          <button type="button" className="zorai-approval-button zorai-approval-button--primary" onClick={() => void handleDecision("approved-session")}>
            Allow for session
          </button>
        </footer>
      </section>
  );

  if (inline) return dialog;
  return (
    <div style={style} className={overlayClassName} role="presentation">
      {dialog}
    </div>
  );
}

function InfoCard({ label, value, mono = false }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className="zorai-approval-info">
      <div className="zorai-approval-label">{label}</div>
      <strong className={mono ? "zorai-approval-info__mono" : undefined}>{value}</strong>
    </div>
  );
}

function ThreadApprovalBanner({ inlineHosts }: { inlineHosts: number }) {
  const runtime = useAgentChatPanelRuntime();
  const waiting = useAgentMissionStore((s) => s.approvals.find((entry) => threadApprovalNeedsBanner(entry, inlineHosts)));
  const threadTitle = useAgentStore((s) => {
    const threadId = waiting?.threadId;
    if (!threadId) return null;
    return s.threads.find((thread) => thread.daemonThreadId === threadId || thread.id === threadId)?.title ?? null;
  });
  if (!waiting?.threadId) return null;
  const threadId = waiting.threadId;

  return (
    <div className="zorai-approval-banner" role="status">
      <ThreadNeedsActionIcon />
      <span>Zorai is waiting for your approval{threadTitle ? <> in <strong>{threadTitle}</strong></> : null}</span>
      <button type="button" className="zorai-approval-button zorai-approval-button--primary" onClick={() => {
        navigateZorai({ view: "threads" });
        void openThreadTarget(runtime, threadId);
      }}>Review</button>
    </div>
  );
}
