import type { MemoryProvenanceReport } from "./shared";

export function SessionVaultMemoryView({
    report,
    status,
    loading,
    confirmMemoryEntry,
    retractMemoryEntry,
}: {
    report: MemoryProvenanceReport | null;
    status: string | null;
    loading: boolean;
    confirmMemoryEntry: (entryId: string) => Promise<void>;
    retractMemoryEntry: (entryId: string) => Promise<void>;
}) {
    if (loading) {
        return <EmptyState message="Loading memory provenance..." />;
    }

    if (!report) {
        return <EmptyState message={status || "No memory provenance loaded yet."} />;
    }

    return (
        <div style={{ padding: 12, display: "grid", gap: 10 }}>
            <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
                <Metric label="Entries" value={String(report.total_entries)} />
                <Metric label="Uncertain" value={String(report.summary_by_status.uncertain ?? 0)} />
                <Metric label="Active" value={String(report.summary_by_status.active ?? 0)} />
                <Metric label="Retracted" value={String(report.summary_by_status.retracted ?? 0)} />
            </div>

            {status ? (
                <div style={{ fontSize: "var(--text-xs)", color: "var(--zorai-muted)" }}>{status}</div>
            ) : null}

            <div style={{ display: "grid", gap: 8 }}>
                {report.entries.map((entry) => (
                    <div
                        key={entry.id}
                        style={{
                            border: "1px solid var(--zorai-border)",
                            borderRadius: 10,
                            background: "var(--zorai-bg-panel)",
                            padding: "10px 12px",
                            display: "grid",
                            gap: 6,
                        }}
                    >
                        <div style={{ display: "flex", justifyContent: "space-between", gap: 8, flexWrap: "wrap", alignItems: "center" }}>
                            <div style={{ fontSize: "var(--text-sm)", fontWeight: 600 }}>{entry.target}</div>
                            <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                                <span style={{ fontSize: "var(--text-xs)", color: statusColor(entry.status) }}>
                                    {entry.status}
                                </span>
                                {entry.status === "uncertain" ? (
                                    <button
                                        type="button"
                                        onClick={() => void confirmMemoryEntry(entry.id)}
                                        className="zorai-ghost-button zorai-ghost-button--compact"
                                    >
                                        Confirm
                                    </button>
                                ) : null}
                                {entry.status !== "retracted" ? (
                                    <button
                                        type="button"
                                        onClick={() => void retractMemoryEntry(entry.id)}
                                        className="zorai-ghost-button zorai-ghost-button--compact"
                                    >
                                        Retract
                                    </button>
                                ) : null}
                            </div>
                        </div>
                        <div style={{ fontSize: "var(--text-xs)", color: "var(--zorai-muted)", fontVariantNumeric: "tabular-nums" }}>
                            {entry.source_kind} · {entry.mode} · age {entry.age_days.toFixed(1)}d · confidence {(entry.confidence * 100).toFixed(0)}%
                        </div>
                        <div style={{ fontSize: "var(--text-sm)", color: "var(--zorai-text)", whiteSpace: "pre-wrap", lineHeight: 1.5 }}>
                            {entry.content}
                        </div>
                        {entry.fact_keys.length > 0 ? (
                            <div style={{ fontSize: "var(--text-xs)", color: "var(--zorai-muted)" }}>
                                Facts: {entry.fact_keys.join(", ")}
                            </div>
                        ) : null}
                        {entry.relationships.length > 0 ? (
                            <div style={{ fontSize: "var(--text-xs)", color: "var(--zorai-muted)", display: "grid", gap: 2 }}>
                                <span>Relationships:</span>
                                {entry.relationships.map((relationship) => (
                                    <span key={`${relationship.relation_type}-${relationship.related_entry_id}-${relationship.fact_key ?? "none"}`}>
                                        {relationship.relation_type} {relationship.related_entry_id}
                                        {relationship.fact_key ? ` (${relationship.fact_key})` : ""}
                                    </span>
                                ))}
                            </div>
                        ) : null}
                    </div>
                ))}
            </div>
        </div>
    );
}

function Metric({ label, value }: { label: string; value: string }) {
    return (
        <div style={{ border: "1px solid var(--zorai-border)", borderRadius: 10, background: "var(--zorai-bg-panel)", padding: "10px 12px", minWidth: 100, display: "grid", gap: 2 }}>
            <span style={{ fontSize: "var(--text-xs)", color: "var(--zorai-muted)" }}>{label}</span>
            <span style={{ fontSize: "var(--text-base)", fontWeight: 600, fontVariantNumeric: "tabular-nums" }}>{value}</span>
        </div>
    );
}

function EmptyState({ message }: { message: string }) {
    return (
        <div style={{ padding: 32, textAlign: "center", color: "var(--zorai-muted)", fontSize: "var(--text-sm)" }}>
            {message}
        </div>
    );
}

function statusColor(status: string): string {
    switch (status) {
        case "uncertain":
            return "var(--warning)";
        case "retracted":
            return "var(--danger)";
        default:
            return "var(--success)";
    }
}