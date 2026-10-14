import { useMemo, useState } from "react";
import type { AgentMessage, AgentThread } from "../../lib/agentStore";
import { AgentStatisticsView } from "../../lib/agentUsageStatistics";
import { ActionButton, EmptyPanel, MetricRibbon, SectionTitle, inputStyle } from "./shared";

type UsageSortKey = "totalTokens" | "promptTokens" | "completionTokens" | "cost" | "avgTps" | "requests";
type UsageWindow = "today" | "7d" | "30d" | "all";
type ExportFormat = "json" | "csv";

export function UsageView({
    threads,
    messagesByThread,
}: {
    threads: AgentThread[];
    messagesByThread: Record<string, AgentMessage[]>;
}) {
    const [query, setQuery] = useState("");
    const [sortKey, setSortKey] = useState<UsageSortKey>("totalTokens");
    const [sortDirection, setSortDirection] = useState<"asc" | "desc">("desc");
    const [window, setWindow] = useState<UsageWindow>("all");
    const [exportFormat, setExportFormat] = useState<ExportFormat>("json");
    const [compactMode, setCompactMode] = useState(false);
    const [mode, setMode] = useState<"explorer" | "statistics">("explorer");

    const windowStart = useMemo(() => {
        const now = Date.now();
        if (window === "today") return now - 24 * 60 * 60 * 1000;
        if (window === "7d") return now - 7 * 24 * 60 * 60 * 1000;
        if (window === "30d") return now - 30 * 24 * 60 * 60 * 1000;
        return 0;
    }, [window]);

    const graphWindows = useMemo(() => {
        const now = Date.now();
        return [
            { key: "daily", label: "Daily", start: now - 24 * 60 * 60 * 1000 },
            { key: "weekly", label: "Weekly", start: now - 7 * 24 * 60 * 60 * 1000 },
            { key: "monthly", label: "Monthly", start: now - 30 * 24 * 60 * 60 * 1000 },
        ] as const;
    }, []);

    const stats = useMemo(() => {
        const providerMap = new Map<string, {
            key: string;
            provider: string;
            model: string;
            requests: number;
            promptTokens: number;
            completionTokens: number;
            totalTokens: number;
            reasoningTokens: number;
            audioTokens: number;
            videoTokens: number;
            cost: number;
            avgTps: number;
            _tpsSamples: number;
        }>();

        const sessionRows: Array<{
            threadId: string;
            title: string;
            providerModels: string;
            requests: number;
            promptTokens: number;
            completionTokens: number;
            totalTokens: number;
            reasoningTokens: number;
            audioTokens: number;
            videoTokens: number;
            cost: number;
            avgTps: number;
            maxTps: number;
            minTps: number;
            updatedAt: number;
        }> = [];

        let totalRequests = 0;
        let totalPromptTokens = 0;
        let totalCompletionTokens = 0;
        let totalTokens = 0;
        let totalReasoningTokens = 0;
        let totalAudioTokens = 0;
        let totalVideoTokens = 0;
        let totalCost = 0;
        let tpsSamples = 0;
        let tpsSum = 0;
        let maxTps = 0;
        let minTps = Number.POSITIVE_INFINITY;

        for (const thread of threads) {
            const threadMessages = messagesByThread[thread.id] ?? [];
            const assistantMessages = threadMessages.filter((message) =>
                message.role === "assistant"
                && message.createdAt >= windowStart
                && ((message.totalTokens ?? 0) > 0 || message.cost !== undefined),
            );
            if (assistantMessages.length === 0) continue;

            let sessionPrompt = 0;
            let sessionCompletion = 0;
            let sessionTotal = 0;
            let sessionReasoning = 0;
            let sessionAudio = 0;
            let sessionVideo = 0;
            let sessionCost = 0;
            let sessionTpsSum = 0;
            let sessionTpsCount = 0;
            let sessionMaxTps = 0;
            let sessionMinTps = Number.POSITIVE_INFINITY;
            const providerModels = new Set<string>();

            for (const message of assistantMessages) {
                const provider = String(message.provider || "unknown");
                const model = String(message.model || "unknown");
                const key = `${provider}/${model}`;
                const prompt = Number(message.inputTokens ?? 0);
                const completion = Number(message.outputTokens ?? 0);
                const total = Number(message.totalTokens ?? (prompt + completion));
                const reasoning = Number(message.reasoningTokens ?? 0);
                const audio = Number(message.audioTokens ?? 0);
                const video = Number(message.videoTokens ?? 0);
                const cost = Number(message.cost ?? 0);

                let providerRow = providerMap.get(key);
                if (!providerRow) {
                    providerRow = {
                        key,
                        provider,
                        model,
                        requests: 0,
                        promptTokens: 0,
                        completionTokens: 0,
                        totalTokens: 0,
                        reasoningTokens: 0,
                        audioTokens: 0,
                        videoTokens: 0,
                        cost: 0,
                        avgTps: 0,
                        _tpsSamples: 0,
                    };
                    providerMap.set(key, providerRow);
                }

                providerRow.requests += 1;
                providerRow.promptTokens += prompt;
                providerRow.completionTokens += completion;
                providerRow.totalTokens += total;
                providerRow.reasoningTokens += reasoning;
                providerRow.audioTokens += audio;
                providerRow.videoTokens += video;
                providerRow.cost += cost;

                providerModels.add(key);
                totalRequests += 1;
                totalPromptTokens += prompt;
                totalCompletionTokens += completion;
                totalTokens += total;
                totalReasoningTokens += reasoning;
                totalAudioTokens += audio;
                totalVideoTokens += video;
                totalCost += cost;

                sessionPrompt += prompt;
                sessionCompletion += completion;
                sessionTotal += total;
                sessionReasoning += reasoning;
                sessionAudio += audio;
                sessionVideo += video;
                sessionCost += cost;

                if (typeof message.tps === "number" && Number.isFinite(message.tps) && message.tps > 0) {
                    providerRow.avgTps += message.tps;
                    providerRow._tpsSamples += 1;
                    tpsSum += message.tps;
                    tpsSamples += 1;
                    maxTps = Math.max(maxTps, message.tps);
                    minTps = Math.min(minTps, message.tps);
                    sessionTpsSum += message.tps;
                    sessionTpsCount += 1;
                    sessionMaxTps = Math.max(sessionMaxTps, message.tps);
                    sessionMinTps = Math.min(sessionMinTps, message.tps);
                }
            }

            sessionRows.push({
                threadId: thread.id,
                title: thread.title,
                providerModels: Array.from(providerModels).join(", "),
                requests: assistantMessages.length,
                promptTokens: sessionPrompt,
                completionTokens: sessionCompletion,
                totalTokens: sessionTotal,
                reasoningTokens: sessionReasoning,
                audioTokens: sessionAudio,
                videoTokens: sessionVideo,
                cost: sessionCost,
                avgTps: sessionTpsCount > 0 ? sessionTpsSum / sessionTpsCount : 0,
                maxTps: sessionMaxTps,
                minTps: sessionMinTps === Number.POSITIVE_INFINITY ? 0 : sessionMinTps,
                updatedAt: thread.updatedAt,
            });
        }

        const providerRows = Array.from(providerMap.values()).map((row) => ({
            ...row,
            avgTps: row._tpsSamples > 0 ? row.avgTps / row._tpsSamples : 0,
        }));

        return {
            providerRows,
            sessionRows,
            totals: {
                sessions: sessionRows.length,
                requests: totalRequests,
                promptTokens: totalPromptTokens,
                completionTokens: totalCompletionTokens,
                totalTokens,
                reasoningTokens: totalReasoningTokens,
                audioTokens: totalAudioTokens,
                videoTokens: totalVideoTokens,
                totalCost,
                avgCostPerSession: sessionRows.length > 0 ? totalCost / sessionRows.length : 0,
                avgTps: tpsSamples > 0 ? tpsSum / tpsSamples : 0,
                maxTps,
                minTps: minTps === Number.POSITIVE_INFINITY ? 0 : minTps,
            },
        };
    }, [messagesByThread, threads, windowStart]);

    const graphSeries = useMemo(() => {
        return graphWindows.map((entry) => {
            let tokens = 0;
            let cost = 0;

            for (const list of Object.values(messagesByThread)) {
                for (const message of list) {
                    if (message.role !== "assistant") continue;
                    if (message.createdAt < entry.start) continue;
                    tokens += Number(message.totalTokens ?? 0);
                    cost += Number(message.cost ?? 0);
                }
            }

            return {
                key: entry.key,
                label: entry.label,
                tokens,
                cost,
            };
        });
    }, [graphWindows, messagesByThread]);

    const filteredProviderRows = useMemo(() => {
        const q = query.trim().toLowerCase();
        if (!q) return stats.providerRows;
        return stats.providerRows.filter((row) => (`${row.provider}/${row.model}`).toLowerCase().includes(q));
    }, [query, stats.providerRows]);

    const filteredSessionRows = useMemo(() => {
        const q = query.trim().toLowerCase();
        if (!q) return stats.sessionRows;
        return stats.sessionRows.filter((row) =>
            row.title.toLowerCase().includes(q)
            || row.providerModels.toLowerCase().includes(q)
            || row.threadId.toLowerCase().includes(q),
        );
    }, [query, stats.sessionRows]);

    const compareBySort = <T extends { [k: string]: unknown }>(a: T, b: T) => {
        const av = Number(a[sortKey] ?? 0);
        const bv = Number(b[sortKey] ?? 0);
        const direction = sortDirection === "asc" ? 1 : -1;
        return (av - bv) * direction;
    };

    const sortedProviderRows = useMemo(() => [...filteredProviderRows].sort(compareBySort), [filteredProviderRows, sortKey, sortDirection]);
    const sortedSessionRows = useMemo(() => [...filteredSessionRows].sort(compareBySort), [filteredSessionRows, sortKey, sortDirection]);

    const updateSort = (key: UsageSortKey) => {
        if (sortKey === key) {
            setSortDirection((prev) => (prev === "desc" ? "asc" : "desc"));
            return;
        }
        setSortKey(key);
        setSortDirection("desc");
    };

    const exportData = () => {
        const payload = {
            generatedAt: new Date().toISOString(),
            window,
            query,
            sort: { key: sortKey, direction: sortDirection },
            totals: stats.totals,
            graphSeries,
            providers: sortedProviderRows,
            sessions: sortedSessionRows,
        };

        if (exportFormat === "json") {
            const blob = new Blob([JSON.stringify(payload, null, 2)], { type: "application/json" });
            downloadBlob(blob, `usage-${Date.now()}.json`);
            return;
        }

        const providerCsvRows = [
            ["provider", "model", "requests", "prompt_tokens", "completion_tokens", "total_tokens", "reasoning_tokens", "audio_tokens", "video_tokens", "cost", "avg_tps"],
            ...sortedProviderRows.map((row) => [
                row.provider,
                row.model,
                String(row.requests),
                String(row.promptTokens),
                String(row.completionTokens),
                String(row.totalTokens),
                String(row.reasoningTokens),
                String(row.audioTokens),
                String(row.videoTokens),
                row.cost.toFixed(6),
                row.avgTps.toFixed(3),
            ]),
        ];

        const sessionCsvRows = [
            ["thread_id", "title", "providers_models", "requests", "prompt_tokens", "completion_tokens", "total_tokens", "reasoning_tokens", "audio_tokens", "video_tokens", "cost", "avg_tps", "max_tps", "min_tps", "updated_at"],
            ...sortedSessionRows.map((row) => [
                row.threadId,
                row.title,
                row.providerModels,
                String(row.requests),
                String(row.promptTokens),
                String(row.completionTokens),
                String(row.totalTokens),
                String(row.reasoningTokens),
                String(row.audioTokens),
                String(row.videoTokens),
                row.cost.toFixed(6),
                row.avgTps.toFixed(3),
                row.maxTps.toFixed(3),
                row.minTps.toFixed(3),
                new Date(row.updatedAt).toISOString(),
            ]),
        ];

        const graphCsvRows = [["period", "tokens", "cost"], ...graphSeries.map((row) => [row.label, String(row.tokens), row.cost.toFixed(6)])];

        const csv = [
            "# summary",
            csvRow(["window", window]),
            csvRow(["total_sessions", String(stats.totals.sessions)]),
            csvRow(["total_requests", String(stats.totals.requests)]),
            csvRow(["total_tokens", String(stats.totals.totalTokens)]),
            csvRow(["total_cost", stats.totals.totalCost.toFixed(6)]),
            "",
            "# graph_series",
            ...graphCsvRows.map(csvRow),
            "",
            "# providers",
            ...providerCsvRows.map(csvRow),
            "",
            "# sessions",
            ...sessionCsvRows.map(csvRow),
        ].join("\n");

        const blob = new Blob([csv], { type: "text/csv;charset=utf-8" });
        downloadBlob(blob, `usage-${Date.now()}.csv`);
    };

    return (
        <div style={{ padding: "var(--space-4)", height: "100%", overflow: "auto" }}>
            <div style={{ display: "flex", gap: "var(--space-2)", marginBottom: "var(--space-3)", flexWrap: "wrap" }}>
                <ActionButton onClick={() => setMode("explorer")} disabled={mode === "explorer"}>Explorer</ActionButton>
                <ActionButton onClick={() => setMode("statistics")} disabled={mode === "statistics"}>Statistics</ActionButton>
            </div>

            {mode === "statistics" ? (
                <AgentStatisticsView />
            ) : (
                <>
            <MetricRibbon
                items={[
                    { label: "Sessions", value: String(stats.totals.sessions) },
                    { label: "Requests", value: String(stats.totals.requests) },
                    { label: "Total Tokens", value: stats.totals.totalTokens.toLocaleString() },
                    { label: "Total Cost", value: `$${stats.totals.totalCost.toFixed(6)}` },
                    { label: "Avg Session Cost", value: `$${stats.totals.avgCostPerSession.toFixed(6)}` },
                ]}
            />

            <MetricRibbon
                items={[
                    { label: "Prompt", value: stats.totals.promptTokens.toLocaleString() },
                    { label: "Completion", value: stats.totals.completionTokens.toLocaleString() },
                    { label: "Reasoning", value: stats.totals.reasoningTokens.toLocaleString() },
                    { label: "Audio", value: stats.totals.audioTokens.toLocaleString() },
                    { label: "Video", value: stats.totals.videoTokens.toLocaleString() },
                    { label: "TPS avg/max/min", value: `${stats.totals.avgTps.toFixed(1)} / ${stats.totals.maxTps.toFixed(1)} / ${stats.totals.minTps.toFixed(1)}` },
                ]}
            />

            <SectionTitle title="Usage Explorer" subtitle="Search and sort usage by provider/model and session" />
            <div style={{ display: "flex", gap: "var(--space-2)", marginBottom: "var(--space-3)", flexWrap: "wrap" }}>
                <input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Search provider, model, thread, id..." style={{ ...inputStyle, minWidth: 280 }} />
                <select value={window} onChange={(event) => setWindow(event.target.value as UsageWindow)} style={{ ...inputStyle, flex: "0 0 auto", minWidth: 120 }}>
                    <option value="today">Today</option>
                    <option value="7d">7 days</option>
                    <option value="30d">30 days</option>
                    <option value="all">All</option>
                </select>
                <label style={{ display: "flex", alignItems: "center", gap: 6, fontSize: "var(--text-sm)", color: "var(--text-secondary)" }}>
                    <input type="checkbox" checked={compactMode} onChange={(event) => setCompactMode(event.target.checked)} />
                    Compact
                </label>
                <select value={exportFormat} onChange={(event) => setExportFormat(event.target.value as ExportFormat)} style={{ ...inputStyle, flex: "0 0 auto", minWidth: 110 }}>
                    <option value="json">JSON</option>
                    <option value="csv">CSV</option>
                </select>
                <ActionButton onClick={exportData}>Export</ActionButton>
                <ActionButton onClick={() => updateSort("totalTokens")}>Tokens {sortKey === "totalTokens" ? (sortDirection === "desc" ? "▼" : "▲") : ""}</ActionButton>
                <ActionButton onClick={() => updateSort("promptTokens")}>Prompt {sortKey === "promptTokens" ? (sortDirection === "desc" ? "▼" : "▲") : ""}</ActionButton>
                <ActionButton onClick={() => updateSort("completionTokens")}>Completion {sortKey === "completionTokens" ? (sortDirection === "desc" ? "▼" : "▲") : ""}</ActionButton>
                <ActionButton onClick={() => updateSort("cost")}>Cost {sortKey === "cost" ? (sortDirection === "desc" ? "▼" : "▲") : ""}</ActionButton>
                <ActionButton onClick={() => updateSort("avgTps")}>Avg TPS {sortKey === "avgTps" ? (sortDirection === "desc" ? "▼" : "▲") : ""}</ActionButton>
                <ActionButton onClick={() => updateSort("requests")}>Requests {sortKey === "requests" ? (sortDirection === "desc" ? "▼" : "▲") : ""}</ActionButton>
            </div>

            <SectionTitle title="Usage Graphs" subtitle="Rolling daily, weekly, and monthly sums" />
            <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "var(--space-3)", marginBottom: "var(--space-4)" }}>
                <UsageBarGraph title="Token Usage" color="var(--accent)" rows={graphSeries.map((row) => ({ label: row.label, value: row.tokens, formatted: row.tokens.toLocaleString() }))} />
                <UsageBarGraph title="Cost Usage" color="var(--warning)" rows={graphSeries.map((row) => ({ label: row.label, value: row.cost, formatted: `$${row.cost.toFixed(6)}` }))} />
            </div>

            <SectionTitle title="By Provider / Model" subtitle="Detailed usage ranked by selected sort" />
            {sortedProviderRows.length === 0 ? <EmptyPanel message="No provider/model usage yet." /> : compactMode ? <UsageProviderTable rows={sortedProviderRows} /> : (
                <div style={{ display: "grid", gap: "var(--space-2)" }}>
                    {sortedProviderRows.map((row) => (
                        <div key={row.key} style={{ border: "1px solid var(--glass-border)", borderRadius: "var(--radius-lg)", padding: "var(--space-3)", background: "var(--bg-secondary)" }}>
                            <div style={{ display: "flex", justifyContent: "space-between", gap: "var(--space-2)", marginBottom: 6 }}>
                                <div style={{ fontSize: "var(--text-sm)", fontWeight: 700 }}>{row.provider}/{row.model}</div>
                                <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)" }}>{row.requests} req</div>
                            </div>
                            <div style={{ display: "flex", flexWrap: "wrap", gap: 10, fontSize: "var(--text-sm)", color: "var(--text-secondary)" }}>
                                <span>tok {row.totalTokens.toLocaleString()}</span>
                                <span>in {row.promptTokens.toLocaleString()}</span>
                                <span>out {row.completionTokens.toLocaleString()}</span>
                                <span>reasoning {row.reasoningTokens.toLocaleString()}</span>
                                <span>audio {row.audioTokens.toLocaleString()}</span>
                                <span>video {row.videoTokens.toLocaleString()}</span>
                                <span>cost ${row.cost.toFixed(6)}</span>
                                <span>avg tps {row.avgTps.toFixed(1)}</span>
                            </div>
                        </div>
                    ))}
                </div>
            )}

            <SectionTitle title="By Session (Thread)" subtitle="Session-level totals and performance" />
            {sortedSessionRows.length === 0 ? <EmptyPanel message="No session usage yet." /> : compactMode ? <UsageSessionTable rows={sortedSessionRows} /> : (
                <div style={{ display: "grid", gap: "var(--space-2)" }}>
                    {sortedSessionRows.map((row) => (
                        <div key={row.threadId} style={{ border: "1px solid var(--glass-border)", borderRadius: "var(--radius-lg)", padding: "var(--space-3)", background: "var(--bg-secondary)" }}>
                            <div style={{ display: "flex", justifyContent: "space-between", gap: "var(--space-2)", marginBottom: 6 }}>
                                <div style={{ fontSize: "var(--text-sm)", fontWeight: 700 }}>{row.title}</div>
                                <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)" }}>{new Date(row.updatedAt).toLocaleString()}</div>
                            </div>
                            <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)", marginBottom: 6 }}>{row.providerModels || "unknown model"}</div>
                            <div style={{ display: "flex", flexWrap: "wrap", gap: 10, fontSize: "var(--text-sm)", color: "var(--text-secondary)" }}>
                                <span>req {row.requests}</span>
                                <span>tok {row.totalTokens.toLocaleString()}</span>
                                <span>in {row.promptTokens.toLocaleString()}</span>
                                <span>out {row.completionTokens.toLocaleString()}</span>
                                <span>reasoning {row.reasoningTokens.toLocaleString()}</span>
                                <span>audio {row.audioTokens.toLocaleString()}</span>
                                <span>video {row.videoTokens.toLocaleString()}</span>
                                <span>cost ${row.cost.toFixed(6)}</span>
                                <span>avg/max/min tps {row.avgTps.toFixed(1)} / {row.maxTps.toFixed(1)} / {row.minTps.toFixed(1)}</span>
                            </div>
                        </div>
                    ))}
                </div>
            )}
                </>
            )}
        </div>
    );
}

