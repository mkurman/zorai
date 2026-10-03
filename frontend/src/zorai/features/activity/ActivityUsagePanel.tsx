import { LoadingPanel, LoadingState } from "@/components/LoadingState";
import { Children, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { getBridge } from "@/lib/bridge";
import {
  fillDailyRange,
  formatCount,
  formatDate,
  type UsageStats,
} from "./ActivityUsageStats";
import { mergeStatisticsSessionPage, statisticsSessionPageMatches } from "./activitySessionPage";
import { CostAreaChart, DailyTokenChart, ShareDonut, TotalsSplitBar, UsageBarList } from "./ActivityUsageCharts";
import { useAgentChatPanelRuntime } from "@/components/agent-chat-panel/runtime/context";
import { openThreadTarget } from "../threads/openThreadTarget";
import { navigateZorai } from "../../shell/zoraiNavigationEvents";

type UsageTab = "overview" | "providers" | "models" | "rankings";

const usageTabs: Array<{ id: UsageTab; label: string }> = [
  { id: "overview", label: "Overview" },
  { id: "providers", label: "Providers" },
  { id: "models", label: "Models" },
  { id: "rankings", label: "Rankings" },
];

const windows: Array<{ id: ZoraiStatisticsWindow; label: string }> = [
  { id: "today", label: "Today" },
  { id: "7d", label: "7d" },
  { id: "30d", label: "30d" },
  { id: "all", label: "All" },
];

const MAX_CHART_DAYS = 30;

export function UsagePanel({ stats }: { stats: UsageStats }) {
  const [tab, setTab] = useState<UsageTab>("overview");
  const [windowId, setWindowId] = useState<ZoraiStatisticsWindow>("all");
  const [snapshot, setSnapshot] = useState<ZoraiAgentStatisticsSnapshot | null>(null);
  const [sessionPageSize, setSessionPageSize] = useState(25);
  const [sessionPage, setSessionPage] = useState(0);
  const [statsLoading, setStatsLoading] = useState(false);
  const [sessionsLoading, setSessionsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [sessionsError, setSessionsError] = useState<string | null>(null);
  const sessionPageRef = useRef(sessionPage);
  const pageSizeRef = useRef(sessionPageSize);
  const appliedSessionRef = useRef<{ window: ZoraiStatisticsWindow; page: number; pageSize: number } | null>(null);
  const pendingSessionPageRef = useRef<ZoraiAgentStatisticsSnapshot | null>(null);
  const overviewOwnsFirstPage = useRef(false);
  sessionPageRef.current = sessionPage;
  pageSizeRef.current = sessionPageSize;

  useEffect(() => {
    const bridge = getBridge();
    if (!bridge?.agentGetStatistics) {
      setSnapshot(null);
      setError("Statistics bridge is unavailable.");
      return;
    }

    overviewOwnsFirstPage.current = true;
    appliedSessionRef.current = null;
    pendingSessionPageRef.current = null;
    let cancelled = false;
    const requestedPageSize = pageSizeRef.current;
    setStatsLoading(true);
    setError(null);
    void bridge.agentGetStatistics(windowId, requestedPageSize, 0, false).then((result) => {
      if (cancelled) return;
      const incoming = (result ?? null) as ZoraiAgentStatisticsSnapshot | null;
      if (!incoming) {
        setSnapshot(null);
        return;
      }
      setSnapshot((current) => {
        const applied = appliedSessionRef.current;
        const pending = pendingSessionPageRef.current;
        if (pending && applied?.window === incoming.window && statisticsSessionPageMatches(pending, applied.page * applied.pageSize, applied.pageSize)) {
          return mergeStatisticsSessionPage(incoming, pending);
        }
        if (current && applied?.window === incoming.window && (applied.page !== 0 || applied.pageSize !== incoming.session_limit)) {
          return mergeStatisticsSessionPage(incoming, current);
        }
        return incoming;
      });
    }).catch((fetchError) => {
      if (!cancelled) {
        setSnapshot(null);
        setError(fetchError?.message || "Statistics request failed.");
      }
    }).finally(() => {
      if (!cancelled) setStatsLoading(false);
    });

    return () => {
      cancelled = true;
    };
  }, [windowId]);

  useEffect(() => {
    if (overviewOwnsFirstPage.current && sessionPage === 0) {
      overviewOwnsFirstPage.current = false;
      return;
    }

    const bridge = getBridge();
    if (!bridge?.agentGetStatistics) return;

    let cancelled = false;
    const requestedPage = sessionPage;
    const requestedPageSize = sessionPageSize;
    const requestedOffset = requestedPage * requestedPageSize;
    setSessionsLoading(true);
    setSessionsError(null);
    void bridge.agentGetStatistics(windowId, requestedPageSize, requestedOffset, true).then((result) => {
      if (cancelled) return;
      const incoming = (result ?? null) as ZoraiAgentStatisticsSnapshot | null;
      if (!incoming || !statisticsSessionPageMatches(incoming, requestedOffset, requestedPageSize)) return;
      if (sessionPageRef.current !== requestedPage || pageSizeRef.current !== requestedPageSize) return;
      appliedSessionRef.current = { window: windowId, page: requestedPage, pageSize: requestedPageSize };
      pendingSessionPageRef.current = incoming;
      setSnapshot((current) => current ? mergeStatisticsSessionPage(current, incoming) : current);
    }).catch((fetchError) => {
      if (!cancelled) setSessionsError(fetchError?.message || "Session page request failed.");
    }).finally(() => {
      if (!cancelled) setSessionsLoading(false);
    });

    return () => {
      cancelled = true;
    };
  }, [sessionPage, sessionPageSize, windowId]);

  const dailyRows = useMemo(() => {
    if (!snapshot) return fillDailyRange(stats.dailyRows, MAX_CHART_DAYS);
    const historicalRows = (snapshot.daily ?? []).map((row) => ({
      dayStart: row.day_start,
      dayKey: row.day_key,
      inputTokens: row.input_tokens,
      outputTokens: row.output_tokens,
      totalTokens: row.total_tokens,
      cost: row.cost_usd,
      requests: row.request_count,
    }));
    return fillDailyRange(
      historicalRows,
      chartDaysForWindow(windowId, historicalRows.length),
      windowId === "all" ? undefined : Date.now(),
    );
  }, [snapshot, stats.dailyRows, windowId]);

  return (
    <div className="zorai-usage-stack">
      <div className="zorai-toolbar">
        {usageTabs.map((item) => (
          <button key={item.id} type="button" className={["zorai-ghost-button", tab === item.id ? "zorai-button--active" : ""].filter(Boolean).join(" ")} onClick={() => setTab(item.id)}>
            {item.label}
          </button>
        ))}
        <span className="zorai-inline-note">Window</span>
        {windows.map((item) => (
          <button key={item.id} type="button" className={["zorai-ghost-button", windowId === item.id ? "zorai-button--active" : ""].filter(Boolean).join(" ")} onClick={() => { setSessionPage(0); setWindowId(item.id); }}>
            {item.label}
          </button>
        ))}
      </div>

      {statsLoading && !snapshot ? <LoadingPanel label="Loading historical statistics…" /> : null}
      {statsLoading && snapshot ? <LoadingState size={14} label="Refreshing statistics" /> : null}
      {error ? <div className="zorai-empty-state">{error} Local loaded-message total: {formatCount(stats.totals.totalTokens)} tok.</div> : null}
      {snapshot ? <StatisticsBody snapshot={snapshot} tab={tab} /> : null}
      {tab === "overview" ? <DailyTrendsPanel snapshot={snapshot} stats={stats} dailyRows={dailyRows} /> : null}
      <SessionUsageTable
        rows={snapshot?.sessions ?? []}
        total={snapshot?.session_total ?? 0}
        page={sessionPage}
        pageSize={sessionPageSize}
        onPageChange={setSessionPage}
        onPageSizeChange={(size) => { setSessionPage(0); setSessionPageSize(size); }}
        loading={sessionsLoading}
        error={sessionsError}
      />
    </div>
  );
}

function DailyTrendsPanel({ snapshot, stats, dailyRows }: { snapshot: ZoraiAgentStatisticsSnapshot | null; stats: UsageStats; dailyRows: ReturnType<typeof fillDailyRange> }) {
  const totals = snapshot ? {
    promptTokens: snapshot.totals.input_tokens,
    completionTokens: snapshot.totals.output_tokens,
    requests: (snapshot.daily ?? []).reduce((sum, row) => sum + row.request_count, 0),
    sessions: snapshot.session_total,
    avgTps: 0,
  } : stats.totals;
  return (
    <div className="zorai-usage-grid">
      <div className="zorai-panel zorai-usage-panel--wide">
        <DailyTokenChart rows={dailyRows} />
      </div>
      <div className="zorai-panel">
        <div className="zorai-section-label">Cost Trend</div>
        <CostAreaChart rows={dailyRows} height={88} />
      </div>
      <div className="zorai-panel">
        <div className="zorai-section-label">Token Split</div>
        <TotalsSplitBar inputTokens={totals.promptTokens} outputTokens={totals.completionTokens} />
        <p className="zorai-empty-state">
          Across {formatCount(totals.requests)} requests in {formatCount(totals.sessions)} sessions{totals.avgTps > 0 ? ` · avg ${totals.avgTps.toFixed(1)} tok/s` : ""}.
        </p>
      </div>
    </div>
  );
}

function StatisticsBody({ snapshot, tab }: { snapshot: ZoraiAgentStatisticsSnapshot; tab: UsageTab }) {
  if (tab === "providers") return <ProviderTable rows={snapshot.providers} />;
  if (tab === "models") return <ModelTable rows={snapshot.models} />;
  if (tab === "rankings") return <Rankings snapshot={snapshot} />;

  const models = [...snapshot.models].sort((a, b) => b.total_tokens - a.total_tokens);

  return (
    <div className="zorai-usage-grid">
      <div className="zorai-panel zorai-usage-panel--wide">
        <div className="zorai-section-label">Totals</div>
        <div className="zorai-metric-grid">
          <UsageMetric label="Input tokens" value={`${formatTokenValue(snapshot.totals.input_tokens)} tok`} />
          <UsageMetric label="Output tokens" value={`${formatTokenValue(snapshot.totals.output_tokens)} tok`} />
          <UsageMetric label="Total tokens" value={`${formatTokenValue(snapshot.totals.total_tokens)} tok`} />
          <UsageMetric label="Total cost" value={formatCost(snapshot.totals.cost_usd)} />
          <UsageMetric label="Providers" value={String(snapshot.totals.provider_count)} />
          <UsageMetric label="Models" value={String(snapshot.totals.model_count)} />
        </div>
        <p className="zorai-empty-state">Generated at: {formatGeneratedAt(snapshot.generated_at)}</p>
        {snapshot.has_incomplete_cost_history ? (
          <p className="zorai-empty-state">Warning: historical cost is incomplete for this window. Older rows without stored cost are counted as $0.</p>
        ) : null}
      </div>
      <div className="zorai-panel">
        <div className="zorai-section-label">Token Share By Model</div>
        <ShareDonut rows={models.map(toUsageRow)} metric="totalTokens" emptyText="No model share for this window." />
      </div>
      <div className="zorai-panel">
        <div className="zorai-section-label">Cost Share By Model</div>
        <ShareDonut rows={[...snapshot.models].sort((a, b) => b.cost_usd - a.cost_usd).map(toUsageRow)} metric="cost" emptyText="No cost share for this window." />
      </div>
    </div>
  );
}

function toUsageRow(row: ZoraiModelStatisticsRow) {
  return {
    key: `${row.provider}/${row.model}`,
    provider: row.provider,
    model: row.model,
    requests: 0,
    promptTokens: 0,
    completionTokens: 0,
    totalTokens: row.total_tokens,
    reasoningTokens: 0,
    audioTokens: 0,
    videoTokens: 0,
    cost: row.cost_usd,
    avgTps: 0,
    tpsSamples: 0,
  };
}

function ProviderTable({ rows }: { rows: ZoraiProviderStatisticsRow[] }) {
  const total = rows.reduce((acc, row) => acc + row.total_tokens, 0);
  return (
    <div className="zorai-usage-stack">
      <div className="zorai-panel zorai-usage-panel--wide">
        <div className="zorai-section-label">Provider Token Share</div>
        <UsageBarList
          rows={rows.map((row) => ({
            key: row.provider,
            provider: row.provider,
            model: "",
            requests: 0,
            promptTokens: 0,
            completionTokens: 0,
            totalTokens: row.total_tokens,
            reasoningTokens: 0,
            audioTokens: 0,
            videoTokens: 0,
            cost: row.cost_usd,
            avgTps: 0,
            tpsSamples: 0,
          }))}
          metric="totalTokens"
          emptyText="No provider statistics for this window."
        />
        {rows.length > 0 ? <p className="zorai-empty-state">{rows.length} providers · {formatTokenValue(total)} tok total</p> : null}
      </div>
      <UsageTable title="Providers" columns={["Provider", "In", "Out", "Total", "Cost"]} empty="No provider statistics for this window.">
        {rows.map((row) => (
          <tr key={row.provider}><td>{row.provider}</td><td>{formatTokenValue(row.input_tokens)} tok</td><td>{formatTokenValue(row.output_tokens)} tok</td><td>{formatTokenValue(row.total_tokens)} tok</td><td>{formatCost(row.cost_usd)}</td></tr>
        ))}
      </UsageTable>
    </div>
  );
}

function ModelTable({ rows }: { rows: ZoraiModelStatisticsRow[] }) {
  return (
    <div className="zorai-usage-stack">
      <div className="zorai-panel zorai-usage-panel--wide">
        <div className="zorai-section-label">Model Cost Ranking</div>
        <UsageBarList
          rows={[...rows].sort((a, b) => b.cost_usd - a.cost_usd).map(toUsageRow)}
          metric="cost"
          emptyText="No model statistics for this window."
        />
      </div>
      <UsageTable title="Provider / Model" columns={["Provider / Model", "In", "Out", "Total", "Cost"]} empty="No model statistics for this window.">
        {rows.map((row) => (
          <tr key={`${row.provider}/${row.model}`}><td>{row.provider} / {row.model}</td><td>{formatTokenValue(row.input_tokens)} tok</td><td>{formatTokenValue(row.output_tokens)} tok</td><td>{formatTokenValue(row.total_tokens)} tok</td><td>{formatCost(row.cost_usd)}</td></tr>
        ))}
      </UsageTable>
    </div>
  );
}

function Rankings({ snapshot }: { snapshot: ZoraiAgentStatisticsSnapshot }) {
  const byTokens = [...snapshot.models].sort((a, b) => b.total_tokens - a.total_tokens).map(toUsageRow);
  const byCost = [...snapshot.models].sort((a, b) => b.cost_usd - a.cost_usd).map(toUsageRow);
  return (
    <div className="zorai-usage-grid">
      <div className="zorai-panel">
        <div className="zorai-section-label">Top Models By Tokens</div>
        <UsageBarList rows={byTokens} metric="totalTokens" emptyText="No rankings for this window." />
      </div>
      <div className="zorai-panel">
        <div className="zorai-section-label">Top Models By Cost</div>
        <UsageBarList rows={byCost} metric="cost" emptyText="No rankings for this window." />
      </div>
    </div>
  );
}

function SessionUsageTable({
  rows,
  total,
  page,
  pageSize,
  onPageChange,
  onPageSizeChange,
  loading,
  error,
}: {
  rows: ZoraiSessionStatisticsRow[];
  total: number;
  page: number;
  pageSize: number;
  onPageChange: (page: number) => void;
  onPageSizeChange: (size: number) => void;
  loading: boolean;
  error: string | null;
}) {
  const runtime = useAgentChatPanelRuntime();
  const pageCount = Math.max(1, Math.ceil(total / pageSize));
  const openSession = async (threadId: string) => {
    const opened = await openThreadTarget(runtime, threadId);
    if (opened) navigateZorai({ view: "threads" });
  };
  return (
    <div className="zorai-panel zorai-usage-panel--wide">
      <div className="zorai-section-label">
        <span>Sessions <span className="zorai-inline-note">· {formatCount(total)} historical</span></span>
        <label className="zorai-usage-page-size">Per page
          <select value={pageSize} onChange={(event) => onPageSizeChange(Number(event.target.value))}>
            {[10, 25, 50, 100].map((size) => <option key={size} value={size}>{size}</option>)}
          </select>
        </label>
      </div>
      <div className="zorai-usage-table-wrap">
        <table className="zorai-usage-table">
          <thead><tr>{["Thread", "Provider models", "Req", "Input", "Output", "Total", "Cost", "Updated", "Open"].map((column) => <th key={column}>{column}</th>)}</tr></thead>
          <tbody>
            {rows.length === 0 ? <tr><td colSpan={9}>No historical session usage exists for this window.</td></tr> : rows.map((row) => (
              <tr key={row.thread_id}>
                <td>{row.title}</td>
                <td>{row.provider_models.join(", ") || "unknown"}</td>
                <td>{formatCount(row.request_count)}</td>
                <td>{formatCount(row.input_tokens)}</td>
                <td>{formatCount(row.output_tokens)}</td>
                <td>{formatCount(row.total_tokens)}</td>
                <td>{formatCost(row.cost_usd)}</td>
                <td>{formatDate(row.updated_at)}</td>
                <td><button type="button" className="zorai-ghost-button" onClick={() => void openSession(row.thread_id)}>Open in Threads</button></td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {error ? <p className="zorai-empty-state">{error}</p> : null}
      <div className="zorai-usage-pagination" aria-label="Session usage pages">
        {loading ? <LoadingState size={14} label="Refreshing sessions" /> : null}
        <button type="button" className="zorai-ghost-button" disabled={page <= 0} onClick={() => onPageChange(page - 1)}>Previous</button>
        <span>Page {Math.min(page + 1, pageCount)} of {pageCount}</span>
        <button type="button" className="zorai-ghost-button" disabled={page + 1 >= pageCount} onClick={() => onPageChange(page + 1)}>Next</button>
      </div>
    </div>
  );
}

function UsageTable({ title, columns, empty, children }: { title: string; columns: string[]; empty: string; children: ReactNode }) {
  const hasRows = Children.count(children) > 0;

  return (
    <div className="zorai-panel zorai-usage-panel--wide">
      <div className="zorai-section-label">{title}</div>
      <div className="zorai-usage-table-wrap">
        <table className="zorai-usage-table">
          <thead><tr>{columns.map((column) => <th key={column}>{column}</th>)}</tr></thead>
          <tbody>{hasRows ? children : <tr><td colSpan={columns.length}>{empty}</td></tr>}</tbody>
        </table>
      </div>
    </div>
  );
}

function UsageMetric({ label, value }: { label: string; value: string }) {
  return <div className="zorai-metric-card"><strong>{value}</strong><span>{label}</span></div>;
}

function chartDaysForWindow(windowId: ZoraiStatisticsWindow, availableDays: number): number {
  if (windowId === "today") return 1;
  if (windowId === "7d") return 7;
  if (windowId === "30d") return 30;
  return Math.max(1, Math.min(MAX_CHART_DAYS, availableDays));
}

function formatCost(value: number): string {
  return `$${Number(value || 0).toFixed(6)}`;
}

function formatGeneratedAt(value: number): string {
  return Number.isFinite(value) ? new Date(value).toLocaleString() : "unknown";
}

function formatTokenValue(tokens: number): string {
  const rounded = Math.max(0, Math.round(tokens || 0));
  if (rounded < 1000) return String(rounded);
  const units = ["", "k", "M", "B", "T", "P"];
  let value = rounded;
  let unit = 0;
  while (value >= 999_995 && unit + 1 < units.length) {
    value /= 1000;
    unit += 1;
  }
  return `${(value / 1000).toFixed(2)}${units[unit + 1] ?? ""}`;
}