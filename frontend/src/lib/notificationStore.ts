import { create } from "zustand";
import { getBridge } from "./bridge";
import {
  TerminalNotification,
  NotificationAction,
  NotificationId,
  NotificationSource,
  WorkspaceId,
  SurfaceId,
  PaneId,
} from "./types";

const MAX_NOTIFICATIONS = 500;
const NOTIFICATION_CATEGORY = "notification";

type AgentEventRow = {
  id: string;
  category: string;
  kind: string;
  pane_id?: string | null;
  workspace_id?: string | null;
  surface_id?: string | null;
  session_id?: string | null;
  payload_json: string;
  timestamp: number;
};

type InboxNotificationActionRow = {
  id: string;
  label: string;
  action_type?: string | null;
  target?: string | null;
  payload_json?: string | null;
};

type InboxNotificationRow = {
  id: string;
  source: string;
  kind: string;
  title: string;
  body: string;
  subtitle?: string | null;
  severity: string;
  created_at: number;
  updated_at: number;
  read_at?: number | null;
  archived_at?: number | null;
  deleted_at?: number | null;
  actions?: InboxNotificationActionRow[];
  metadata_json?: string | null;
};

let _nId = 0;
let sharedNotificationsLoaded = false;

function newNotifId(): NotificationId {
  return `notif_${++_nId}`;
}

function isActive(notification: TerminalNotification): boolean {
  return notification.archivedAt == null && notification.deletedAt == null;
}

function sortNotifications(notifications: TerminalNotification[]): TerminalNotification[] {
  return [...notifications]
    .sort((left, right) => {
      const rightTs = right.updatedAt ?? right.timestamp;
      const leftTs = left.updatedAt ?? left.timestamp;
      return rightTs - leftTs;
    })
    .slice(0, MAX_NOTIFICATIONS);
}

function unreadCount(notifications: TerminalNotification[]): number {
  return notifications.filter((notification) => isActive(notification) && !notification.isRead).length;
}

function parseAgentEventRows(rows: unknown): AgentEventRow[] {
  if (!Array.isArray(rows)) return [];
  return rows.filter((row): row is AgentEventRow => {
    if (!row || typeof row !== "object") return false;
    const candidate = row as Partial<AgentEventRow>;
    return typeof candidate.id === "string"
      && typeof candidate.category === "string"
      && typeof candidate.kind === "string"
      && typeof candidate.payload_json === "string"
      && typeof candidate.timestamp === "number";
  });
}

function parsePersistentNotification(row: AgentEventRow): TerminalNotification | null {
  if (row.category !== NOTIFICATION_CATEGORY) return null;
  try {
    const payload = JSON.parse(row.payload_json) as InboxNotificationRow;
    return {
      id: payload.id,
      workspaceId: row.workspace_id ?? null,
      surfaceId: row.surface_id ?? null,
      paneId: row.pane_id ?? null,
      panelId: row.pane_id ?? null,
      title: payload.title,
      subtitle: payload.subtitle ?? null,
      body: payload.body,
      icon: null,
      progress: null,
      isRead: payload.read_at != null,
      timestamp: payload.updated_at ?? row.timestamp,
      source: (payload.source ?? "system") as NotificationSource,
      actions: (payload.actions ?? []).map((action) => ({
        id: action.id,
        label: action.label,
        actionType: action.action_type ?? null,
        target: action.target ?? null,
        payloadJson: action.payload_json ?? null,
      })),
      severity: payload.severity ?? "info",
      kind: payload.kind ?? row.kind,
      createdAt: payload.created_at ?? row.timestamp,
      updatedAt: payload.updated_at ?? row.timestamp,
      archivedAt: payload.archived_at ?? null,
      deletedAt: payload.deleted_at ?? null,
      metadataJson: payload.metadata_json ?? null,
      persistent: true,
    };
  } catch {
    return null;
  }
}

function serializePersistentNotification(notification: TerminalNotification): AgentEventRow | null {
  if (!notification.persistent) return null;
  const payload: InboxNotificationRow = {
    id: notification.id,
    source: notification.source,
    kind: notification.kind ?? "notification",
    title: notification.title,
    body: notification.body,
    subtitle: notification.subtitle ?? null,
    severity: notification.severity ?? "info",
    created_at: notification.createdAt ?? notification.timestamp,
    updated_at: notification.updatedAt ?? notification.timestamp,
    read_at: notification.isRead ? (notification.updatedAt ?? Date.now()) : null,
    archived_at: notification.archivedAt ?? null,
    deleted_at: notification.deletedAt ?? null,
    actions: (notification.actions ?? []).map((action) => ({
      id: action.id,
      label: action.label,
      action_type: action.actionType ?? null,
      target: action.target ?? null,
      payload_json: action.payloadJson ?? null,
    })),
    metadata_json: notification.metadataJson ?? null,
  };
  return {
    id: notification.id,
    category: NOTIFICATION_CATEGORY,
    kind: payload.kind,
    pane_id: notification.paneId ?? null,
    workspace_id: notification.workspaceId ?? null,
    surface_id: notification.surfaceId ?? null,
    session_id: null,
    payload_json: JSON.stringify(payload),
    timestamp: payload.updated_at,
  };
}