function csvRow(values: string[]): string {
    return values.map((value) => `"${String(value).replace(/"/g, '""')}"`).join(",");
}

function downloadBlob(blob: Blob, fileName: string) {
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = fileName;
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
    URL.revokeObjectURL(url);
}

function UsageBarGraph({ title, color, rows }: { title: string; color: string; rows: Array<{ label: string; value: number; formatted: string }> }) {
    const maxValue = Math.max(...rows.map((row) => row.value), 1);
    return (
        <div style={{ border: "1px solid var(--glass-border)", borderRadius: "var(--radius-lg)", background: "var(--bg-secondary)", padding: "var(--space-3)" }}>
            <div style={{ fontSize: "var(--text-sm)", fontWeight: 700, marginBottom: "var(--space-2)" }}>{title}</div>
            <div style={{ display: "grid", gap: "var(--space-2)" }}>
                {rows.map((row) => (
                    <div key={row.label} style={{ display: "grid", gridTemplateColumns: "72px 1fr auto", alignItems: "center", gap: 8 }}>
                        <span style={{ fontSize: "var(--text-sm)", color: "var(--text-muted)" }}>{row.label}</span>
                        <div style={{ height: 12, borderRadius: 999, background: "color-mix(in srgb, var(--text-primary) 8%, transparent)", overflow: "hidden" }}>
                            <div style={{ height: "100%", width: `${Math.max(2, (row.value / maxValue) * 100)}%`, background: color, borderRadius: 999 }} />
                        </div>
                        <span style={{ fontSize: "var(--text-sm)", color: "var(--text-secondary)", minWidth: 70, textAlign: "right" }}>{row.formatted}</span>
                    </div>
                ))}
            </div>
        </div>
    );
}

