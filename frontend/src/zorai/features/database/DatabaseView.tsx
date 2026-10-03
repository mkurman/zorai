import { useEffect, useMemo, useRef, useState, type KeyboardEvent, type MouseEvent } from "react";
import { RefreshButton } from "@/zorai/shell/RefreshButton";
import { LoadingPanel, LoadingState } from "@/components/LoadingState";
import { applyDatabaseSelectionDraftValue, buildDatabaseRowUpdates, databaseDraftKey, displayDatabaseValue, getDatabaseSelectedDraftKeys, getLastDatabasePageOffset, getNextDatabaseSort, isBlobPlaceholder, isDatabaseCellSelected, normalizeDatabasePageSize, sortDatabaseRowsForDisplay, type DatabaseCellCoordinate, type DatabaseCellSelection } from "./databaseModel";
import { executeDatabaseSql, listDatabaseTables, queryDatabaseRows, updateDatabaseRows } from "./databaseBridge";
import type { DatabaseSortState, DatabaseSqlResult, DatabaseTablePage, DatabaseTableSummary } from "./databaseTypes";

type DatabaseViewProps = {
  activeTable: string | null;
  onSelectTable: (tableName: string) => void;
};

type DatabaseRailProps = DatabaseViewProps;

export function DatabaseRail({ activeTable, onSelectTable }: DatabaseRailProps) {
  const [tables, setTables] = useState<DatabaseTableSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [refreshKey, setRefreshKey] = useState(0);

  useEffect(() => {
    const refreshTables = () => setRefreshKey((current) => current + 1);
    window.addEventListener("zorai-database-schema-changed", refreshTables);
    return () => window.removeEventListener("zorai-database-schema-changed", refreshTables);
  }, []);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    listDatabaseTables()
      .then((nextTables) => {
        if (cancelled) return;
        setTables(nextTables);
        if (!activeTable && nextTables[0]) onSelectTable(nextTables[0].name);
      })
      .catch(() => {
        if (!cancelled) setTables([]);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [activeTable, onSelectTable, refreshKey]);

  return (
    <div className="zorai-rail-stack">
      {loading && tables.length === 0 ? <ThreadListSkeletonFallback /> : null}
      {tables.map((table) => (
        <button
          type="button"
          key={table.name}
          className={[
            "zorai-rail-card",
            "zorai-rail-card--button",
            table.name === activeTable ? "zorai-rail-card--active" : "",
          ].filter(Boolean).join(" ")}
          onClick={() => onSelectTable(table.name)}
        >
          <strong>{table.name}</strong>
          <span>{table.rowCount ?? "-"} rows · {table.editable ? "editable" : table.tableType}</span>
        </button>
      ))}
      {!loading && tables.length === 0 ? <div className="zorai-empty">No database tables found.</div> : null}
    </div>
  );
}

function ThreadListSkeletonFallback() {
  return <div className="zorai-database-table-skeleton" role="status" aria-label="Loading database tables">
    {Array.from({ length: 5 }, (_, index) => <LoadingState key={index} variant="skeleton" lines={2} />)}
  </div>;
}

export function DatabaseView({ activeTable }: DatabaseViewProps) {
  const [page, setPage] = useState<DatabaseTablePage | null>(null);
  const [offset, setOffset] = useState(0);
  const [pageSize, setPageSize] = useState(100);
  const [drafts, setDrafts] = useState<Record<string, string>>({});
  const [loading, setLoading] = useState(false);
  const [status, setStatus] = useState<string | null>(null);
  const [sort, setSort] = useState<DatabaseSortState | null>(null);
  const [selection, setSelection] = useState<DatabaseCellSelection>(null);
  const [sql, setSql] = useState("");
  const [sqlRunning, setSqlRunning] = useState(false);
  const [sqlResult, setSqlResult] = useState<DatabaseSqlResult | null>(null);
  const [sqlError, setSqlError] = useState<string | null>(null);
  const draggingSelectionRef = useRef(false);
  const bulkEditValueRef = useRef<string | null>(null);
  const sqlRunIdRef = useRef(0);

  useEffect(() => {
    setOffset(0);
    setDrafts({});
    setSort(null);
    setSelection(null);
  }, [activeTable]);

  useEffect(() => {
    const finishDrag = () => {
      draggingSelectionRef.current = false;
    };
    window.addEventListener("mouseup", finishDrag);
    return () => window.removeEventListener("mouseup", finishDrag);
  }, []);

  useEffect(() => {
    if (!selection) bulkEditValueRef.current = null;
  }, [selection]);

  useEffect(() => {
    if (!activeTable) return;
    let cancelled = false;
    setLoading(true);
    setStatus(null);
    queryDatabaseRows(activeTable, offset, pageSize, sort)
      .then((nextPage) => {
        if (!cancelled) {
          setPage(nextPage);
          setSelection(null);
        }
      })
      .catch((error) => {
        if (!cancelled) {
          setPage(null);
          setStatus(error?.message || "Database table could not be loaded.");
        }
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [activeTable, offset, pageSize, sort]);

  const updates = useMemo(() => buildDatabaseRowUpdates(page, drafts), [page, drafts]);
  const displayRows = useMemo(() => sortDatabaseRowsForDisplay(page, sort), [page, sort]);
  const displayColumns = useMemo(() => page?.columns ?? [], [page?.columns]);
  const selectedDraftKeys = useMemo(
    () => getDatabaseSelectedDraftKeys(page, displayRows, displayColumns, selection),
    [page, displayRows, displayColumns, selection],
  );
  const tableEditable = page?.editable ?? false;
  const dirtyCount = updates.reduce((total, update) => total + Object.keys(update.values).length, 0);
  const canGoPrevious = offset > 0;
  const canGoNext = page ? offset + page.limit < page.totalRows : false;
  const lastOffset = page ? getLastDatabasePageOffset(page.totalRows, pageSize) : 0;

  const refreshVisibleRows = async () => {
    if (!activeTable) return;
    setLoading(true);
    setStatus(null);
    setDrafts({});
    try {
      const nextPage = await queryDatabaseRows(activeTable, offset, pageSize, sort);
      setPage(nextPage);
      setSelection(null);
      setStatus("Refreshed visible rows.");
    } catch (error: any) {
      setPage(null);
      setStatus(error?.message || "Refresh failed.");
    } finally {
      setLoading(false);
    }
  };

  const pushChanges = async () => {
    if (!activeTable || updates.length === 0) return;
    setLoading(true);
    setStatus(null);
    try {
      const updatedRows = await updateDatabaseRows(activeTable, updates);
      setDrafts({});
      const nextPage = await queryDatabaseRows(activeTable, offset, pageSize, sort);
      setPage(nextPage);
      setSelection(null);
      setStatus(`Pushed ${updatedRows} row${updatedRows === 1 ? "" : "s"}.`);
    } catch (error: any) {
      setStatus(error?.message || "Push failed.");
    } finally {
      setLoading(false);
    }
  };

  const beginCellSelection = (
    coordinate: DatabaseCellCoordinate,
    editable: boolean,
    event: MouseEvent<HTMLTableCellElement>,
  ) => {
    if (event.button !== 0 || loading) return;
    if (!editable) {
      setSelection(null);
      bulkEditValueRef.current = null;
      return;
    }
    event.preventDefault();
    draggingSelectionRef.current = true;
    bulkEditValueRef.current = null;
    setSelection({ anchor: coordinate, focus: coordinate });
    event.currentTarget.querySelector<HTMLTextAreaElement>("textarea")?.focus();
  };

  const extendCellSelection = (coordinate: DatabaseCellCoordinate) => {
    if (!draggingSelectionRef.current) return;
    setSelection((current) => current ? { ...current, focus: coordinate } : null);
  };

  const applySelectedDraftValue = (nextValue: string) => {
    setDrafts((current) => applyDatabaseSelectionDraftValue(
      page,
      displayRows,
      displayColumns,
      selection,
      current,
      nextValue,
    ));
  };

  const handleSelectedKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (selectedDraftKeys.length <= 1 || event.ctrlKey || event.metaKey || event.altKey) return;
    if (event.key === "Backspace" || event.key === "Delete") {
      event.preventDefault();
      bulkEditValueRef.current = "";
      applySelectedDraftValue("");
      return;
    }
    if (event.key.length === 1) {
      event.preventDefault();
      const nextValue = `${bulkEditValueRef.current ?? ""}${event.key}`;
      bulkEditValueRef.current = nextValue;
      applySelectedDraftValue(nextValue);
    }
  };

  const runSql = async () => {
    const query = sql.trim();
    if (!query) {
      setSqlError("SQL query is empty.");
      setSqlResult(null);
      return;
    }
    const runId = sqlRunIdRef.current + 1;
    sqlRunIdRef.current = runId;
    setSqlRunning(true);
    setSqlError(null);
    setSqlResult(null);
    try {
      const result = await executeDatabaseSql(query);
      if (sqlRunIdRef.current !== runId) return;
      setSqlResult(result);
      if (result.columns.length === 0 && activeTable) {
        setDrafts({});
        setSelection(null);
        const nextPage = await queryDatabaseRows(activeTable, offset, pageSize, sort);
        if (sqlRunIdRef.current === runId) setPage(nextPage);
      }
      if (result.columns.length === 0) {
        window.dispatchEvent(new Event("zorai-database-schema-changed"));
      }
    } catch (error: any) {
      if (sqlRunIdRef.current === runId) {
        setSqlError(error?.message || "SQL execution failed.");
      }
    } finally {
      if (sqlRunIdRef.current === runId) setSqlRunning(false);
    }
  };

  const stopSql = () => {
    sqlRunIdRef.current += 1;
    setSqlRunning(false);
    setSqlError("Stopped waiting for SQL result. SQLite may still finish the statement in the background.");
  };

  if (!activeTable) {
    return (
      <section className="zorai-database-surface">
        <DatabaseSqlConsole
          sql={sql}
          running={sqlRunning}
          result={sqlResult}
          error={sqlError}
          onSqlChange={setSql}
          onRun={runSql}
          onStop={stopSql}
        />
        <div className="zorai-empty-main"><h1>Database</h1><p>Select a table to inspect rows.</p></div>
      </section>
    );
  }

  return (
    <section className="zorai-database-surface">
      <div className="zorai-database-toolbar">
        <div>
          <div className="zorai-kicker">SQLite Table</div>
          <h2>{activeTable}</h2>
        </div>
        <div className="zorai-database-controls">
          <button type="button" className="zorai-primary-button" onClick={pushChanges} disabled={loading || dirtyCount === 0}>
            Push{dirtyCount > 0 ? ` ${dirtyCount}` : ""}
          </button>
          <RefreshButton disabled={loading || !page} busy={loading} onClick={refreshVisibleRows} />
          <button type="button" className="zorai-ghost-button" disabled={!canGoPrevious || loading} onClick={() => { setSelection(null); setOffset(0); }}>First</button>
          <button type="button" className="zorai-ghost-button" disabled={!canGoPrevious || loading} onClick={() => { setSelection(null); setOffset(Math.max(0, offset - pageSize)); }}>Prev</button>
          <span>{page ? `${offset + 1}-${Math.min(offset + page.limit, page.totalRows)} / ${page.totalRows}` : "0 / 0"}</span>
          <button type="button" className="zorai-ghost-button" disabled={!canGoNext || loading} onClick={() => { setSelection(null); setOffset(offset + pageSize); }}>Next</button>
          <button type="button" className="zorai-ghost-button" disabled={loading || !page || offset === lastOffset} onClick={() => { setSelection(null); setOffset(lastOffset); }}>Last</button>
          <label>
            <span>Rows</span>
            <input
              className="zorai-database-page-size"
              type="number"
              min={1}
              max={500}
              value={pageSize}
              onChange={(event) => {
                setPageSize(normalizeDatabasePageSize(Number(event.target.value)));
                setOffset(0);
                setSelection(null);
              }}
            />
          </label>
        </div>
      </div>

      {status ? <div className="zorai-inline-note">{status}</div> : null}
      <DatabaseSqlConsole
        sql={sql}
        running={sqlRunning}
        result={sqlResult}
        error={sqlError}
        onSqlChange={setSql}
        onRun={runSql}
        onStop={stopSql}
      />
      <div className="zorai-database-table-wrap">
        <table className="zorai-database-table">
          <thead>
            <tr>
              <th>rowid</th>
              {displayColumns.map((column) => (
                <th key={column.name}>
                  <button
                    type="button"
                    className="zorai-database-sort-button"
                    onClick={() => {
                      setSort((current) => getNextDatabaseSort(current, column.name));
                      setOffset(0);
                      setDrafts({});
                      setSelection(null);
                    }}
                    title={`Sort by ${column.name}`}
                  >
                    <span>{column.name}</span>
                    <span className="zorai-database-sort-icon" aria-hidden="true">
                      {sort?.column === column.name ? (sort.direction === "desc" ? "⌄" : "⌃") : ""}
                    </span>
                  </button>
                  <small>{column.primaryKey ? "PK" : column.declaredType || "value"}</small>
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {displayRows.map((row, rowIndex) => (
              <tr key={row.rowid ?? rowIndex}>
                <td className="zorai-database-rowid">{row.rowid ?? "-"}</td>
                {displayColumns.map((column, columnIndex) => {
                  const originalValue = row.values[column.name];
                  const editable = tableEditable && typeof row.rowid === "number" && column.editable && !isBlobPlaceholder(originalValue);
                  const key = typeof row.rowid === "number" ? databaseDraftKey(row.rowid, column.name) : `${rowIndex}:${column.name}`;
                  const isDirty = Object.prototype.hasOwnProperty.call(drafts, key);
                  const value = isDirty ? drafts[key] : displayDatabaseValue(originalValue);
                  const coordinate = { rowIndex, columnIndex };
                  const isSelected = isDatabaseCellSelected(selection, rowIndex, columnIndex);
                  return (
                    <DatabaseCellEditor
                      key={column.name}
                      editable={editable}
                      loading={loading}
                      isDirty={isDirty}
                      isSelected={isSelected}
                      value={value}
                      onChange={(nextValue) => setDrafts((current) => ({ ...current, [key]: nextValue }))}
                      onKeyDown={handleSelectedKeyDown}
                      onSelectionStart={(event) => beginCellSelection(coordinate, editable, event)}
                      onSelectionEnter={() => extendCellSelection(coordinate)}
                    />
                  );
                })}
              </tr>
            ))}
          </tbody>
        </table>
        {!loading && page?.rows.length === 0 ? <div className="zorai-empty">No rows on this page.</div> : null}
        {loading && !page ? <LoadingPanel label="Loading database rows…" /> : null}
      </div>
    </section>
  );
}

type DatabaseSqlConsoleProps = {
  sql: string;
  running: boolean;
  result: DatabaseSqlResult | null;
  error: string | null;
  onSqlChange: (sql: string) => void;
  onRun: () => void;
  onStop: () => void;
};

export function DatabaseSqlConsole({
  sql,
  running,
  result,
  error,
  onSqlChange,
  onRun,
  onStop,
}: DatabaseSqlConsoleProps) {
  const rows = Math.min(14, Math.max(3, sql.split("\n").length));
  return (
    <section className="zorai-database-sql-console">
      <div className="zorai-database-sql-editor-row">
        <textarea
          className="zorai-database-sql-textarea"
          value={sql}
          rows={rows}
          spellCheck={false}
          placeholder="SQL"
          onChange={(event) => onSqlChange(event.target.value)}
        />
        <button
          type="button"
          className={running ? "zorai-sql-run-button zorai-danger-button" : "zorai-sql-run-button zorai-primary-button"}
          onClick={running ? onStop : onRun}
        >
          {running ? "Stop" : "Run"}
        </button>
      </div>
      {error ? <div className="zorai-database-sql-error">{error}</div> : null}
      {result ? <DatabaseSqlResultView result={result} /> : null}
    </section>
  );
}

function DatabaseSqlResultView({ result }: { result: DatabaseSqlResult }) {
  if (result.columns.length === 0) {
    return <div className="zorai-database-sql-summary">{result.message || `${result.rowsAffected} rows affected.`}</div>;
  }
  return (
    <div className="zorai-database-sql-result-wrap">
      <table className="zorai-database-sql-result">
        <thead>
          <tr>
            {result.columns.map((column) => <th key={column}>{column}</th>)}
          </tr>
        </thead>
        <tbody>
          {result.rows.map((row, rowIndex) => (
            <tr key={rowIndex}>
              {result.columns.map((column) => (
                <td key={column}>{displayDatabaseValue(row[column])}</td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
      <div className="zorai-database-sql-summary">{result.message}</div>
    </div>
  );
}

type DatabaseCellEditorProps = {
  editable: boolean;
  loading: boolean;
  isDirty: boolean;
  isSelected: boolean;
  value: string;
  onChange: (value: string) => void;
  onKeyDown?: (event: KeyboardEvent<HTMLTextAreaElement>) => void;
  onSelectionStart?: (event: MouseEvent<HTMLTableCellElement>) => void;
  onSelectionEnter?: () => void;
};

export function DatabaseCellEditor({
  editable,
  loading,
  isDirty,
  isSelected,
  value,
  onChange,
  onKeyDown,
  onSelectionStart,
  onSelectionEnter,
}: DatabaseCellEditorProps) {
  return (
    <td
      className={[
        isDirty ? "zorai-database-cell--dirty" : "",
        isSelected ? "zorai-database-cell--selected" : "",
      ].filter(Boolean).join(" ")}
      aria-selected={isSelected}
      onMouseDown={onSelectionStart}
      onMouseEnter={onSelectionEnter}
    >
      <textarea
        className="zorai-database-cell-editor"
        value={value}
        disabled={!editable || loading}
        rows={2}
        spellCheck={false}
        onChange={(event) => onChange(event.target.value)}
        onKeyDown={onKeyDown}
      />
    </td>
  );
}
