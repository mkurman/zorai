import type { ReactNode } from "react";
import type { AgentRun } from "@/lib/agentRuns";
import {
  formatRunStatus,
  runStatusColor,
} from "@/lib/agentRuns";
import type {
  SpawnedAgentTree,
  SpawnedAgentTreeNode,
} from "@/lib/spawnedAgentTree";
import { ActionButton, EmptyPanel } from "./shared";

type SpawnedAgentsPanelProps = {
  tree: SpawnedAgentTree<AgentRun> | null;
  selectedDaemonThreadId: string | null;
  canGoBackThread: boolean;
  threadNavigationDepth: number;
  backThreadTitle: string | null;
  canOpenSpawnedThread: (run: AgentRun) => boolean;
  openSpawnedThread: (run: AgentRun) => Promise<boolean>;
  goBackThread: () => void;
  compact?: boolean;
};

type SpawnedAgentNodeProps = {
  node: SpawnedAgentTreeNode<AgentRun>;
  depth: number;
  selectedDaemonThreadId: string | null;
  selectedTaskId?: string | null;
  canOpenSpawnedThread: (run: AgentRun) => boolean;
  openSpawnedThread: (run: AgentRun) => Promise<boolean>;
  renderActions?: (args: {
    node: SpawnedAgentTreeNode<AgentRun>;
    canOpen: boolean;
    openSpawnedThread: () => void;
  }) => ReactNode;
  compact?: boolean;
};

function sessionHint(run: AgentRun): string | null {
  if (!run.session_id) {
    return null;
  }
  return run.session_id;
}

function renderNodeMeta(run: AgentRun): string | null {
  const parts = [run.runtime ?? null, sessionHint(run)].filter(Boolean);
  return parts.length > 0 ? parts.join(" · ") : null;
}

export function SpawnedAgentNode({
  node,
  depth,
  selectedDaemonThreadId,
  selectedTaskId,
  canOpenSpawnedThread,
  openSpawnedThread,
  renderActions,
  compact = false,
}: SpawnedAgentNodeProps) {
  const isSelected = Boolean(
    (node.item.thread_id && node.item.thread_id === selectedDaemonThreadId) ||
      (selectedTaskId &&
        (node.item.task_id === selectedTaskId || node.item.id === selectedTaskId)),
  );
  const canOpen = canOpenSpawnedThread(node.item);
  const meta = renderNodeMeta(node.item);

  return (
    <div
      data-node-title={node.item.title}
      data-node-depth={depth}
      style={{
        display: "grid",
        gap: "var(--space-2)",
        marginLeft: depth > 0 ? (compact ? 8 : 16) : 0,
        paddingLeft: depth > 0 ? (compact ? "var(--space-2)" : "var(--space-3)") : 0,
        borderLeft: depth > 0 ? "1px solid var(--glass-border)" : "none",
      }}
    >
      <div
        style={{
          border: "1px solid",
          borderColor: isSelected ? "var(--accent)" : "var(--border)",
          borderRadius: compact ? "var(--radius-md)" : "var(--radius-lg)",
          padding: compact ? "var(--space-2)" : "var(--space-3)",
          background: isSelected ? "color-mix(in srgb, var(--accent) 8%, transparent)" : "var(--bg-secondary)",
          display: "grid",
          gap: compact ? 6 : "var(--space-2)",
        }}
      >
        <div style={{ display: "flex", justifyContent: "space-between", gap: "var(--space-2)", alignItems: "flex-start" }}>
          <div style={{ display: "grid", gap: 4 }}>
            <div style={{ fontSize: compact ? "var(--text-xs)" : "var(--text-sm)", fontWeight: 700, color: "var(--text-primary)" }}>
              {node.item.title}
            </div>
            {meta && (
              <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)", wordBreak: "break-word" }}>
                {meta}
              </div>
            )}
          </div>
          <div style={{ display: "flex", gap: "var(--space-1)", flexWrap: "wrap", justifyContent: "flex-end" }}>
            <span
              style={{
                fontSize: 11,
                fontWeight: 700,
                borderRadius: 999,
                padding: compact ? "1px 6px" : "2px 8px",
                border: "1px solid color-mix(in srgb, currentColor 35%, transparent)",
                color: runStatusColor(node.item.status),
                background: "color-mix(in srgb, currentColor 10%, transparent)",
              }}
            >
              {formatRunStatus(node.item)}
            </span>
            {isSelected && (
              <span
                style={{
                  fontSize: 11,
                  fontWeight: 700,
                  borderRadius: 999,
                  padding: compact ? "1px 6px" : "2px 8px",
                  border: "1px solid var(--accent-soft)",
                  color: "var(--accent)",
                  background: "color-mix(in srgb, var(--accent) 12%, transparent)",
                }}
              >
                Current
              </span>
            )}
          </div>
        </div>

        <div style={{ display: "flex", justifyContent: "space-between", gap: "var(--space-2)", alignItems: "center", flexWrap: "wrap" }}>
          <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)" }}>
            {node.item.thread_id ? "Chat ready" : "Waiting for thread"}
          </div>
          {renderActions ? (
            renderActions({
              node,
              canOpen,
              openSpawnedThread: () => void openSpawnedThread(node.item),
            })
          ) : (
            <ActionButton
              disabled={!canOpen}
              onClick={canOpen ? () => void openSpawnedThread(node.item) : undefined}
            >
              <span aria-label={`Open chat for ${node.item.title}`}>Open Chat</span>
            </ActionButton>
          )}
        </div>
      </div>

      {node.children.map((child) => (
        <SpawnedAgentNode
          key={child.item.id}
          node={child}
          depth={depth + 1}
          selectedDaemonThreadId={selectedDaemonThreadId}
          selectedTaskId={selectedTaskId}
          canOpenSpawnedThread={canOpenSpawnedThread}
          openSpawnedThread={openSpawnedThread}
          renderActions={renderActions}
          compact={compact}
        />
      ))}
    </div>
  );
}

