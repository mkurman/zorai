export function CommandLogHeader({
    visibleCount,
    totalCount,
    failureCount,
    runningCount,
}: {
    visibleCount: number;
    totalCount: number;
    failureCount: number;
    runningCount: number;
}) {
    return (
        <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(120px, 1fr))", gap: 10, padding: "12px 12px 0" }}>
            <MetricCard label="Visible" value={String(visibleCount)} />
            <MetricCard label="Total" value={String(totalCount)} />
            <MetricCard label="Failures" value={String(failureCount)} />
            <MetricCard label="Running" value={String(runningCount)} />
        </div>
    );
}

function MetricCard({ label, value }: { label: string; value: string }) {
    return (
        <div style={{ borderRadius: 10, padding: "10px 12px", border: "1px solid var(--zorai-border)", background: "var(--zorai-bg-panel)", display: "grid", gap: 4, minWidth: 0 }}>
            <span style={{ fontSize: "var(--text-xs)", color: "var(--zorai-muted)" }}>{label}</span>
            <span style={{ fontSize: "var(--text-base)", fontWeight: 600, color: "var(--zorai-text)", fontVariantNumeric: "tabular-nums" }}>{value}</span>
        </div>
    );
}
