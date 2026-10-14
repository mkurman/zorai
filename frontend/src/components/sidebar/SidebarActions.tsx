export function SidebarActions({
    workspacesCount,
    toggleAgentPanel,
    toggleSystemMonitor,
    toggleCommandPalette,
    toggleSearch,
    toggleCommandHistory,
    toggleCommandLog,
    toggleFileManager,
    toggleSessionVault,
    toggleSettings,
}: {
    workspacesCount: number;
    toggleAgentPanel: () => void;
    toggleSystemMonitor: () => void;
    toggleCommandPalette: () => void;
    toggleSearch: () => void;
    toggleCommandHistory: () => void;
    toggleCommandLog: () => void;
    toggleFileManager: () => void;
    toggleSessionVault: () => void;
    toggleSettings: () => void;
}) {
    const actions = [
        { label: "Mission", onClick: toggleAgentPanel, accent: true },
        { label: "Monitor", onClick: toggleSystemMonitor },
        { label: "Palette", onClick: toggleCommandPalette },
        { label: "Search", onClick: toggleSearch },
        { label: "History", onClick: toggleCommandHistory },
        { label: "Logs", onClick: toggleCommandLog },
        { label: "Files", onClick: toggleFileManager, accent: true },
        { label: "Vault", onClick: toggleSessionVault },
        { label: "Settings", onClick: toggleSettings },
    ];

    return (
        <div style={{ borderTop: "1px solid var(--border)", background: "var(--bg-secondary)" }}>
            <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(48px, 1fr))", gap: 2, padding: "var(--space-2) var(--space-3)" }}>
                {actions.map((item) => (
                    <button
                        key={item.label}
                        onClick={(e) => { e.stopPropagation(); item.onClick(); }}
                        title={item.label}
                        style={{
                            background: item.accent ? "color-mix(in srgb, var(--info) 8%, transparent)" : "var(--zorai-bg-surface)",
                            border: "1px solid",
                            borderColor: item.accent ? "color-mix(in srgb, var(--info) 20%, transparent)" : "var(--glass-border)",
                            color: item.accent ? "var(--agent)" : "var(--text-muted)",
                            cursor: "pointer",
                            fontSize: "var(--text-xs)",
                            fontWeight: 600,
                            padding: "4px 2px",
                            borderRadius: "var(--radius-sm)",
                            textTransform: "uppercase",
                            letterSpacing: "0.05em",
                            transition: "all var(--transition-fast)",
                            lineHeight: 1.2,
                            textAlign: "center",
                        }}
                        onMouseEnter={(e) => {
                            e.currentTarget.style.background = item.accent ? "color-mix(in srgb, var(--info) 15%, transparent)" : "color-mix(in srgb, var(--text-primary) 6%, transparent)";
                            e.currentTarget.style.color = item.accent ? "var(--agent)" : "var(--text-primary)";
                        }}
                        onMouseLeave={(e) => {
                            e.currentTarget.style.background = item.accent ? "color-mix(in srgb, var(--info) 8%, transparent)" : "var(--zorai-bg-surface)";
                            e.currentTarget.style.color = item.accent ? "var(--agent)" : "var(--text-muted)";
                        }}
                    >
                        {item.label}
                    </button>
                ))}
            </div>
            <div style={{ padding: "var(--space-2) var(--space-4)", fontSize: "var(--text-xs)", color: "var(--text-muted)", textAlign: "center" }}>
                {workspacesCount} workspace{workspacesCount !== 1 ? "s" : ""}
            </div>
        </div>
    );
}
