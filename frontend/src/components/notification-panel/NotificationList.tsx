import { useMemo, useState } from "react";
import type { TerminalNotification } from "../../lib/types";
import { formatTime } from "./shared";

export function NotificationList({
  notifications,
  markRead,
  archiveNotification,
  deleteNotification,
  onSelectNotification,
  onApproveNotification,
  onDenyNotification,
  onNotificationAction,
}: {
  notifications: TerminalNotification[];
  markRead: (id: string) => void;
  archiveNotification: (id: string) => void;
  deleteNotification: (id: string) => void;
  onSelectNotification?: (notification: TerminalNotification) => void;
  onApproveNotification?: (notification: TerminalNotification) => void;
  onDenyNotification?: (notification: TerminalNotification) => void;
  onNotificationAction?: (notification: TerminalNotification, actionId: string) => void;
}) {
  const [expandedIds, setExpandedIds] = useState<Record<string, boolean>>({});
  const activeNotifications = useMemo(
    () => notifications.filter((notification) => notification.archivedAt == null && notification.deletedAt == null),
    [notifications],
  );

  const toggleExpanded = (notification: TerminalNotification) => {
    setExpandedIds((state) => ({ ...state, [notification.id]: !state[notification.id] }));
    if (!notification.isRead) {
      markRead(notification.id);
    }
  };

  return (
    <div style={{ flex: 1, overflow: "auto", padding: "4px 0 14px" }}>
      {activeNotifications.length === 0 ? (
        <div
          style={{
            padding: 32,
            textAlign: "center",
            color: "var(--text-secondary)",
            fontSize: "var(--text-sm)",
          }}
        >
          No notifications
        </div>
      ) : (
        activeNotifications.map((notification) => {
          const expanded = !!expandedIds[notification.id];
          const accentColor = notification.severity === "error"
            ? "var(--danger)"
            : notification.severity === "warning" || notification.source === "approval"
              ? "var(--warning)"
              : "var(--accent)";
          return (
            <div
              key={notification.id}
              style={{
                padding: "12px 16px",
                borderBottom: "1px solid color-mix(in srgb, var(--text-primary) 3%, transparent)",
                margin: "0 10px 8px",
                borderRadius: 0,
                border: `1px solid ${notification.source === "approval" ? "var(--approval-border)" : "color-mix(in srgb, var(--text-primary) 6%, transparent)"}`,
                background: notification.source === "approval" ? "var(--approval-soft)" : "color-mix(in srgb, var(--text-primary) 2%, transparent)",
                opacity: notification.isRead ? 0.82 : 1,
              }}
            >
              <div
                onClick={() => toggleExpanded(notification)}
                style={{ display: "grid", gap: 6, cursor: "pointer" }}
              >
                <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                  {!notification.isRead ? (
                    <div
                      style={{
                        width: 7,
                        height: 7,
                        borderRadius: "50%",
                        background: accentColor,
                        flexShrink: 0,
                      }}
                    />
                  ) : null}
                  <span style={{ fontSize: "var(--text-sm)", fontWeight: notification.isRead ? 500 : 700 }}>
                    {notification.title}
                  </span>
                  <span style={{ marginLeft: "auto", fontSize: "var(--text-sm)", color: "var(--text-secondary)" }}>
                    {formatTime(notification.updatedAt ?? notification.timestamp)}
                  </span>
                </div>
                <div style={{ display: "flex", alignItems: "center", gap: 8, fontSize: "var(--text-sm)", color: "var(--text-secondary)", textTransform: "uppercase", letterSpacing: "0.06em" }}>
                  <span>{notification.source}</span>
                  {notification.subtitle ? <span>{notification.subtitle}</span> : null}
                  {notification.severity ? <span>{notification.severity}</span> : null}
                </div>
                <div
                  style={{
                    fontSize: "var(--text-sm)",
                    color: "var(--text-secondary)",
                    whiteSpace: "pre-wrap",
                    wordBreak: "break-word",
                    lineHeight: 1.5,
                  }}
                >
                  {expanded ? notification.body : truncate(notification.body, 160)}
                </div>
              </div>

              <div style={{ marginTop: 12, display: "flex", flexWrap: "wrap", gap: 8 }}>
                <ActionButton
                  label={expanded ? "Collapse" : "Expand"}
                  onClick={() => toggleExpanded(notification)}
                  tone="info"
                />
                {!notification.isRead ? (
                  <ActionButton label="Mark Read" onClick={() => markRead(notification.id)} tone="success" />
                ) : null}
                <ActionButton label="Archive" onClick={() => archiveNotification(notification.id)} tone="neutral" />
                <ActionButton label="Delete" onClick={() => deleteNotification(notification.id)} tone="danger" />
                {notification.source === "approval" ? (
                  <>
                    <ActionButton label="Allow" onClick={() => onApproveNotification?.(notification)} tone="success" />
                    <ActionButton label="Deny" onClick={() => onDenyNotification?.(notification)} tone="danger" />
                  </>
                ) : null}
                {notification.actions?.map((action) => (
                  <ActionButton
                    key={action.id}
                    label={action.label}
                    onClick={() => onNotificationAction?.(notification, action.id)}
                    tone="info"
                  />
                ))}
                <ActionButton
                  label="Focus"
                  onClick={() => {
                    onSelectNotification?.(notification);
                    if (!notification.isRead) {
                      markRead(notification.id);
                    }
                  }}
                  tone="neutral"
                />
              </div>
            </div>
          );
        })
      )}
    </div>
  );
}

function ActionButton({
  label,
  onClick,
  tone,
}: {
  label: string;
  onClick: () => void;
  tone: "info" | "success" | "neutral" | "danger";
}) {
  const palette = tone === "danger"
    ? { border: "color-mix(in srgb, var(--danger) 36%, transparent)", background: "color-mix(in srgb, var(--danger) 14%, transparent)", color: "var(--danger)" }
    : tone === "success"
      ? { border: "color-mix(in srgb, var(--success) 36%, transparent)", background: "color-mix(in srgb, var(--success) 16%, transparent)", color: "var(--success)" }
      : tone === "info"
        ? { border: "color-mix(in srgb, var(--info) 36%, transparent)", background: "color-mix(in srgb, var(--info) 14%, transparent)", color: "var(--accent)" }
        : { border: "color-mix(in srgb, var(--text-primary) 12%, transparent)", background: "color-mix(in srgb, var(--text-primary) 4%, transparent)", color: "var(--text-secondary)" };
  return (
    <button
      type="button"
      onClick={onClick}
      style={{
        border: `1px solid ${palette.border}`,
        background: palette.background,
        color: palette.color,
        fontSize: "var(--text-sm)",
        fontWeight: 700,
        padding: "5px 10px",
        cursor: "pointer",
      }}
    >
      {label}
    </button>
  );
}

function truncate(value: string, maxLength: number): string {
  if (value.length <= maxLength) return value;
  return `${value.slice(0, maxLength - 1)}…`;
}
