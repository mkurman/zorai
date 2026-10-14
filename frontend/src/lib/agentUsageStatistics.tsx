import { useEffect, useMemo, useState } from "react";
import { getBridge } from "./bridge";
import type React from "react";
import { LoadingPanel, LoadingState } from "../components/LoadingState";
import { ActionButton, EmptyPanel, MetricRibbon, SectionTitle } from "../components/agent-chat-panel/shared";

const STATISTICS_WINDOWS: Array<{ value: ZoraiStatisticsWindow; label: string }> = [
    { value: "today", label: "Today" },
    { value: "7d", label: "7d" },
    { value: "30d", label: "30d" },
    { value: "all", label: "All" },
];

const USD = new Intl.NumberFormat("en-US", { style: "currency", currency: "USD", maximumFractionDigits: 6 });

export function AgentStatisticsView() {
    const [window, setWindow] = useState<ZoraiStatisticsWindow>("all");
    const [snapshot, setSnapshot] = useState<ZoraiAgentStatisticsSnapshot | null>(null);
    const [loading, setLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);

    useEffect(() => {
        let cancelled = false;
        const bridge = getBridge();

        if (!bridge?.agentGetStatistics) {
            setSnapshot(null);
            setError("Statistics bridge is unavailable in this session.");
            setLoading(false);
            return () => {
                cancelled = true;
            };
        }

        setLoading(true);
        setError(null);

        void bridge
            .agentGetStatistics(window)
            .then((result) => {
                if (cancelled) return;
                setSnapshot((result ?? null) as ZoraiAgentStatisticsSnapshot | null);
            })
            .catch((fetchError) => {
                if (cancelled) return;
                setSnapshot(null);
                setError(fetchError?.message || String(fetchError));
            })
            .finally(() => {
                if (!cancelled) {
                    setLoading(false);
                }
            });

        return () => {
            cancelled = true;
        };
    }, [window]);

    const totals = snapshot?.totals;
    const generatedAt = snapshot ? new Date(snapshot.generated_at).toLocaleString() : null;

    const totalsItems = useMemo(() => {
        if (!totals) {
            return [];
        }
        return [
            { label: "Input Tokens", value: totals.input_tokens.toLocaleString() },
            { label: "Output Tokens", value: totals.output_tokens.toLocaleString() },
            { label: "Total Tokens", value: totals.total_tokens.toLocaleString() },
            { label: "Cost", value: USD.format(totals.cost_usd) },
            { label: "Providers", value: totals.provider_count.toLocaleString() },
            { label: "Models", value: totals.model_count.toLocaleString() },
        ];
    }, [totals]);

    if (error) {
        return (
            <div style={{ display: "grid", gap: "var(--space-3)" }}>
                <SectionTitle title="Agent Statistics" subtitle="Daemon-backed usage snapshots" />
                <EmptyPanel message={error} />
            </div>
        );
    }

    if (loading && !snapshot) {
        return (
            <div style={{ display: "grid", gap: "var(--space-3)" }}>
                <SectionTitle title="Agent Statistics" subtitle="Loading daemon snapshot..." />
                <LoadingPanel label="Fetching statistics from the daemon…" />
            </div>
        );
    }

    return (
        <div style={{ display: "grid", gap: "var(--space-3)" }}>
            <div style={{ display: "flex", alignItems: "center", gap: "var(--space-2)", flexWrap: "wrap" }}>
                {STATISTICS_WINDOWS.map((option) => (
                    <ActionButton key={option.value} onClick={() => setWindow(option.value)} disabled={option.value === window}>{option.label}</ActionButton>
                ))}
                <span style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)" }}>
                    {generatedAt ? `Generated ${generatedAt}` : "Generated time unavailable"}
                </span>
                {loading ? <LoadingState size={14} label="Refreshing" /> : null}
            </div>

            {totalsItems.length > 0 ? <MetricRibbon items={totalsItems} /> : <EmptyPanel message="No totals available for this window." />}

            {snapshot?.has_incomplete_cost_history ? (
                <div style={{ padding: "var(--space-3)", borderRadius: "var(--radius-lg)", border: "1px solid color-mix(in srgb, var(--warning) 50%, transparent)", background: "color-mix(in srgb, var(--warning) 8%, transparent)", color: "var(--text-primary)" }}>
                    Cost data is incomplete for part of this window. Totals are still useful, but some rows are missing pricing history.
                </div>
            ) : null}

            <div style={{ display: "flex", gap: "var(--space-2)", flexWrap: "wrap" }}>
                <SectionTitle title="Providers" subtitle="Canonical provider aggregates from the daemon snapshot" />
            </div>
            <StatisticsTable
                columns={["Provider", "Input", "Output", "Total", "Cost"]}
                rows={snapshot?.providers ?? []}
                emptyMessage="No provider statistics for this window."
                rowKey={(row) => row.provider}
                renderRow={(row) => [
                    row.provider,
                    row.input_tokens.toLocaleString(),
                    row.output_tokens.toLocaleString(),
                    row.total_tokens.toLocaleString(),
                    USD.format(row.cost_usd),
                ]}
            />

            <SectionTitle title="Models" subtitle="Canonical model aggregates from the daemon snapshot" />
            <StatisticsTable
                columns={["Provider / Model", "Input", "Output", "Total", "Cost"]}
                rows={snapshot?.models ?? []}
                emptyMessage="No model statistics for this window."
                rowKey={(row) => `${row.provider}/${row.model}`}
                renderRow={(row) => [
                    `${row.provider}/${row.model}`,
                    row.input_tokens.toLocaleString(),
                    row.output_tokens.toLocaleString(),
                    row.total_tokens.toLocaleString(),
                    USD.format(row.cost_usd),
                ]}
            />

            <div style={{ display: "grid", gridTemplateColumns: "repeat(2, minmax(0, 1fr))", gap: "var(--space-3)" }}>
                <RankingTable
                    title="Top 5 by Tokens"
                    rows={snapshot?.top_models_by_tokens ?? []}
                    emptyMessage="No token ranking available."
                    rowKey={(row) => `${row.provider}/${row.model}`}
                    renderValue={(row) => row.total_tokens.toLocaleString()}
                    renderMeta={(row) => `${row.provider}/${row.model}`}
                />
                <RankingTable
                    title="Top 5 by Cost"
                    rows={snapshot?.top_models_by_cost ?? []}
                    emptyMessage="No cost ranking available."
                    rowKey={(row) => `${row.provider}/${row.model}`}
                    renderValue={(row) => USD.format(row.cost_usd)}
                    renderMeta={(row) => `${row.provider}/${row.model}`}
                />
            </div>
        </div>
    );
}