function UsageProviderTable({ rows }: { rows: Array<{ key: string; provider: string; model: string; requests: number; promptTokens: number; completionTokens: number; totalTokens: number; reasoningTokens: number; audioTokens: number; videoTokens: number; cost: number; avgTps: number; }> }) {
    return (
        <div style={{ overflow: "auto", border: "1px solid var(--glass-border)", borderRadius: "var(--radius-lg)" }}>
            <table style={{ width: "100%", borderCollapse: "collapse", fontSize: "var(--text-sm)" }}>
                <thead><tr style={{ position: "sticky", top: 0, background: "var(--bg-tertiary)", zIndex: 1 }}><Th>Provider / Model</Th><Th>Req</Th><Th>Prompt</Th><Th>Completion</Th><Th>Total</Th><Th>Reasoning</Th><Th>Audio</Th><Th>Video</Th><Th>Cost</Th><Th>Avg TPS</Th></tr></thead>
                <tbody>{rows.map((row) => (<tr key={row.key} style={{ borderTop: "1px solid color-mix(in srgb, var(--text-primary) 8%, transparent)" }}><Td>{row.provider}/{row.model}</Td><Td>{row.requests}</Td><Td>{row.promptTokens.toLocaleString()}</Td><Td>{row.completionTokens.toLocaleString()}</Td><Td>{row.totalTokens.toLocaleString()}</Td><Td>{row.reasoningTokens.toLocaleString()}</Td><Td>{row.audioTokens.toLocaleString()}</Td><Td>{row.videoTokens.toLocaleString()}</Td><Td>${row.cost.toFixed(6)}</Td><Td>{row.avgTps.toFixed(1)}</Td></tr>))}</tbody>
            </table>
        </div>
    );
}

