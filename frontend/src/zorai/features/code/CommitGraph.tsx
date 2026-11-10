import { useEffect, useState } from "react";
import { useCommitGraphLayout, type LaidOutCommit } from "./commitGraphLayout";
import { loadWorkspaceGraphHistory, type WorkspaceGraphCommit } from "./workspaceRefresh";

const PAGE_SIZE = 200;
const GRAPH_MAX_LANES = 10;
const LANE_WIDTH = 12;
const ROW_HEIGHT = 24;

const laneX = (lane: number) => lane * LANE_WIDTH + LANE_WIDTH / 2;
const laneColor = (color: number) => `var(--zorai-graph-${color % 8})`;

function edgePath(fromX: number, fromY: number, toX: number, toY: number): string {
  if (fromX === toX) return `M ${fromX} ${fromY} V ${toY}`;
  const bend = (toY - fromY) / 2;
  return `M ${fromX} ${fromY} C ${fromX} ${fromY + bend} ${toX} ${toY - bend} ${toX} ${toY}`;
}

function CommitGraphCell({ row, laneCount }: { row: LaidOutCommit; laneCount: number }) {
  const mid = ROW_HEIGHT / 2;
  const { lane, color, incoming, passing, outgoing } = row.graph;
  const isMerge = row.parents.length > 1;
  return (
    <svg className="zorai-commit-graph-cells" width={laneCount * LANE_WIDTH} height={ROW_HEIGHT} viewBox={`0 0 ${laneCount * LANE_WIDTH} ${ROW_HEIGHT}`} aria-hidden="true">
      {passing.map((edge) => <path key={`p${edge.from}`} d={edgePath(laneX(edge.from), 0, laneX(edge.to), ROW_HEIGHT)} stroke={laneColor(edge.color)} />)}
      {incoming.map((edge) => <path key={`i${edge.from}`} d={edgePath(laneX(edge.from), 0, laneX(edge.to), mid)} stroke={laneColor(edge.color)} />)}
      {outgoing.map((edge) => <path key={`o${edge.to}`} d={edgePath(laneX(edge.from), mid, laneX(edge.to), ROW_HEIGHT)} stroke={laneColor(edge.color)} />)}
      <circle
        className={isMerge ? "is-merge" : undefined}
        cx={laneX(lane)}
        cy={mid}
        r={isMerge ? 3.2 : 3.6}
        stroke={laneColor(color)}
        fill={isMerge ? "var(--zorai-bg-panel)" : laneColor(color)}
      />
    </svg>
  );
}

export function CommitGraph({ root, bridge }: {
  root: string;
  bridge: Pick<ZoraiBridge, "workspaceGitHistory">;
}) {
  const [commits, setCommits] = useState<WorkspaceGraphCommit[]>([]);
  const [limit, setLimit] = useState(PAGE_SIZE);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    void loadWorkspaceGraphHistory(bridge, root, limit)
      .then((next) => { if (!cancelled) { setCommits(next); setError(null); } })
      .catch((reason: unknown) => { if (!cancelled) setError(reason instanceof Error ? reason.message : String(reason)); })
      .finally(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, [root, bridge, limit]);

  const rows = useCommitGraphLayout(commits);
  const laneCount = Math.min(GRAPH_MAX_LANES, Math.max(1, ...rows.map((row) => row.graph.width)));

  return (
    <div className="zorai-commit-graph" role="list" aria-label="Commit graph">
      {error ? <div className="zorai-commit-graph-error" role="alert">{error}</div> : null}
      {!loading && rows.length === 0 && !error ? <div className="zorai-commit-graph-empty">No commits.</div> : null}
      {rows.map((row) => (
        <div className="zorai-commit-graph-row" role="listitem" key={row.hash} title={`${row.shortHash} ${row.subject}`}>
          <CommitGraphCell row={row} laneCount={laneCount} />
          <span className="zorai-commit-graph-hash">{row.shortHash}</span>
          <span className="zorai-commit-graph-subject">
            {row.subject}
            {row.refs.length > 0 ? (
              <span className="zorai-commit-graph-refs">
                {row.refs.map((ref) => <em key={ref}>{ref}</em>)}
              </span>
            ) : null}
          </span>
          <span className="zorai-commit-graph-author">{row.author}</span>
        </div>
      ))}
      {rows.length >= limit ? (
        <button type="button" className="zorai-commit-graph-more" onClick={() => setLimit((current) => current + PAGE_SIZE)}>
          Show more
        </button>
      ) : null}
    </div>
  );
}
