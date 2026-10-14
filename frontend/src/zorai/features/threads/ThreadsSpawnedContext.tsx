import { formatRunStatus, runStatusColor, type AgentRun } from "@/lib/agentRuns";
import type { SpawnedAgentTree, SpawnedAgentTreeNode } from "@/lib/spawnedAgentTree";

export function SpawnedContext({
  tree,
  selectedDaemonThreadId,
  canGoBackThread,
  threadNavigationDepth,
  backThreadTitle,
  canOpenSpawnedThread,
  openSpawnedThread,
  goBackThread,
  compact = false,
}: {
  tree: SpawnedAgentTree<AgentRun> | null;
  selectedDaemonThreadId: string | null;
  canGoBackThread: boolean;
  threadNavigationDepth: number;
  backThreadTitle: string | null;
  canOpenSpawnedThread: (run: AgentRun) => boolean;
  openSpawnedThread: (run: AgentRun) => Promise<boolean>;
  goBackThread: () => void;
  compact?: boolean;
}) {
  const backLabel = backThreadTitle ? `Back to ${backThreadTitle}` : "Back";

  return (
    <section className={compact ? "zorai-spawned-context zorai-spawned-context--compact" : "zorai-spawned-context"}>
      {compact ? null : <div className="zorai-spawned-context__header">
        <div>
          <div className="zorai-section-label">Spawned Agents</div>
          <span>{threadNavigationDepth > 0 ? `${threadNavigationDepth} hop history` : "Child work for this thread"}</span>
        </div>
        <button type="button" className="zorai-ghost-button" disabled={!canGoBackThread} onClick={canGoBackThread ? goBackThread : undefined}>
          {backLabel}
        </button>
      </div>}
      <div className="zorai-spawned-context__list">
        {!tree ? <div className="zorai-empty">No spawned agents for this thread yet.</div> : null}
        {tree?.anchor ? (
          <SpawnedContextNode
            node={tree.anchor}
            depth={0}
            selectedDaemonThreadId={selectedDaemonThreadId}
            canOpenSpawnedThread={canOpenSpawnedThread}
            openSpawnedThread={openSpawnedThread}
            compact={compact}
          />
        ) : null}
        {tree?.roots.map((root) => (
          <SpawnedContextNode
            key={root.item.id}
            node={root}
            depth={0}
            selectedDaemonThreadId={selectedDaemonThreadId}
            canOpenSpawnedThread={canOpenSpawnedThread}
            openSpawnedThread={openSpawnedThread}
            compact={compact}
          />
        ))}
      </div>
    </section>
  );
}

function spawnedRuntimeLabel(run: AgentRun): string | null {
  const provider = run.provider?.trim();
  const model = run.model?.trim();
  const effort = run.reasoning_effort?.trim();
  const parts = [provider, model, effort ? `effort: ${effort}` : ""].filter(Boolean);
  return parts.length > 0 ? parts.join(" · ") : null;
}

function SpawnedContextNode({
  node,
  depth,
  selectedDaemonThreadId,
  canOpenSpawnedThread,
  openSpawnedThread,
  compact,
}: {
  node: SpawnedAgentTreeNode<AgentRun>;
  depth: number;
  selectedDaemonThreadId: string | null;
  canOpenSpawnedThread: (run: AgentRun) => boolean;
  openSpawnedThread: (run: AgentRun) => Promise<boolean>;
  compact: boolean;
}) {
  const run = node.item;
  const canOpen = canOpenSpawnedThread(run);
  const selected = Boolean(run.thread_id && run.thread_id === selectedDaemonThreadId);
  const runtimeLabel = spawnedRuntimeLabel(run);

  const children = node.children.map((child) => (
    <SpawnedContextNode
      key={child.item.id}
      node={child}
      depth={depth + 1}
      selectedDaemonThreadId={selectedDaemonThreadId}
      canOpenSpawnedThread={canOpenSpawnedThread}
      openSpawnedThread={openSpawnedThread}
      compact={compact}
    />
  ));

  if (compact) {
    return (
      <>
        <button
          type="button"
          className={selected ? "zorai-spawned-row is-active" : "zorai-spawned-row"}
          style={{ paddingLeft: 8 + depth * 12 }}
          disabled={!canOpen}
          title={[run.title, runtimeLabel, formatRunStatus(run)].filter(Boolean).join("\n")}
          onClick={canOpen ? () => void openSpawnedThread(run) : undefined}
        >
          <span className="zorai-spawned-row__dot" style={{ background: runStatusColor(run.status) }} aria-hidden="true" />
          <span className="zorai-spawned-row__title">{run.title}</span>
          {run.model ? <span className="zorai-spawned-row__model">{run.model}</span> : null}
          <span className="zorai-spawned-row__status" style={{ color: runStatusColor(run.status) }}>{formatRunStatus(run)}</span>
        </button>
        {children}
      </>
    );
  }

  return (
    <div className="zorai-spawned-node" style={{ marginLeft: depth > 0 ? 10 : 0 }}>
      <article className={selected ? "zorai-spawned-card zorai-spawned-card--active" : "zorai-spawned-card"}>
        <div>
          <strong>{run.title}</strong>
          {runtimeLabel ? <span className="zorai-spawned-card__runtime-label">{runtimeLabel}</span> : null}
          <span className="zorai-spawned-card__session-id">{run.session_id}</span>
        </div>
        <div className="zorai-spawned-card__footer">
          <span style={{ color: runStatusColor(run.status) }}>{formatRunStatus(run)}</span>
          <button type="button" className="zorai-ghost-button" disabled={!canOpen} onClick={canOpen ? () => void openSpawnedThread(run) : undefined}>
            Open
          </button>
        </div>
      </article>
      {children}
    </div>
  );
}