function UsageSessionTable({ rows }: { rows: Array<{ threadId: string; title: string; providerModels: string; requests: number; promptTokens: number; completionTokens: number; totalTokens: number; reasoningTokens: number; audioTokens: number; videoTokens: number; cost: number; avgTps: number; maxTps: number; minTps: number; updatedAt: number; }> }) {
    return (
        <div style={{ overflow: "auto", border: "1px solid var(--glass-border)", borderRadius: "var(--radius-lg)" }}>
            <table style={{ width: "100%", borderCollapse: "collapse", fontSize: "var(--text-sm)" }}>
                <thead><tr style={{ position: "sticky", top: 0, background: "var(--bg-tertiary)", zIndex: 1 }}><Th>Session</Th><Th>Providers</Th><Th>Req</Th><Th>Prompt</Th><Th>Completion</Th><Th>Total</Th><Th>Reasoning</Th><Th>Cost</Th><Th>Avg</Th><Th>Max</Th><Th>Min</Th><Th>Updated</Th></tr></thead>
                <tbody>{rows.map((row) => (<tr key={row.threadId} style={{ borderTop: "1px solid color-mix(in srgb, var(--text-primary) 8%, transparent)" }}><Td>{row.title}</Td><Td>{row.providerModels || "unknown"}</Td><Td>{row.requests}</Td><Td>{row.promptTokens.toLocaleString()}</Td><Td>{row.completionTokens.toLocaleString()}</Td><Td>{row.totalTokens.toLocaleString()}</Td><Td>{row.reasoningTokens.toLocaleString()}</Td><Td>${row.cost.toFixed(6)}</Td><Td>{row.avgTps.toFixed(1)}</Td><Td>{row.maxTps.toFixed(1)}</Td><Td>{row.minTps.toFixed(1)}</Td><Td>{new Date(row.updatedAt).toLocaleDateString()}</Td></tr>))}</tbody>
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