export function SpawnedAgentsPanel({
  tree,
  selectedDaemonThreadId,
  canGoBackThread,
  threadNavigationDepth,
  backThreadTitle,
  canOpenSpawnedThread,
  openSpawnedThread,
  goBackThread,
  compact = false,
}: SpawnedAgentsPanelProps) {
  const backLabel = backThreadTitle ? `Back to ${backThreadTitle}` : "Back";

  return (
    <aside
      style={{
        width: compact ? "100%" : 300,
        minWidth: compact ? 0 : 260,
        maxWidth: compact ? "none" : 340,
        height: "100%",
        border: "1px solid var(--border)",
        borderRadius: compact ? "var(--radius-md)" : "var(--radius-xl)",
        background: "var(--zorai-bg)",
        display: "flex",
        flexDirection: "column",
        overflow: "hidden",
      }}
    >
      <div
        style={{
          padding: compact ? "var(--space-2)" : "var(--space-3)",
          borderBottom: "1px solid var(--border)",
          background: "var(--bg-secondary)",
          display: "grid",
          gap: "var(--space-2)",
        }}
      >
        <div>
          <div style={{ fontSize: "var(--text-sm)", fontWeight: 700 }}>
            Spawned Agents
          </div>
          <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)", marginTop: 2 }}>
            {threadNavigationDepth > 0
              ? `${threadNavigationDepth} hop history`
              : "Open child threads without leaving the transcript."}
          </div>
        </div>
        <ActionButton
          disabled={!canGoBackThread}
          onClick={canGoBackThread ? goBackThread : undefined}
        >
          {backLabel}
        </ActionButton>
      </div>

      <div
        style={{
          flex: 1,
          minHeight: 0,
          overflow: "auto",
          padding: compact ? "var(--space-2)" : "var(--space-3)",
          display: "grid",
          gap: compact ? "var(--space-2)" : "var(--space-3)",
        }}
      >
        {!tree && (
          <EmptyPanel message="No spawned agents for this thread yet." />
        )}

        {tree?.anchor && (
          <SpawnedAgentNode
            node={tree.anchor}
            depth={0}
            selectedDaemonThreadId={selectedDaemonThreadId}
            selectedTaskId={null}
            canOpenSpawnedThread={canOpenSpawnedThread}
            openSpawnedThread={openSpawnedThread}
            compact={compact}
          />
        )}

        {tree?.roots.map((root) => (
          <SpawnedAgentNode
            key={root.item.id}
            node={root}
            depth={0}
            selectedDaemonThreadId={selectedDaemonThreadId}
            selectedTaskId={null}
            canOpenSpawnedThread={canOpenSpawnedThread}
            openSpawnedThread={openSpawnedThread}
            compact={compact}
          />
        ))}
      </div>
    </aside>
  );
}
