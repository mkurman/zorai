import type { CSSProperties } from "react";
import { getBridge } from "@/lib/bridge";
import { useWorkspaceStore } from "../lib/workspaceStore";
import { useNotificationStore } from "../lib/notificationStore";
import { useAgentMissionStore } from "../lib/agentMissionStore";
import { encodeTextToBase64 } from "./terminal-pane/utils";
import { resolveApprovalAtSource } from "./approvalResolution";
import { NotificationHeader } from "./notification-panel/NotificationHeader";
import { NotificationList } from "./notification-panel/NotificationList";
import type { TerminalNotification } from "../lib/types";
import { usePluginStore } from "../lib/pluginStore";

/**
 * Slide-over notification panel (Ctrl+I).
 * Shows notification history with mark-read and clear actions.
 */
type NotificationPanelProps = {
  style?: CSSProperties;
  className?: string;
};

export function NotificationPanel({ style, className }: NotificationPanelProps = {}) {
  const open = useWorkspaceStore((s) => s.notificationPanelOpen);
  const toggle = useWorkspaceStore((s) => s.toggleNotificationPanel);
  const notifications = useNotificationStore((s) => s.notifications);
  const markRead = useNotificationStore((s) => s.markRead);
  const markAllRead = useNotificationStore((s) => s.markAllRead);
  const archiveRead = useNotificationStore((s) => s.archiveRead);
  const archiveNotification = useNotificationStore((s) => s.archiveNotification);
  const removeNotification = useNotificationStore((s) => s.removeNotification);
  const clearPaneNotifications = useNotificationStore((s) => s.clearPaneNotifications);
  const setActiveWorkspace = useWorkspaceStore((s) => s.setActiveWorkspace);
  const setActiveSurface = useWorkspaceStore((s) => s.setActiveSurface);
  const setActivePaneId = useWorkspaceStore((s) => s.setActivePaneId);
  const focusCanvasPanel = useWorkspaceStore((s) => s.focusCanvasPanel);
  const clearCanvasPanelStatus = useWorkspaceStore((s) => s.clearCanvasPanelStatus);
  const toggleSettings = useWorkspaceStore((s) => s.toggleSettings);
  const approvals = useAgentMissionStore((s) => s.approvals);
  const resolveApproval = useAgentMissionStore((s) => s.resolveApproval);
  const selectPlugin = usePluginStore((s) => s.selectPlugin);

  const handleSelectNotification = (notification: TerminalNotification) => {
    if (notification.workspaceId) {
      setActiveWorkspace(notification.workspaceId);
    }
    if (notification.surfaceId) {
      setActiveSurface(notification.surfaceId);
    }

    const panelId = notification.panelId ?? notification.paneId;
    if (panelId) {
      focusCanvasPanel(panelId, { storePreviousView: true });
      setActivePaneId(panelId);
    }
  };

  const reactToApprovalNotification = async (notification: TerminalNotification, decision: "approve" | "deny") => {
    handleSelectNotification(notification);

    const paneId = notification.panelId ?? notification.paneId;
    if (!paneId) return;

    const zorai = getBridge();
    const pendingApproval = approvals.find(
      (entry) => entry.paneId === paneId && entry.status === "pending" && entry.handledAt === null
    );

    let resolvedOrAnswered = false;
    if (pendingApproval) {
      const status = decision === "approve" ? "approved-once" : "denied";
      resolvedOrAnswered = await resolveApprovalAtSource(zorai, pendingApproval, status);
      if (resolvedOrAnswered) {
        resolveApproval(pendingApproval.id, status);
      }
    } else if (zorai?.sendTerminalInput) {
      const response = decision === "approve" ? "y\r" : "n\r";
      resolvedOrAnswered = Boolean(await zorai.sendTerminalInput(paneId, encodeTextToBase64(response)));
    }

    if (resolvedOrAnswered) {
      clearPaneNotifications(paneId, "approval");
      clearCanvasPanelStatus(paneId);
    }
  };

  const handleNotificationAction = async (notification: TerminalNotification, actionId: string) => {
    if (actionId === "request_concierge_welcome") {
      const bridge = getBridge();
      void bridge?.agentRequestConciergeWelcome?.();
      markRead(notification.id);
      return;
    }
    const action = notification.actions?.find((entry) => entry.id === actionId);
    if (action?.actionType === "open_plugin_settings" && action.target) {
      if (!useWorkspaceStore.getState().settingsOpen) {
        toggleSettings();
      }
      window.dispatchEvent(new CustomEvent("zorai-open-settings-tab", { detail: { tab: "plugins" } }));
      window.dispatchEvent(new CustomEvent("zorai-open-settings-tab", { detail: { tab: "plugins" } }));
      await selectPlugin(action.target);
      markRead(notification.id);
      return;
    }
    markRead(notification.id);
  };

  if (!open) return null;

  const activeNotifications = notifications.filter((notification) => notification.archivedAt == null && notification.deletedAt == null);
  const unread = activeNotifications.filter((n) => !n.isRead);

  return (
    <div
      onClick={toggle}
      style={{
        position: "fixed",
        inset: 0,
        background: "color-mix(in srgb, var(--shade) 56%, transparent)",
        zIndex: 900,
        display: "flex",
        justifyContent: "flex-end",
        backdropFilter: "none",
        ...(style ?? {}),
      }}
      className={className}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        style={{
          width: 440,
          maxWidth: "90vw",
          height: "100%",
          background: "var(--zorai-bg)",
          borderLeft: "1px solid var(--glass-border)",
          display: "flex",
          flexDirection: "column",
          boxShadow: "none",
        }}
      >
        <NotificationHeader
          unreadCount={unread.length}
          totalCount={activeNotifications.length}
          markAllRead={markAllRead}
          archiveRead={archiveRead}
          close={toggle}
        />

        <NotificationList
          notifications={notifications}
          markRead={markRead}
          archiveNotification={archiveNotification}
          deleteNotification={removeNotification}
          onSelectNotification={handleSelectNotification}
          onApproveNotification={(notification) => {
            void reactToApprovalNotification(notification, "approve");
          }}
          onDenyNotification={(notification) => {
            void reactToApprovalNotification(notification, "deny");
          }}
          onNotificationAction={handleNotificationAction}
        />
      </div>
    </div>
  );
}