function StatisticsTable<T extends Record<string, unknown>>({
    columns,
    rows,
    rowKey,
    renderRow,
    emptyMessage,
}: {
    columns: string[];
    rows: T[];
    rowKey: (row: T) => string;
    renderRow: (row: T) => string[];
    emptyMessage: string;
}) {
    if (rows.length === 0) {
        return <EmptyPanel message={emptyMessage} />;
    }

    return (
        <TableShell>
            <thead>
                <tr style={{ position: "sticky", top: 0, background: "var(--bg-tertiary)", zIndex: 1 }}>
                    {columns.map((column) => <Th key={column}>{column}</Th>)}
                </tr>
            </thead>
            <tbody>
                {rows.map((row) => (
                    <tr key={rowKey(row)} style={{ borderTop: "1px solid color-mix(in srgb, var(--text-primary) 8%, transparent)" }}>
                        {renderRow(row).map((value, index) => (
                            <Td key={`${rowKey(row)}-${index}`}>{value}</Td>
                        ))}
                    </tr>
                ))}
            </tbody>
        </TableShell>
    );
}

function RankingTable<T extends { provider: string; model: string }>({
    title,
    rows,
    rowKey,
    renderValue,
    renderMeta,
    emptyMessage,
}: {
    title: string;
    rows: T[];
    rowKey: (row: T) => string;
    renderValue: (row: T) => string;
    renderMeta: (row: T) => string;
    emptyMessage: string;
}) {
    return (
        <div style={{ display: "grid", gap: "var(--space-2)" }}>
            <SectionTitle title={title} subtitle="Top five canonical model rows from the daemon snapshot" />
            {rows.length === 0 ? (
                <EmptyPanel message={emptyMessage} />
            ) : (
                <TableShell>
                    <thead>
                        <tr style={{ position: "sticky", top: 0, background: "var(--bg-tertiary)", zIndex: 1 }}>
                            <Th>Model</Th>
                            <Th>Value</Th>
                        </tr>
                    </thead>
                    <tbody>
                        {rows.map((row, index) => (
                            <tr key={rowKey(row)} style={{ borderTop: "1px solid color-mix(in srgb, var(--text-primary) 8%, transparent)" }}>
                                <Td>{index + 1}. {renderMeta(row)}</Td>
                                <Td>{renderValue(row)}</Td>
                            </tr>
                        ))}
                    </tbody>
                </TableShell>
            )}
        </div>
    );
}

function TableShell({ children }: { children: React.ReactNode }) {
    return (
        <div style={{ overflow: "auto", border: "1px solid var(--glass-border)", borderRadius: "var(--radius-lg)" }}>
            <table style={{ width: "100%", borderCollapse: "collapse", fontSize: "var(--text-sm)" }}>
                {children}
            </table>
        </div>
    );
}

function Th({ children }: { children: React.ReactNode }) {
    return <th style={{ textAlign: "left", padding: "8px 10px", color: "var(--text-muted)", fontWeight: 600, borderBottom: "1px solid color-mix(in srgb, var(--text-primary) 10%, transparent)", whiteSpace: "nowrap" }}>{children}</th>;
}

function Td({ children }: { children: React.ReactNode }) {
    return <td style={{ padding: "8px 10px", color: "var(--text-secondary)", whiteSpace: "nowrap", verticalAlign: "top" }}>{children}</td>;
}
