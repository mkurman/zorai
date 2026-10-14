import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { getBridge } from "@/lib/bridge";
import { ZORAI_APP_NAME } from "@/zorai/branding";
import { useAgentMissionStore } from "../lib/agentMissionStore";
import { useAgentStore } from "../lib/agentStore";
import { useKeybindStore } from "../lib/keybindStore";
import { useNotificationStore } from "../lib/notificationStore";
import { useWorkspaceStore } from "../lib/workspaceStore";

type TitleMenuItem = {
  id: string;
  label: string;
  shortcut?: string;
  tone?: "default" | "agent";
  onSelect: () => void;
};

type TitleMenuGroup = {
  id: string;
  label: string;
  items: TitleMenuItem[];
};

export function TitleBar() {
  const workspace = useWorkspaceStore((s) => s.activeWorkspace());
  const surface = useWorkspaceStore((s) => s.activeSurface());
  const createWorkspace = useWorkspaceStore((s) => s.createWorkspace);
  const createSurface = useWorkspaceStore((s) => s.createSurface);
  const splitActive = useWorkspaceStore((s) => s.splitActive);
  const toggleZoom = useWorkspaceStore((s) => s.toggleZoom);
  const toggleSidebar = useWorkspaceStore((s) => s.toggleSidebar);
  const toggleSettings = useWorkspaceStore((s) => s.toggleSettings);
  const settingsOpen = useWorkspaceStore((s) => s.settingsOpen);
  const toggleFileManager = useWorkspaceStore((s) => s.toggleFileManager);
  const toggleSessionVault = useWorkspaceStore((s) => s.toggleSessionVault);
  const toggleCommandPalette = useWorkspaceStore((s) => s.toggleCommandPalette);
  const toggleCommandHistory = useWorkspaceStore((s) => s.toggleCommandHistory);
  const toggleCommandLog = useWorkspaceStore((s) => s.toggleCommandLog);
  const toggleSnippets = useWorkspaceStore((s) => s.toggleSnippetPicker);
  const toggleAgentPanel = useWorkspaceStore((s) => s.toggleAgentPanel);
  const toggleTimeTravel = useWorkspaceStore((s) => s.toggleTimeTravel);
  const toggleSystemMonitor = useWorkspaceStore((s) => s.toggleSystemMonitor);
  const toggleSearch = useWorkspaceStore((s) => s.toggleSearch);
  const toggleNotificationPanel = useWorkspaceStore((s) => s.toggleNotificationPanel);
  const notificationPanelOpen = useWorkspaceStore((s) => s.notificationPanelOpen);
  const approvals = useAgentMissionStore((s) => s.approvals);
  const cognitiveEvents = useAgentMissionStore((s) => s.cognitiveEvents);
  const unreadNotifications = useNotificationStore((s) => s.unreadCount);
  const active_provider = useAgentStore((s) => s.agentSettings.active_provider);
  const bindings = useKeybindStore((s) => s.bindings);
  const [platform, setPlatform] = useState<string | null>(null);
  const [maximized, setMaximized] = useState(false);
  const [openMenuId, setOpenMenuId] = useState<string | null>(null);
  const menuBarRef = useRef<HTMLDivElement | null>(null);
  const approvalCount = useMemo(
    () => approvals.filter((entry) => entry.status === "pending").length,
    [approvals],
  );
  const traceCount = cognitiveEvents.length;
  const shortcutFor = useCallback(
    (action: string): string | undefined => bindings.find((binding) => binding.action === action)?.combo,
    [bindings],
  );

  const openAbout = useCallback(() => {
    if (!settingsOpen) {
      toggleSettings();
    }
    window.setTimeout(() => {
      window.dispatchEvent(new CustomEvent("zorai-open-settings-tab", {
        detail: { tab: "about" },
      }));
      window.dispatchEvent(new CustomEvent("zorai-open-settings-tab", {
        detail: { tab: "about" },
      }));
    }, 50);
  }, [settingsOpen, toggleSettings]);

  const linuxMenus = useMemo<TitleMenuGroup[]>(() => [
    {
      id: "workspace",
      label: "Workspace",
      items: [
        { id: "new-workspace", label: "New Workspace", shortcut: shortcutFor("newWorkspace"), onSelect: () => createWorkspace() },
        { id: "new-surface", label: "New Surface", shortcut: shortcutFor("newSurface"), onSelect: () => createSurface() },
        { id: "split-horizontal", label: "Split Horizontal", shortcut: shortcutFor("splitHorizontal"), onSelect: () => splitActive("horizontal") },
        { id: "split-vertical", label: "Split Vertical", shortcut: shortcutFor("splitVertical"), onSelect: () => splitActive("vertical") },
        { id: "toggle-zoom", label: "Zoom Pane", shortcut: shortcutFor("toggleZoom"), onSelect: toggleZoom },
      ],
    },
    {
      id: "panels",
      label: "Panels",
      items: [
        { id: "mission", label: "Mission Console", shortcut: shortcutFor("toggleAgentPanel"), tone: "agent", onSelect: toggleAgentPanel },
        { id: "notifications", label: "Notifications", shortcut: shortcutFor("toggleNotifications"), onSelect: toggleNotificationPanel },
        { id: "monitor", label: "System Monitor", shortcut: shortcutFor("toggleSystemMonitor"), onSelect: toggleSystemMonitor },
        { id: "files", label: "File Manager", shortcut: shortcutFor("toggleFileManager"), onSelect: toggleFileManager },
        { id: "vault", label: "Session Vault", shortcut: shortcutFor("toggleSessionVault"), onSelect: toggleSessionVault },
        { id: "settings", label: "Settings", shortcut: shortcutFor("toggleSettings"), onSelect: toggleSettings },
        { id: "sidebar", label: "Toggle Sidebar", shortcut: shortcutFor("toggleSidebar"), onSelect: toggleSidebar },
      ],
    },
    {
      id: "tools",
      label: "Tools",
      items: [
        { id: "palette", label: "Command Palette", shortcut: shortcutFor("toggleCommandPalette"), onSelect: toggleCommandPalette },
        { id: "search", label: "Search", shortcut: shortcutFor("toggleSearch"), onSelect: toggleSearch },
        { id: "history", label: "Command History", shortcut: shortcutFor("toggleCommandHistory"), onSelect: toggleCommandHistory },
        { id: "logs", label: "Command Log", shortcut: shortcutFor("toggleCommandLog"), onSelect: toggleCommandLog },
        { id: "time-travel", label: "Time Travel", shortcut: shortcutFor("toggleTimeTravel"), onSelect: toggleTimeTravel },
        { id: "snippets", label: "Snippets", shortcut: shortcutFor("toggleSnippets"), onSelect: toggleSnippets },
        { id: "runtime", label: "Runtime Settings", onSelect: openAbout },
      ],
    },
  ], [
    createSurface,
    createWorkspace,
    openAbout,
    shortcutFor,
    splitActive,
    toggleAgentPanel,
    toggleCommandHistory,
    toggleCommandLog,
    toggleNotificationPanel,
    toggleSnippets,
    toggleTimeTravel,

    toggleCommandPalette,
    toggleFileManager,
    toggleSearch,
    toggleSessionVault,
    toggleSettings,
    toggleSidebar,
    toggleSystemMonitor,
    toggleZoom,
  ]);

  useEffect(() => {
    const zorai = getBridge();
    if (!zorai?.onWindowState) return;

    zorai.getPlatform?.().then((value: string) => setPlatform(value));

    const cleanup = zorai.onWindowState((state: string) => {
      setMaximized(state === "maximized");
    });

    zorai.windowIsMaximized?.().then((m: boolean) => setMaximized(m));

    return cleanup;
  }, []);

  useEffect(() => {
    if (!openMenuId) {
      return;
    }

    const handlePointerDown = (event: MouseEvent) => {
      if (!menuBarRef.current?.contains(event.target as Node)) {
        setOpenMenuId(null);
      }
    };

    const handleEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setOpenMenuId(null);
      }
    };

    window.addEventListener("mousedown", handlePointerDown);
    window.addEventListener("keydown", handleEscape);
    return () => {
      window.removeEventListener("mousedown", handlePointerDown);
      window.removeEventListener("keydown", handleEscape);
    };
  }, [openMenuId]);

  const hasZorai = typeof window !== "undefined" && ("zorai" in window || "zorai" in window);
  if (!hasZorai) return null;
  if (platform === null) return null;
  if (platform === "win32") return null;

  const zorai = getBridge();

  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "space-between",
        height: "var(--title-bar-height)",
        background: "var(--bg-secondary)",
        borderBottom: "1px solid var(--border)",
        WebkitAppRegion: "drag",
        flexShrink: 0,
        padding: "0 var(--space-3) 0 var(--space-4)",
        userSelect: "none",
      } as React.CSSProperties}
    >
      <div style={{ display: "flex", alignItems: "center", gap: "var(--space-3)", fontSize: "var(--text-sm)", fontWeight: 600 }}>
        <div style={{ display: "flex", flexDirection: "column", gap: 1 }}>
          <span
            style={{
              color: "var(--mission)",
              letterSpacing: "0.15em",
              textTransform: "uppercase",
              fontSize: "var(--text-xs)",
              fontWeight: 700,
            }}
          >
            {ZORAI_APP_NAME}
          </span>
          <span style={{ color: "var(--text-muted)", fontSize: "var(--text-xs)" }}>agentic runtime</span>
        </div>

        {workspace && (
          <div style={{ display: "flex", alignItems: "center", gap: "var(--space-2)", fontSize: "var(--text-xs)", color: "var(--text-secondary)" }}>
            <span className="zorai-chip" style={{ color: workspace.accentColor }}>
              {workspace.name}
              {surface && <span style={{ color: "var(--text-muted)" }}>/{surface.name}</span>}
            </span>

            <span className="zorai-chip">provider {active_provider}</span>

            <span
              className="zorai-chip"
              style={{
                color: approvalCount > 0 ? "var(--approval)" : "var(--success)",
                background: approvalCount > 0 ? "var(--approval-soft)" : "var(--success-soft)",
              }}
            >
              {approvalCount > 0 ? `${approvalCount} approvals` : "safe lane"}
            </span>

            <span className="zorai-chip">trace {traceCount}</span>
          </div>
        )}
      </div>

      {platform === "linux" ? (
        <div
          ref={menuBarRef}
          style={{ display: "flex", alignItems: "stretch", gap: 2, WebkitAppRegion: "no-drag", position: "relative" } as React.CSSProperties}
        >
          {linuxMenus.map((menu) => (
            <div key={menu.id} style={{ position: "relative" }}>
              <button
                type="button"
                onClick={() => setOpenMenuId((current) => (current === menu.id ? null : menu.id))}
                style={{
                  height: 28,
                  marginTop: 6,
                  padding: "0 var(--space-3)",
                  borderRadius: "var(--radius-md)",
                  border: "1px solid",
                  borderColor: openMenuId === menu.id ? "var(--mission-border)" : "transparent",
                  background: openMenuId === menu.id ? "var(--mission-soft)" : "transparent",
                  color: openMenuId === menu.id ? "var(--text-primary)" : "var(--text-secondary)",
                  fontSize: "var(--text-xs)",
                  fontWeight: 600,
                  letterSpacing: "0.03em",
                }}
              >
                {menu.label}
              </button>

              {openMenuId === menu.id ? (
                <div
                  style={{
                    position: "absolute",
                    top: "calc(100% + 8px)",
                    left: 0,
                    minWidth: 240,
                    padding: "var(--space-2)",
                    borderRadius: "var(--radius-lg)",
                    border: "1px solid var(--glass-border)",
                    background: "var(--bg-elevated)",
                    boxShadow: "0 18px 48px color-mix(in srgb, var(--shade) 35%, transparent)",
                    display: "grid",
                    gap: 2,
                    zIndex: 40,
                  }}
                >
                  {menu.items.map((item) => (
                    <button
                      key={item.id}
                      type="button"
                      onClick={() => {
                        setOpenMenuId(null);
                        item.onSelect();
                      }}
                      style={{
                        display: "flex",
                        alignItems: "center",
                        justifyContent: "space-between",
                        gap: "var(--space-3)",
                        width: "100%",
                        padding: "var(--space-2) var(--space-3)",
                        borderRadius: "var(--radius-md)",
                        background: item.tone === "agent" ? "var(--agent-soft)" : "transparent",
                        color: item.tone === "agent" ? "var(--agent)" : "var(--text-primary)",
                        textAlign: "left",
                      }}
                      onMouseEnter={(event) => {
                        event.currentTarget.style.background = item.tone === "agent" ? "color-mix(in srgb, var(--info) 20%, transparent)" : "var(--bg-tertiary)";
                      }}
                      onMouseLeave={(event) => {
                        event.currentTarget.style.background = item.tone === "agent" ? "var(--agent-soft)" : "transparent";
                      }}
                    >
                      <span style={{ fontSize: "var(--text-sm)" }}>{item.label}</span>
                      <span style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)", whiteSpace: "nowrap" }}>
                        {item.shortcut ?? ""}
                      </span>
                    </button>
                  ))}
                </div>
              ) : null}
            </div>
          ))}
        </div>
      ) : (
        <div style={{ display: "flex", alignItems: "center", gap: "var(--space-1)", WebkitAppRegion: "no-drag" } as React.CSSProperties}>
          <ActionPill label="Mission" onClick={toggleAgentPanel} tone="agent" />
          <ActionPill label="Monitor" onClick={toggleSystemMonitor} />
          <ActionPill label="Palette" onClick={toggleCommandPalette} />
          <ActionPill label="Search" onClick={toggleSearch} />
          <ActionPill label="History" onClick={toggleCommandHistory} />
          <ActionPill label="Logs" onClick={toggleCommandLog} />
        </div>
      )}

      <div style={{ display: "flex", alignItems: "center", gap: "var(--space-2)", WebkitAppRegion: "no-drag" } as React.CSSProperties}>
        <button
          type="button"
          onClick={toggleNotificationPanel}
          title={unreadNotifications > 0 ? `${unreadNotifications} unread notification(s)` : "Open notifications"}
          style={{
            border: "1px solid",
            borderColor: unreadNotifications > 0 ? "var(--approval-border)" : "var(--glass-border)",
            background: notificationPanelOpen
              ? "var(--mission-soft)"
              : unreadNotifications > 0
                ? "var(--approval-soft)"
                : "transparent",
            color: unreadNotifications > 0 ? "var(--approval)" : "var(--text-secondary)",
            borderRadius: "var(--radius-md)",
            height: 28,
            minWidth: unreadNotifications > 0 ? 40 : 32,
            padding: "0 8px",
            display: "inline-flex",
            alignItems: "center",
            justifyContent: "center",
            gap: 6,
            fontSize: "var(--text-xs)",
            fontWeight: 700,
            cursor: "pointer",
            position: "relative",
          }}
        >
          <span style={{ letterSpacing: "0.06em" }}>NTF</span>
          {unreadNotifications > 0 ? (
            <span
              style={{
                minWidth: 16,
                lineHeight: "16px",
                padding: "0 4px",
                borderRadius: "var(--radius-full)",
                background: "var(--approval)",
                color: "var(--zorai-bg)",
                fontSize: 10,
                fontWeight: 800,
              }}
            >
              {unreadNotifications > 99 ? "99+" : unreadNotifications}
            </span>
          ) : null}
        </button>
        <WindowButton label="─" onClick={() => zorai?.windowMinimize?.()} />
        <WindowButton label={maximized ? "❐" : "□"} onClick={() => zorai?.windowMaximize?.()} />
        <WindowButton label="✕" onClick={() => zorai?.windowClose?.()} isClose />
      </div>
    </div>
  );
}

