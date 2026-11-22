import { useMemo } from "react";
import { useAgentChatPanelRuntime } from "@/components/agent-chat-panel/runtime/context";
import { shouldBypassApprovalPrompt } from "@/components/approvalResolution";
import { useAgentMissionStore } from "@/lib/agentMissionStore";
import { useAgentStore } from "@/lib/agentStore";
import { useNotificationStore } from "@/lib/notificationStore";
import { useOperatorQuestionStore } from "@/lib/operatorQuestionStore";
import { useSettingsStore } from "@/lib/settingsStore";
import { buildInboxItems, type InboxItem } from "./inboxModel";
import { useInboxStore } from "./inboxStore";

export function useInboxItems(): InboxItem[] {
  const { goalRunsForTrace } = useAgentChatPanelRuntime();
  const approvals = useAgentMissionStore((state) => state.approvals);
  const question = useOperatorQuestionStore((state) => state.question);
  const threads = useAgentStore((state) => state.threads);
  const managedSecurityLevel = useAgentStore((state) => state.agentSettings.managed_security_level);
  const terminalSecurityLevel = useSettingsStore((state) => state.settings.securityLevel);
  const notifications = useNotificationStore((state) => state.notifications);
  const baselineAt = useInboxStore((state) => state.baselineAt);
  const threadReadAt = useInboxStore((state) => state.threadReadAt);
  const threadArchivedAt = useInboxStore((state) => state.threadArchivedAt);

  return useMemo(() => buildInboxItems({
    approvals,
    question,
    goalRuns: goalRunsForTrace,
    threads,
    notifications,
    now: Date.now(),
    baselineAt,
    threadReadAt,
    threadArchivedAt,
    isApprovalHiddenByPolicy: (approval) => shouldBypassApprovalPrompt(approval, terminalSecurityLevel, managedSecurityLevel),
  }), [approvals, baselineAt, goalRunsForTrace, managedSecurityLevel, notifications, question, terminalSecurityLevel, threadArchivedAt, threadReadAt, threads]);
}
