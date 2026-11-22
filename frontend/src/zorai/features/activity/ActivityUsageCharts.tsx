import { useMemo, useState } from "react";
import {
  CHART_COLORS,
  DailyUsageRow,
  UsageRow,
  formatCount,
  formatCost,
  formatDayLabel,
  formatPercent,
  shareOfTotal,
} from "./ActivityUsageStats";

const CHART_MAX_DAYS = 60;

type TokenTimelineMode = "stacked" | "total";

export function DailyTokenChart({ rows, height = 132 }: { rows: DailyUsageRow[]; height?: number }) {
  const [mode, setMode] = useState<TokenTimelineMode>("stacked");
  const [hoveredIndex, setHoveredIndex] = useState<number | null>(null);

  const series = useMemo(() => {
    if (rows.length === 0) return { bars: [], max: 0 };
    const visible = rows.slice(-CHART_MAX_DAYS);
    const max = visible.reduce((peak, row) => Math.max(peak, row.totalTokens), 0);
    const bars = visible.map((row) => ({
      row,
      inputHeight: max > 0 ? (row.inputTokens / max) * 100 : 0,
      outputHeight: max > 0 ? (row.outputTokens / max) * 100 : 0,
    }));
    return { bars, max };
  }, [rows]);

  if (series.bars.length === 0) {
    return <div className="zorai-empty-state">No daily token data in the loaded history.</div>;
  }

  const hovered = hoveredIndex !== null ? series.bars[hoveredIndex] : null;

  return (
    <div className="zorai-chart-block">
      <div className="zorai-chart-head">
        <div className="zorai-section-label">Daily Tokens</div>
        <div className="zorai-chart-controls">
          {(["stacked", "total"] as const).map((option) => (
            <button
              key={option}
              type="button"
              className={["zorai-ghost-button zorai-chart-toggle", mode === option ? "zorai-button--active" : ""].filter(Boolean).join(" ")}
              onClick={() => setMode(option)}
            >
              {option === "stacked" ? "In + Out" : "Total"}
            </button>
          ))}
        </div>
      </div>
      <div className="zorai-chart-columns" role="img" aria-label={`Daily token usage, last ${series.bars.length} days, peak ${formatCount(series.max)} tokens`} style={{ height }}>
        {series.bars.map((bar, index) => {
          const totalPercent = series.max > 0 ? (bar.row.totalTokens / series.max) * 100 : 0;
          const outputPercent = mode === "stacked" ? 0 : series.max > 0 ? (bar.row.outputTokens / series.max) * 100 : 0;
          const inputPercent = mode === "stacked" ? bar.inputHeight : Math.max(0, totalPercent - outputPercent);
          const isActive = hoveredIndex === index;
          return (
            <div
              key={bar.row.dayKey}
              className={["zorai-chart-col", isActive ? "zorai-chart-col--active" : ""].filter(Boolean).join(" ")}
              onMouseEnter={() => setHoveredIndex(index)}
              onMouseLeave={() => setHoveredIndex((current) => (current === index ? null : current))}
            >
              <div className="zorai-chart-col-track">
                {mode === "stacked" ? (
                  <>
                    {bar.outputHeight > 0 ? <i className="zorai-chart-seg zorai-chart-seg--output" style={{ height: `${bar.outputHeight}%` }} /> : null}
                    {inputPercent > 0 ? <i className="zorai-chart-seg zorai-chart-seg--input" style={{ height: `${inputPercent}%` }} /> : null}
                  </>
                ) : (
                  <>
                    {inputPercent > 0 ? <i className="zorai-chart-seg zorai-chart-seg--input" style={{ height: `${inputPercent}%` }} /> : null}
                    {outputPercent > 0 ? <i className="zorai-chart-seg zorai-chart-seg--output" style={{ height: `${outputPercent}%` }} /> : null}
                  </>
                )}
              </div>
              <span className="zorai-chart-col-label">{formatDayLabel(bar.row.dayStart)}</span>
            </div>
          );
        })}
      </div>
      <div className="zorai-chart-legend">
        <LegendItem color="var(--zorai-chart-input, var(--zorai-accent))" label="Input" />
        <LegendItem color="var(--zorai-chart-output, var(--zorai-accent-secondary))" label="Output" />
        {hovered ? (
          <span className="zorai-chart-tooltip">
            {formatDayLabel(hovered.row.dayStart, true)}: {formatCount(hovered.row.totalTokens)} tok · {formatCost(hovered.row.cost)} · {formatCount(hovered.row.requests)} req
          </span>
        ) : null}
      </div>
    </div>
  );
}

