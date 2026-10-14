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
        <div style={{ padding: 18, display: "grid", gap: 12 }}>
            <div style={{ display: "flex", gap: 10, flexWrap: "wrap" }}>
                <Metric label="Entries" value={String(report.total_entries)} />
                <Metric label="Uncertain" value={String(report.summary_by_status.uncertain ?? 0)} />
                <Metric label="Active" value={String(report.summary_by_status.active ?? 0)} />
                <Metric label="Retracted" value={String(report.summary_by_status.retracted ?? 0)} />
            </div>

            {status ? (
                <div style={{ fontSize: "var(--text-sm)", color: "var(--text-secondary)" }}>{status}</div>
            ) : null}

            <div style={{ display: "grid", gap: 10 }}>
                {report.entries.map((entry) => (
                    <div
                        key={entry.id}
                        style={{
                            border: "1px solid color-mix(in srgb, var(--text-primary) 8%, transparent)",
                            background: "color-mix(in srgb, var(--text-primary) 3%, transparent)",
                            padding: 14,
                            display: "grid",
                            gap: 8,
                        }}
                    >
                        <div style={{ display: "flex", justifyContent: "space-between", gap: 10, flexWrap: "wrap" }}>
                            <div style={{ fontSize: "var(--text-sm)", fontWeight: 700 }}>{entry.target}</div>
                            <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                                <span style={{ fontSize: "var(--text-sm)", color: statusColor(entry.status), textTransform: "uppercase", letterSpacing: "0.06em" }}>
                                    {entry.status}
                                </span>
                                {entry.status === "uncertain" ? (
                                    <button
                                        type="button"
                                        onClick={() => void confirmMemoryEntry(entry.id)}
                                        style={{
                                            fontSize: "var(--text-sm)",
                                            padding: "4px 8px",
                                            border: "1px solid color-mix(in srgb, var(--text-primary) 8%, transparent)",
                                            background: "var(--bg-secondary)",
                                            color: "var(--text-primary)",
                                            cursor: "pointer",
                                        }}
                                    >
                                        Confirm
                                    </button>
                                ) : null}
                                {entry.status !== "retracted" ? (
                                    <button
                                        type="button"
                                        onClick={() => void retractMemoryEntry(entry.id)}
                                        style={{
                                            fontSize: "var(--text-sm)",
                                            padding: "4px 8px",
                                            border: "1px solid color-mix(in srgb, var(--text-primary) 8%, transparent)",
                                            background: "var(--bg-secondary)",
                                            color: "var(--text-primary)",
                                            cursor: "pointer",
                                        }}
                                    >
                                        Retract
                                    </button>
                                ) : null}
                            </div>
                        </div>
                        <div style={{ fontSize: "var(--text-sm)", color: "var(--text-secondary)" }}>
                            {entry.source_kind} · {entry.mode} · age {entry.age_days.toFixed(1)}d · confidence {(entry.confidence * 100).toFixed(0)}%
                        </div>
                        <div style={{ fontSize: "var(--text-sm)", color: "var(--text-primary)", whiteSpace: "pre-wrap" }}>
                            {entry.content}
                        </div>
                        {entry.fact_keys.length > 0 ? (
                            <div style={{ fontSize: "var(--text-sm)", color: "var(--text-secondary)" }}>
                                Facts: {entry.fact_keys.join(", ")}
                            </div>
                        ) : null}
                        {entry.relationships.length > 0 ? (
                            <div style={{ fontSize: "var(--text-sm)", color: "var(--text-secondary)", display: "grid", gap: 4 }}>
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
        <div style={{ border: "1px solid color-mix(in srgb, var(--text-primary) 8%, transparent)", background: "var(--bg-secondary)", padding: "10px 12px", minWidth: 100, display: "grid", gap: 4 }}>
            <span style={{ fontSize: "var(--text-sm)", color: "var(--text-secondary)", textTransform: "uppercase", letterSpacing: "0.08em" }}>{label}</span>
            <span style={{ fontSize: "var(--text-base)", fontWeight: 700 }}>{value}</span>
        </div>
    );
}

function EmptyState({ message }: { message: string }) {
    return (
        <div style={{ padding: 32, textAlign: "center", color: "var(--text-secondary)", fontSize: "var(--text-sm)" }}>
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