async function persistNotification(notification: TerminalNotification): Promise<void> {
  const row = serializePersistentNotification(notification);
  if (!row) return;
  await getBridge()?.dbUpsertAgentEvent?.(row);
}

function upsertNotificationList(
  notifications: TerminalNotification[],
  next: TerminalNotification,
): TerminalNotification[] {
  const existingIndex = notifications.findIndex((entry) => entry.id === next.id);
  if (existingIndex >= 0) {
    const merged = [...notifications];
    merged[existingIndex] = next;
    return sortNotifications(merged);
  }
  return sortNotifications([next, ...notifications]);
}

export interface NotificationState {
  notifications: TerminalNotification[];
  unreadCount: number;

  addNotification: (opts: {
    title: string;
    body: string;
    subtitle?: string | null;
    icon?: string | null;
    progress?: number | null;
    source: NotificationSource;
    workspaceId?: WorkspaceId | null;
    surfaceId?: SurfaceId | null;
    paneId?: PaneId | null;
    panelId?: PaneId | null;
    actions?: NotificationAction[];
  }) => void;

  loadSharedNotifications: () => Promise<void>;
  upsertSharedNotification: (notification: unknown) => void;
  markRead: (id: NotificationId) => void;
  markUnread: (id: NotificationId) => void;
  archiveNotification: (id: NotificationId) => void;
  removeNotification: (id: NotificationId) => void;
  clearPaneNotifications: (paneId: PaneId, source?: NotificationSource) => void;
  markAllRead: () => void;
  archiveRead: () => void;
  markWorkspaceRead: (workspaceId: WorkspaceId) => void;
  clearAll: () => void;

  getUnreadForWorkspace: (workspaceId: WorkspaceId) => number;
}

