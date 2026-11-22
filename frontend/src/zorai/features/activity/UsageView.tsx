import { useMemo } from "react";
import { useAgentChatPanelRuntime } from "@/components/agent-chat-panel/runtime/context";
import { UsagePanel } from "./ActivityUsagePanel";
import { buildUsageStats, formatCost, formatCount } from "./ActivityUsageStats";

function useUsageStats() {
  const runtime = useAgentChatPanelRuntime();
  return useMemo(
    () => buildUsageStats(runtime.threads, runtime.allMessagesByThread, runtime.goalRunsForTrace),
    [runtime.allMessagesByThread, runtime.goalRunsForTrace, runtime.threads],
  );
}

export function UsageRail() {
  const stats = useUsageStats();
  const rows: Array<[string, string]> = [
    ["Total tokens", formatCount(stats.totals.totalTokens)],
    ["Requests", formatCount(stats.totals.requests)],
    ["Sessions", formatCount(stats.totals.sessions)],
    ["Cost", formatCost(stats.totals.cost)],
  ];
  return (
    <div className="zorai-rail-stack">
      {rows.map(([label, value]) => (
        <div key={label} className="zorai-rail-card">
          <strong className="zorai-rail-card__title">{value}</strong>
          <span className="zorai-rail-card__status">{label}</span>
        </div>
      ))}
    </div>
  );
}

export function UsageView() {
  const stats = useUsageStats();
  return (
    <section className="zorai-feature-surface zorai-activity-surface">
      <div className="zorai-view-header zorai-activity-header">
        <div>
          <div className="zorai-kicker">Usage</div>
          <h1>Tokens, requests, and cost across providers and models.</h1>
          <p>{formatCount(stats.totals.totalTokens)} tokens over {formatCount(stats.totals.requests)} requests.</p>
        </div>
      </div>
      <UsagePanel stats={stats} />
    </section>
  );
}