export function CostAreaChart({ rows, height = 96 }: { rows: DailyUsageRow[]; height?: number }) {
  const [hoveredIndex, setHoveredIndex] = useState<number | null>(null);

  const { points, areaPath, linePath, max } = useMemo(() => {
    const visible = rows.slice(-CHART_MAX_DAYS);
    if (visible.length === 0) return { points: [], areaPath: "", linePath: "", max: 0 };
    const peak = visible.reduce((acc, row) => Math.max(acc, row.cost), 0);
    const step = visible.length > 1 ? 100 / (visible.length - 1) : 100;
    const mapped = visible.map((row, index) => ({
      row,
      x: visible.length > 1 ? index * step : 50,
      y: peak > 0 ? 100 - (row.cost / peak) * 92 : 100,
    }));
    const line = mapped.map((point, index) => `${index === 0 ? "M" : "L"}${point.x.toFixed(2)},${point.y.toFixed(2)}`).join(" ");
    const area = `${line} L100,100 L0,100 Z`;
    return { points: mapped, areaPath: area, linePath: line, max: peak };
  }, [rows]);

  if (points.length === 0) {
    return <div className="zorai-empty-state">No daily cost data in the loaded history.</div>;
  }

  const hovered = hoveredIndex !== null ? points[hoveredIndex] : null;

  return (
    <div className="zorai-chart-block">
      <div className="zorai-chart-head">
        <div className="zorai-section-label">Daily Cost</div>
        <span className="zorai-chart-tooltip">peak {formatCost(max)}</span>
      </div>
      <svg
        className="zorai-chart-area"
        viewBox="0 0 100 100"
        preserveAspectRatio="none"
        style={{ height }}
        role="img"
        aria-label={`Daily cost, last ${points.length} days, peak ${formatCost(max)}`}
      >
        {[25, 50, 75].map((y) => <line key={y} className="zorai-chart-gridline" x1="0" y1={y} x2="100" y2={y} />)}
        <path className="zorai-chart-area-fill" d={areaPath} />
        <path className="zorai-chart-area-line" d={linePath} />
        {points.map((point, index) => (
          <rect
            key={point.row.dayKey}
            className="zorai-chart-hitzone"
            x={points.length > 1 ? point.x - 100 / points.length / 2 : 0}
            y="0"
            width={points.length > 1 ? 100 / points.length : 100}
            height="100"
            onMouseEnter={() => setHoveredIndex(index)}
            onMouseLeave={() => setHoveredIndex((current) => (current === index ? null : current))}
          />
        ))}
        {hovered ? (
          <circle className="zorai-chart-dot" cx={hovered.x} cy={hovered.y} r="1.4" />
        ) : null}
      </svg>
      <div className="zorai-chart-axis">
        <span>{formatDayLabel(points[0].row.dayStart)}</span>
        {hovered ? <span className="zorai-chart-tooltip">{formatDayLabel(hovered.row.dayStart, true)}: {formatCost(hovered.row.cost)}</span> : <span />}
        <span>{formatDayLabel(points[points.length - 1].row.dayStart)}</span>
      </div>
    </div>
  );
}

export function ShareDonut({
  rows,
  metric,
  emptyText,
}: {
  rows: UsageRow[];
  metric: "totalTokens" | "cost";
  emptyText: string;
}) {
  const [activeIndex, setActiveIndex] = useState<number | null>(null);

  const slices = useMemo(() => {
    const total = rows.reduce((acc, row) => acc + row[metric], 0);
    if (total <= 0) return [];
    let offset = 0;
    return rows.map((row) => {
      const share = shareOfTotal(row[metric], total);
      const slice = { row, share, offset };
      offset += share;
      return slice;
    });
  }, [rows, metric]);

  if (slices.length === 0) return <div className="zorai-empty-state">{emptyText}</div>;

  const active = activeIndex !== null ? slices[activeIndex] : null;
  const accentStart = activeIndex !== null ? activeIndex % CHART_COLORS.length : 0;

  return (
    <div className="zorai-donut-wrap">
      <div className="zorai-donut" role="img" aria-label={active ? `${active.row.provider}/${active.row.model}: ${formatPercent(active.share)}` : "Usage share by model"}>
        <div
          className="zorai-donut-ring"
          style={{ background: `conic-gradient(${slices.map((slice, index) => {
            const start = slice.offset * 100;
            const end = (slice.offset + slice.share) * 100;
            const dim = activeIndex !== null && activeIndex !== index ? " var(--zorai-bg-elevated)" : "";
            const color = dim || CHART_COLORS[(accentStart + index) % CHART_COLORS.length];
            return `${color} ${start.toFixed(2)}% ${end.toFixed(2)}%`;
          }).join(", ")}, var(--zorai-bg-elevated) 0)` }}
        />
        <div className="zorai-donut-hole">
          {active ? (
            <>
              <strong>{formatPercent(active.share)}</strong>
              <span>{active.row.model}</span>
            </>
          ) : (
            <>
              <strong>{metric === "totalTokens" ? formatCount(slices.reduce((acc, slice) => acc + slice.row.totalTokens, 0)) : formatCost(slices.reduce((acc, slice) => acc + slice.row.cost, 0))}</strong>
              <span>{metric === "totalTokens" ? "tokens" : "cost"}</span>
            </>
          )}
        </div>
      </div>
      <div className="zorai-donut-legend">
        {slices.slice(0, 8).map((slice, index) => (
          <button
            key={slice.row.key}
            type="button"
            className={["zorai-donut-legend-item", activeIndex === index ? "zorai-donut-legend-item--active" : ""].filter(Boolean).join(" ")}
            onMouseEnter={() => setActiveIndex(index)}
            onMouseLeave={() => setActiveIndex((current) => (current === index ? null : current))}
          >
            <i style={{ background: CHART_COLORS[(accentStart + index) % CHART_COLORS.length] }} />
            <span title={slice.row.model ? `${slice.row.provider} / ${slice.row.model}` : slice.row.provider}>{modelShortName(slice.row.model) || slice.row.provider}</span>
            <strong>{formatPercent(slice.share)}</strong>
          </button>
        ))}
      </div>
    </div>
  );
}

