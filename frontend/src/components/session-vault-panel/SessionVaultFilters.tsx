import type { ReactNode } from "react";
import { filterInputStyle } from "./shared";

export function SessionVaultFilters({
    query,
    setQuery,
    workspaceFilter,
    setWorkspaceFilter,
    surfaceFilter,
    setSurfaceFilter,
    paneFilter,
    setPaneFilter,
    reasonFilter,
    setReasonFilter,
    dateFilter,
    setDateFilter,
    workspaceOptions,
    surfaceOptions,
    paneOptions,
    reasonOptions,
    children,
}: {
    query: string;
    setQuery: (value: string) => void;
    workspaceFilter: string;
    setWorkspaceFilter: (value: string) => void;
    surfaceFilter: string;
    setSurfaceFilter: (value: string) => void;
    paneFilter: string;
    setPaneFilter: (value: string) => void;
    reasonFilter: string;
    setReasonFilter: (value: string) => void;
    dateFilter: string;
    setDateFilter: (value: string) => void;
    workspaceOptions: Array<{ id: string; name: string }>;
    surfaceOptions: Array<{ id: string; name: string }>;
    paneOptions: string[];
    reasonOptions: string[];
    children?: ReactNode;
}) {
    return (
        <div style={{ display: "flex", flexWrap: "wrap", alignItems: "center", gap: 8, padding: "10px 12px", borderBottom: "1px solid var(--zorai-border)" }}>
            <input
                type="text"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder="Search transcripts..."
                autoFocus
                style={{ ...filterInputStyle, flex: "1 1 180px", minWidth: 140, maxWidth: "none" }}
            />
            <select value={workspaceFilter} onChange={(e) => setWorkspaceFilter(e.target.value)} style={filterInputStyle}>
                <option value="all">All workspaces</option>
                {workspaceOptions.map(({ id, name }) => <option key={id} value={id}>{name}</option>)}
            </select>
            <select value={surfaceFilter} onChange={(e) => setSurfaceFilter(e.target.value)} style={filterInputStyle}>
                <option value="all">All surfaces</option>
                {surfaceOptions.map(({ id, name }) => <option key={id} value={id}>{name}</option>)}
            </select>
            <select value={paneFilter} onChange={(e) => setPaneFilter(e.target.value)} style={filterInputStyle}>
                <option value="all">All panes</option>
                {paneOptions.map((id) => <option key={id} value={id}>{id}</option>)}
            </select>
            <select value={reasonFilter} onChange={(e) => setReasonFilter(e.target.value)} style={filterInputStyle}>
                <option value="all">All reasons</option>
                {reasonOptions.map((reason) => <option key={reason} value={reason}>{reason}</option>)}
            </select>
            <select value={dateFilter} onChange={(e) => setDateFilter(e.target.value)} style={filterInputStyle}>
                <option value="all">All dates</option>
                <option value="today">Today</option>
                <option value="7d">Last 7 days</option>
                <option value="30d">Last 30 days</option>
            </select>
            {children}
        </div>
    );
}