export const useNotificationStore = create<NotificationState>((set, get) => ({
  notifications: [],
  unreadCount: 0,

  addNotification: (opts) => {
    const notif: TerminalNotification = {
      id: newNotifId(),
      workspaceId: opts.workspaceId ?? null,
      surfaceId: opts.surfaceId ?? null,
      paneId: opts.paneId ?? null,
      panelId: opts.panelId ?? opts.paneId ?? null,
      title: opts.title,
      subtitle: opts.subtitle ?? null,
      body: opts.body,
      icon: opts.icon ?? null,
      progress: opts.progress ?? null,
      isRead: false,
      timestamp: Date.now(),
      source: opts.source,
      actions: opts.actions ?? [],
      persistent: false,
    };

    set((state) => {
      const notifications = sortNotifications([notif, ...state.notifications]);
      return { notifications, unreadCount: unreadCount(notifications) };
    });
  },

  loadSharedNotifications: async () => {
    if (sharedNotificationsLoaded) return;
    sharedNotificationsLoaded = true;
    try {
      const rows = parseAgentEventRows(
        await getBridge()?.dbListAgentEvents?.({ category: NOTIFICATION_CATEGORY, limit: 500 }),
      );
      const shared = rows
        .map(parsePersistentNotification)
        .filter((notification): notification is TerminalNotification => notification !== null);
      set((state) => {
        const locals = state.notifications.filter((notification) => !notification.persistent);
        const notifications = sortNotifications([...locals, ...shared]);
        return { notifications, unreadCount: unreadCount(notifications) };
      });
    } catch {
      sharedNotificationsLoaded = false;
    }
  },

  upsertSharedNotification: (notification) => {
    if (!notification || typeof notification !== "object") return;
    const payload = notification as Partial<InboxNotificationRow>;
    if (typeof payload.id !== "string" || typeof payload.title !== "string") return;
    const next: TerminalNotification = {
      id: payload.id,
      workspaceId: null,
      surfaceId: null,
      paneId: null,
      panelId: null,
      title: payload.title,
      subtitle: payload.subtitle ?? null,
      body: payload.body ?? "",
      icon: null,
      progress: null,
      isRead: payload.read_at != null,
      timestamp: payload.updated_at ?? Date.now(),
      source: (payload.source ?? "system") as NotificationSource,
      actions: (payload.actions ?? []).map((action) => ({
        id: action.id,
        label: action.label,
        actionType: action.action_type ?? null,
        target: action.target ?? null,
        payloadJson: action.payload_json ?? null,
      })),
      severity: payload.severity ?? "info",
      kind: payload.kind ?? "notification",
      createdAt: payload.created_at ?? Date.now(),
      updatedAt: payload.updated_at ?? Date.now(),
      archivedAt: payload.archived_at ?? null,
      deletedAt: payload.deleted_at ?? null,
      metadataJson: payload.metadata_json ?? null,
      persistent: true,
    };
    set((state) => {
      const notifications = upsertNotificationList(state.notifications, next);
      return { notifications, unreadCount: unreadCount(notifications) };
    });
  },

  markUnread: (id) => {
    const notification = get().notifications.find((entry) => entry.id === id);
    if (!notification || !notification.isRead) return;
    const updated: TerminalNotification = { ...notification, isRead: false, updatedAt: Date.now() };
    set((state) => {
      const notifications = upsertNotificationList(state.notifications, updated);
      return { notifications, unreadCount: unreadCount(notifications) };
    });
    void persistNotification(updated);
  },

  markRead: (id) => {
    const notification = get().notifications.find((entry) => entry.id === id);
    if (!notification || notification.isRead) return;
    const updated: TerminalNotification = {
      ...notification,
      isRead: true,
      updatedAt: Date.now(),
      timestamp: Date.now(),
    };
    set((state) => {
      const notifications = upsertNotificationList(state.notifications, updated);
      return { notifications, unreadCount: unreadCount(notifications) };
    });
    void persistNotification(updated);
  },

  archiveNotification: (id) => {
    const notification = get().notifications.find((entry) => entry.id === id);
    if (!notification) return;
    if (!notification.persistent) {
      get().removeNotification(id);
      return;
    }
    const now = Date.now();
    const updated: TerminalNotification = {
      ...notification,
      isRead: true,
      archivedAt: now,
      updatedAt: now,
      timestamp: now,
    };
    set((state) => {
      const notifications = upsertNotificationList(state.notifications, updated);
      return { notifications, unreadCount: unreadCount(notifications) };
    });
    void persistNotification(updated);
  },

  removeNotification: (id) => {
    const notification = get().notifications.find((entry) => entry.id === id);
    if (!notification) return;
    if (!notification.persistent) {
      set((state) => {
        const notifications = state.notifications.filter((entry) => entry.id !== id);
        return { notifications, unreadCount: unreadCount(notifications) };
      });
      return;
    }
    const now = Date.now();
    const updated: TerminalNotification = {
      ...notification,
      isRead: true,
      deletedAt: now,
      updatedAt: now,
      timestamp: now,
    };
    set((state) => {
      const notifications = upsertNotificationList(state.notifications, updated);
      return { notifications, unreadCount: unreadCount(notifications) };
    });
    void persistNotification(updated);
  },

  clearPaneNotifications: (paneId, source) => {
    set((state) => {
      const notifications = state.notifications.filter((entry) => {
        if (entry.paneId !== paneId && entry.panelId !== paneId) {
          return true;
        }
        if (entry.persistent) {
          return true;
        }
        if (!source) {
          return false;
        }
        return entry.source !== source;
      });
      return { notifications, unreadCount: unreadCount(notifications) };
    });
  },

  markAllRead: () => {
    const ids = get().notifications.filter((entry) => isActive(entry) && !entry.isRead).map((entry) => entry.id);
    ids.forEach((id) => get().markRead(id));
  },

  archiveRead: () => {
    const ids = get().notifications.filter((entry) => isActive(entry) && entry.isRead).map((entry) => entry.id);
    ids.forEach((id) => get().archiveNotification(id));
  },

  markWorkspaceRead: (workspaceId) => {
    const ids = get().notifications
      .filter((entry) => entry.workspaceId === workspaceId && isActive(entry) && !entry.isRead)
      .map((entry) => entry.id);
    ids.forEach((id) => get().markRead(id));
  },

  clearAll: () => {
    const persistentIds = get().notifications.filter((entry) => entry.persistent && isActive(entry)).map((entry) => entry.id);
    const localNotifications = get().notifications.filter((entry) => entry.persistent);
    set({
      notifications: localNotifications,
      unreadCount: unreadCount(localNotifications),
    });
    persistentIds.forEach((id) => get().removeNotification(id));
  },

  getUnreadForWorkspace: (workspaceId) => {
    return get().notifications.filter(
      (notification) => notification.workspaceId === workspaceId && isActive(notification) && !notification.isRead,
    ).length;
  },
}));
