import { filterInputStyle } from "./shared";

const selectStyle = { ...filterInputStyle, flex: "none", minWidth: 120, maxWidth: 200 } as const;

export function CommandLogFilters({
    query,
    setQuery,
    workspaceFilter,
    setWorkspaceFilter,
    surfaceFilter,
    setSurfaceFilter,
    paneFilter,
    setPaneFilter,
    statusFilter,
    setStatusFilter,
    dateFilter,
    setDateFilter,
    workspaceOptions,
    surfaceOptions,
    paneOptions,
    exportVisible,
    clearAll,
}: {
    query: string;
    setQuery: (value: string) => void;
    workspaceFilter: string;
    setWorkspaceFilter: (value: string) => void;
    surfaceFilter: string;
    setSurfaceFilter: (value: string) => void;
    paneFilter: string;
    setPaneFilter: (value: string) => void;
    statusFilter: string;
    setStatusFilter: (value: string) => void;
    dateFilter: string;
    setDateFilter: (value: string) => void;
    workspaceOptions: Array<{ id: string; name: string }>;
    surfaceOptions: Array<{ id: string; name: string }>;
    paneOptions: string[];
    exportVisible: () => void;
    clearAll: () => void;
}) {
    return (
        <div style={{ display: "flex", flexWrap: "wrap", gap: 8, padding: "10px 12px", borderBottom: "1px solid var(--zorai-border)", alignItems: "center" }}>
            <input
                type="text"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder="Filter commands..."
                autoFocus
                style={{ ...filterInputStyle, flex: "1 1 220px", minWidth: 160 }}
            />
            <select value={workspaceFilter} onChange={(e) => setWorkspaceFilter(e.target.value)} style={selectStyle}>
                <option value="all">All workspaces</option>
                {workspaceOptions.map(({ id, name }) => <option key={id} value={id}>{name}</option>)}
            </select>
            <select value={surfaceFilter} onChange={(e) => setSurfaceFilter(e.target.value)} style={selectStyle}>
                <option value="all">All surfaces</option>
                {surfaceOptions.map(({ id, name }) => <option key={id} value={id}>{name}</option>)}
            </select>
            <select value={paneFilter} onChange={(e) => setPaneFilter(e.target.value)} style={selectStyle}>
                <option value="all">All panes</option>
                {paneOptions.map((id) => <option key={id} value={id}>{id}</option>)}
            </select>
            <select value={statusFilter} onChange={(e) => setStatusFilter(e.target.value)} style={selectStyle}>
                <option value="all">Any status</option>
                <option value="running">Running</option>
                <option value="success">Success</option>
                <option value="failed">Failed</option>
            </select>
            <select value={dateFilter} onChange={(e) => setDateFilter(e.target.value)} style={selectStyle}>
                <option value="all">All dates</option>
                <option value="today">Today</option>
                <option value="7d">Last 7 days</option>
                <option value="30d">Last 30 days</option>
            </select>
            <button type="button" className="zorai-ghost-button zorai-ghost-button--compact" onClick={exportVisible} title="Export visible entries">
                Export
            </button>
            <button type="button" className="zorai-ghost-button zorai-ghost-button--compact" onClick={clearAll} title="Clear all">
                Purge
            </button>
        </div>
    );
}