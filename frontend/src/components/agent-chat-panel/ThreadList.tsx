import { useEffect, useMemo, useState } from "react";
import type { AgentThread } from "../../lib/agentStore";
import { DEFAULT_PAGE_SIZE, iconButtonStyle, inputStyle, PageSizeSelect, PaginationControls } from "./shared";
import { RefreshButton } from "@/zorai/shell/RefreshButton";

export function ThreadListToolbar({
    searchQuery,
    onSearch,
    onRefresh,
    dateFilter,
    onDateFilterChange,
    pageSize,
    onPageSizeChange,
}: {
    searchQuery: string;
    onSearch: (query: string) => void;
    onRefresh: () => void;
    dateFilter: string;
    onDateFilterChange: (value: string) => void;
    pageSize: number;
    onPageSizeChange: (value: number) => void;
}) {
    return (
        <div style={{ marginBottom: "var(--space-3)", display: "flex", gap: "var(--space-3)", flexWrap: "wrap", alignItems: "center" }}>
            <input
                type="text"
                value={searchQuery}
                onChange={(e) => onSearch(e.target.value)}
                placeholder="Search threads..."
                style={{ ...inputStyle, minWidth: 220 }}
            />
            <input
                type="date"
                value={dateFilter}
                onChange={(event) => onDateFilterChange(event.target.value)}
                style={{ ...inputStyle, flex: "0 0 auto", minWidth: 170 }}
            />
            <PageSizeSelect value={pageSize} onChange={onPageSizeChange} />
            <RefreshButton onClick={onRefresh} />
        </div>
    );
}

export function ThreadList({
    threads,
    searchQuery,
    onSearch,
    onRefresh,
    onSelect,
    onDelete,
    activeThreadId,
}: {
    threads: AgentThread[];
    searchQuery: string;
    onSearch: (q: string) => void;
    onRefresh: () => void;
    onSelect: (t: AgentThread) => void;
    onDelete: (id: string) => void;
    activeThreadId?: string | null;
}) {
    const [pageSize, setPageSize] = useState(DEFAULT_PAGE_SIZE);
    const [page, setPage] = useState(1);
    const [dateFilter, setDateFilter] = useState("");

    const filteredThreads = useMemo(() => {
        return threads.filter((thread) => {
            if (!dateFilter) return true;
            const date = new Date(thread.updatedAt);
            const year = date.getFullYear();
            const month = String(date.getMonth() + 1).padStart(2, "0");
            const day = String(date.getDate()).padStart(2, "0");
            return `${year}-${month}-${day}` === dateFilter;
        });
    }, [dateFilter, threads]);

    useEffect(() => {
        setPage(1);
    }, [threads, searchQuery, dateFilter, pageSize]);

    const visibleThreads = useMemo(() => {
        const start = (page - 1) * pageSize;
        return filteredThreads.slice(start, start + pageSize);
    }, [filteredThreads, page, pageSize]);

    return (
        <div style={{ height: "100%", overflow: "auto", padding: "var(--space-3)" }}>
            <ThreadListToolbar
                searchQuery={searchQuery}
                onSearch={onSearch}
                onRefresh={onRefresh}
                dateFilter={dateFilter}
                onDateFilterChange={setDateFilter}
                pageSize={pageSize}
                onPageSizeChange={setPageSize}
            />

            {filteredThreads.length === 0 && (
                <div className="zorai-empty-state">
                    <div className="zorai-empty-state__icon">💬</div>
                    <div className="zorai-empty-state__title">No conversations yet</div>
                    <div className="zorai-empty-state__description">Create a new thread to start collaborating with the agent</div>
                </div>
            )}

            <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-2)" }}>
                {visibleThreads.map((t) => (
                    <div
                        key={t.id}
                        onClick={() => onSelect(t)}
                        className={t.id === activeThreadId ? "zorai-thread-item--active" : undefined}
                        style={{
                            padding: "var(--space-3)",
                            borderRadius: "var(--radius-lg)",
                            border: "1px solid var(--glass-border)",
                            background: "var(--bg-secondary)",
                            cursor: "pointer",
                            transition: "all var(--transition-fast)",
                        }}
                        onMouseEnter={(e) => {
                            e.currentTarget.style.borderColor = "var(--border-strong)";
                            e.currentTarget.style.background = "var(--bg-tertiary)";
                        }}
                        onMouseLeave={(e) => {
                            e.currentTarget.style.borderColor = "var(--glass-border)";
                            e.currentTarget.style.background = "var(--bg-secondary)";
                        }}
                    >
                        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "flex-start" }}>
                            <div style={{ flex: 1, minWidth: 0 }}>
                                <div
                                    style={{
                                        fontSize: "var(--text-sm)",
                                        fontWeight: 600,
                                        overflow: "hidden",
                                        textOverflow: "ellipsis",
                                        whiteSpace: "nowrap",
                                    }}
                                >
                                    {t.title}
                                </div>

                                {t.lastMessagePreview && (
                                    <div
                                        style={{
                                            fontSize: "var(--text-xs)",
                                            color: "var(--text-muted)",
                                            overflow: "hidden",
                                            textOverflow: "ellipsis",
                                            whiteSpace: "nowrap",
                                            marginTop: "var(--space-1)",
                                        }}
                                    >
                                        {t.lastMessagePreview}
                                    </div>
                                )}

                                <div
                                    style={{
                                        fontSize: "var(--text-xs)",
                                        color: "var(--text-muted)",
                                        marginTop: "var(--space-2)",
                                    }}
                                >
                                    {t.messageCount} msgs · {new Date(t.updatedAt).toLocaleDateString()}
                                </div>
                            </div>

                            <button
                                onClick={(e) => {
                                    e.stopPropagation();
                                    onDelete(t.id);
                                }}
                                style={{
                                    ...iconButtonStyle,
                                    color: "var(--danger)",
                                    flexShrink: 0,
                                }}
                                title="Delete thread"
                            >
                                ✕
                            </button>
                        </div>
                    </div>
                ))}
            </div>

            <div style={{ marginTop: "var(--space-3)" }}>
                <PaginationControls
                    page={page}
                    pageSize={pageSize}
                    totalItems={filteredThreads.length}
                    onPageChange={setPage}
                />
            </div>
        </div>
    );
}