function modelShortName(model: string): string {
  return model.slice(model.lastIndexOf("/") + 1);
}

function modelSourceLabel(provider: string, model: string): string {
  const vendor = model.includes("/") ? model.slice(0, model.lastIndexOf("/")) : "";
  return vendor && vendor !== provider ? `${provider} · ${vendor}` : provider;
}

export function UsageBarList({
  rows,
  metric,
  emptyText,
}: {
  rows: UsageRow[];
  metric: "totalTokens" | "cost";
  emptyText: string;
}) {
  const ranked = rows.filter((row) => row[metric] > 0);
  const max = ranked.reduce((peak, row) => Math.max(peak, row[metric]), 0);
  const total = ranked.reduce((acc, row) => acc + row[metric], 0);
  const omitted = rows.length - ranked.length;

  if (ranked.length === 0 || max <= 0) return <div className="zorai-empty-state">{emptyText}</div>;

  return (
    <div className="zorai-usage-bars">
      {ranked.slice(0, 8).map((row) => (
        <div key={row.key} className="zorai-usage-bar zorai-usage-bar--ranked">
          <span className="zorai-usage-bar__label" title={row.model ? `${row.provider} / ${row.model}` : row.provider}>
            <b>{modelShortName(row.model) || row.provider}</b>
            {row.model ? <small>{modelSourceLabel(row.provider, row.model)}</small> : null}
          </span>
          <div>
            <i style={{ width: `${(row[metric] / max) * 100}%` }} />
          </div>
          <strong>
            {metric === "totalTokens" ? formatCount(row.totalTokens) : formatCost(row.cost)}
            <em>{formatPercent(shareOfTotal(row[metric], total))}</em>
          </strong>
        </div>
      ))}
      {omitted > 0 ? <div className="zorai-usage-bars__note">{omitted} more {rows.some((row) => row.model) ? "model" : "provider"}{omitted === 1 ? "" : "s"} with no recorded {metric === "cost" ? "cost" : "tokens"} in this window.</div> : null}
    </div>
  );
}

export function TotalsSplitBar({ inputTokens, outputTokens }: { inputTokens: number; outputTokens: number }) {
  const total = inputTokens + outputTokens;
  const inputShare = shareOfTotal(inputTokens, total);

  if (total <= 0) return <div className="zorai-empty-state">No token split available.</div>;

  return (
    <div className="zorai-split-bar-block">
      <div className="zorai-split-bar" role="img" aria-label={`Input ${formatPercent(inputShare)}, output ${formatPercent(1 - inputShare)}`}>
        <i className="zorai-split-bar--input" style={{ width: `${inputShare * 100}%` }} />
        <i className="zorai-split-bar--output" style={{ width: `${(1 - inputShare) * 100}%` }} />
      </div>
      <div className="zorai-split-bar-meta">
        <span><i className="zorai-dot zorai-dot--input" /> Input {formatCount(inputTokens)} · {formatPercent(inputShare)}</span>
        <span><i className="zorai-dot zorai-dot--output" /> Output {formatCount(outputTokens)} · {formatPercent(1 - inputShare)}</span>
      </div>
    </div>
  );
}

function LegendItem({ color, label }: { color: string; label: string }) {
  return (
    <span className="zorai-chart-legend-item">
      <i style={{ background: color }} />
      {label}
    </span>
  );
}