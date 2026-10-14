import { actionBtnStyle } from "./shared";

export function NotificationHeader({
    unreadCount,
    totalCount,
    markAllRead,
    archiveRead,
    close,
}: {
    unreadCount: number;
    totalCount: number;
    markAllRead: () => void;
    archiveRead: () => void;
    close: () => void;
}) {
    return (
        <div
            style={{
                display: "grid",
                gap: 12,
                padding: "18px 18px 14px",
                borderBottom: "1px solid color-mix(in srgb, var(--text-primary) 8%, transparent)",
            }}
        >
            <div style={{ display: "flex", alignItems: "start", justifyContent: "space-between", gap: 12 }}>
                <div style={{ display: "grid", gap: 6 }}>
                    <span className="zorai-panel-title" style={{ color: "var(--mission)" }}>Mission Feed</span>
                    <span style={{ fontSize: "var(--text-xl)", fontWeight: 800 }}>Notifications</span>
                </div>
                <div style={{ display: "flex", gap: 8 }}>
                    <button onClick={markAllRead} style={actionBtnStyle} title="Mark all read">Read All</button>
                    <button onClick={archiveRead} style={actionBtnStyle} title="Archive read notifications">Archive Read</button>
                    <button onClick={close} style={actionBtnStyle} title="Close">✕</button>
                </div>
            </div>
            <div style={{ display: "grid", gridTemplateColumns: "repeat(3, minmax(0, 1fr))", gap: 10 }}>
                <MetricCard label="Unread" value={String(unreadCount)} />
                <MetricCard label="Total" value={String(totalCount)} />
                <MetricCard label="State" value={unreadCount > 0 ? "attention" : "quiet"} />
            </div>
        </div>
    );
}

function MetricCard({ label, value }: { label: string; value: string }) {
    return (
        <div style={{ borderRadius: 0, padding: "10px 12px", border: "1px solid color-mix(in srgb, var(--text-primary) 6%, transparent)", background: "color-mix(in srgb, var(--bg-surface) 80%, transparent)", display: "grid", gap: 4 }}>
            <span className="zorai-panel-title">{label}</span>
            <span style={{ fontSize: "var(--text-base)", fontWeight: 700 }}>{value}</span>
        </div>
    );
}