function ActionPill({ label, onClick, tone = "default" }: { label: string; onClick: () => void; tone?: "default" | "agent" }) {
  return (
    <button
      onClick={onClick}
      style={{
        border: "1px solid",
        borderColor: tone === "agent" ? "var(--agent-soft)" : "var(--glass-border)",
        background: tone === "agent" ? "var(--agent-soft)" : "transparent",
        color: tone === "agent" ? "var(--agent)" : "var(--text-secondary)",
        borderRadius: "var(--radius-full)",
        padding: "var(--space-1) var(--space-2)",
        fontSize: "var(--text-xs)",
        cursor: "pointer",
        transition: "all var(--transition-fast)",
      }}
      onMouseEnter={(e) => {
        e.currentTarget.style.background = tone === "agent" ? "color-mix(in srgb, var(--info) 20%, transparent)" : "var(--bg-tertiary)";
        e.currentTarget.style.color = "var(--text-primary)";
      }}
      onMouseLeave={(e) => {
        e.currentTarget.style.background = tone === "agent" ? "var(--agent-soft)" : "transparent";
        e.currentTarget.style.color = tone === "agent" ? "var(--agent)" : "var(--text-secondary)";
      }}
    >
      {label}
    </button>
  );
}

function WindowButton({ label, onClick, isClose }: { label: string; onClick: () => void; isClose?: boolean }) {
  return (
    <button
      onClick={onClick}
      style={{
        width: 44,
        height: "var(--title-bar-height)",
        border: "none",
        background: "transparent",
        color: "var(--text-muted)",
        cursor: "pointer",
        fontSize: "var(--text-sm)",
        fontFamily: "inherit",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        borderRadius: 0,
        transition: "all var(--transition-fast)",
      }}
      onMouseEnter={(e) => {
        e.currentTarget.style.background = isClose ? "var(--danger)" : "var(--bg-tertiary)";
        e.currentTarget.style.color = isClose ? "var(--text-on-accent)" : "var(--text-primary)";
      }}
      onMouseLeave={(e) => {
        e.currentTarget.style.background = "transparent";
        e.currentTarget.style.color = "var(--text-muted)";
      }}
    >
      {label}
    </button>
  );
}
