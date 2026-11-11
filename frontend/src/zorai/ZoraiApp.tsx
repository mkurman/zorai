import { useEffect, useRef } from "react";
import { AgentApprovalOverlay } from "@/components/AgentApprovalOverlay";
import { AgentChatPanelProvider } from "@/components/agent-chat-panel/runtime";
import { ConciergeToast } from "@/components/ConciergeToast";
import { OperatorProfileOnboardingPanel } from "@/components/OperatorProfileOnboardingPanel";
import { OperatorQuestionOverlay } from "@/components/OperatorQuestionOverlay";
import { CommandPalette } from "@/components/CommandPalette";
import { SetupOnboardingPanel } from "@/components/SetupOnboardingPanel";
import { TimeTravelSlider } from "@/components/TimeTravelSlider";
import { ToastViewport } from "@/components/ToastViewport";
import { UpdatePrompt } from "@/components/UpdatePrompt";
import { getBridge } from "@/lib/bridge";
import { useAgentStore } from "@/lib/agentStore";
import { useAuditStore } from "@/lib/auditStore";
import { useNotificationStore } from "@/lib/notificationStore";
import { useWorkspaceStore } from "@/lib/workspaceStore";
import { useWorkspaceContextStore } from "@/lib/workspaceContextStore";
import { shouldAutoStartOperatorProfileFromConcierge } from "./conciergeEvents";
import { useZoraiAppCommands } from "./shell/zoraiAppCommands";
import { ZoraiShell } from "./shell/ZoraiShell";
import { ZoraiTitleBar } from "./shell/ZoraiTitleBar";
import { ZoraiContextMenu } from "./shell/ZoraiContextMenu";
import { ThreadBrowserHost } from "./features/browser/ThreadBrowserHost";

export function ZoraiApp() {
  const operatorProfileAutoStartRequested = useRef(false);
  useZoraiAppCommands();

  useEffect(() => {
    void useWorkspaceContextStore.getState().hydrate();
    useWorkspaceStore.setState({ agentPanelOpen: true });
  }, []);

  useEffect(() => {
    const bridge = getBridge();
    void useNotificationStore.getState().loadSharedNotifications();
    if (!bridge?.onAgentEvent) return;

    const unsubscribe = bridge.onAgentEvent((event: any) => {
      if (event?.type === "concierge_welcome") {
        useAgentStore.setState({
          conciergeWelcome: {
            content: event.content ?? "",
            actions: event.actions ?? [],
          },
        });
        if (
          !operatorProfileAutoStartRequested.current
          && shouldAutoStartOperatorProfileFromConcierge(event)
        ) {
          operatorProfileAutoStartRequested.current = true;
          void useAgentStore.getState().maybeStartOperatorProfileOnboarding();
        }
      }
      if (event?.type === "operator-profile-session-started") {
        useAgentStore.getState().applyOperatorProfileSessionStarted(event.data ?? event);
      }
      if (event?.type === "operator-profile-question") {
        useAgentStore.getState().applyOperatorProfileQuestion(event.data ?? event);
      }
      if (event?.type === "operator-profile-progress") {
        useAgentStore.getState().applyOperatorProfileProgress(event.data ?? event);
      }
      if (event?.type === "operator-profile-session-completed") {
        useAgentStore.getState().applyOperatorProfileSessionCompleted(event.data ?? event);
      }
      if (event?.type === "audit_action") {
        useAuditStore.getState().addEntry({
          id: event.id ?? "",
          timestamp: event.timestamp ?? Date.now(),
          actionType: event.action_type ?? "heartbeat",
          summary: event.summary ?? "",
          explanation: event.explanation ?? null,
          confidence: event.confidence ?? null,
          confidenceBand: event.confidence_band ?? null,
          causalTraceId: event.causal_trace_id ?? null,
          threadId: event.thread_id ?? null,
        });
      }
      if (event?.type === "notification_inbox_upsert" && event.notification) {
        useNotificationStore.getState().upsertSharedNotification(event.notification);
      }
    });

    const timer = window.setTimeout(() => {
      void bridge.agentRequestConciergeWelcome?.().catch(() => {});
    }, 250);

    return () => {
      window.clearTimeout(timer);
      if (typeof unsubscribe === "function") unsubscribe();
    };
  }, []);

  return (
    <AgentChatPanelProvider>
      <div className="zorai-window">
        <ZoraiTitleBar />
        <ZoraiShell />
      </div>
      <ZoraiContextMenu />
      <CommandPalette />
      <TimeTravelSlider />
      <SetupOnboardingPanel />
      <OperatorProfileOnboardingPanel />
      <AgentApprovalOverlay />
      <OperatorQuestionOverlay />
      <ConciergeToast />
      <ToastViewport />
      <UpdatePrompt />
      <ThreadBrowserHost />
    </AgentChatPanelProvider>
  );
}
