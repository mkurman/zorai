import { isRunActive, type AgentRun } from "@/lib/agentRuns";
import { normalizeEpochMs } from "./threadFilterModel";

export type SessionThreadIdentity = {
  id: string;
  daemonThreadId?: string | null;
  title?: string | null;
  updatedAt?: number;
};

export type DelegatedAssignment = {
  phase: string;
  description: string;
};

export type DelegatedSessionView = {
  parentThreadId: string;
  assignment: DelegatedAssignment;
  siblings: AgentRun[];
  index: number;
};

const MINUTE_MS = 60_000;
const HOUR_MS = 60 * MINUTE_MS;
const DAY_MS = 24 * HOUR_MS;

export function sameAgentRunSnapshot(left: readonly AgentRun[], right: readonly AgentRun[]): boolean {
  if (left.length !== right.length) return false;
  for (let index = 0; index < left.length; index += 1) {
    const current = left[index];
    const next = right[index];
    if (
      current.id !== next.id
      || current.status !== next.status
      || current.thread_id !== next.thread_id
      || current.parent_thread_id !== next.parent_thread_id
      || current.description !== next.description
      || current.created_at !== next.created_at
    ) {
      return false;
    }
  }
  return true;
}

export function workerThreadIds(runs: readonly AgentRun[]): Set<string> {
  const ids = new Set<string>();
  for (const run of runs) {
    if (!isSpawnedWorkerRun(run) || !run.thread_id) continue;
    ids.add(run.thread_id);
  }
  return ids;
}

export function isWorkerThread(thread: SessionThreadIdentity, workerIds: ReadonlySet<string>): boolean {
  if (workerIds.has(thread.id)) return true;
  return Boolean(thread.daemonThreadId && workerIds.has(thread.daemonThreadId));
}

export function workerCountForThread(thread: SessionThreadIdentity, runs: readonly AgentRun[]): number {
  const ids = identitySet(thread);
  const children = new Set<string>();
  for (const run of runs) {
    if (!run.thread_id || !run.parent_thread_id || !ids.has(run.parent_thread_id)) continue;
    children.add(run.thread_id);
  }
  return children.size;
}

export function threadIsWorking(
  thread: SessionThreadIdentity,
  runs: readonly AgentRun[],
  activeStreamingThreadId: string | null,
): boolean {
  const ids = identitySet(thread);
  if (activeStreamingThreadId && ids.has(activeStreamingThreadId)) return true;
  return runs.some((run) => isRunActive(run) && (
    (run.thread_id != null && ids.has(run.thread_id))
    || (run.parent_thread_id != null && ids.has(run.parent_thread_id))
  ));
}

export function threadIsUnread(
  thread: SessionThreadIdentity,
  lastReadAt: number | null,
  active: boolean,
): boolean {
  if (active || lastReadAt == null) return false;
  const updatedAt = normalizeEpochMs(thread.updatedAt);
  return updatedAt != null && updatedAt > lastReadAt;
}

export function sessionActivityLabel(updatedAt: number, now: number): string {
  const at = normalizeEpochMs(updatedAt);
  if (at == null) return "New";
  const elapsed = Math.max(0, now - at);
  if (elapsed < MINUTE_MS) return "Just now";
  if (elapsed < HOUR_MS) return `${Math.floor(elapsed / MINUTE_MS)}m ago`;
  if (elapsed < DAY_MS) return `${Math.floor(elapsed / HOUR_MS)}h ago`;
  if (elapsed < 7 * DAY_MS) return `${Math.floor(elapsed / DAY_MS)}d ago`;
  return new Date(at).toLocaleDateString();
}

export function formatWorkerPhase(phase: string): string {
  const trimmed = phase.trim();
  if (!trimmed) return "Delegated agent";
  const label = `${trimmed.charAt(0).toUpperCase()}${trimmed.slice(1)}`;
  return label.toLowerCase().endsWith("agent") ? label : `${label} agent`;
}

/**
 * A resumed worker keeps its historical thread title. The header follows the
 * latest brief the lead actually dispatched to that same worker thread.
 */
export function delegatedSessionView(
  thread: SessionThreadIdentity | null | undefined,
  runs: readonly AgentRun[],
): DelegatedSessionView | null {
  if (!thread) return null;
  const ids = identitySet(thread);
  const ownRuns = runs
    .filter((run) => isSpawnedWorkerRun(run) && ids.has(run.thread_id ?? "") && !ids.has(run.parent_thread_id ?? ""))
    .sort(compareRuns);
  const latestOwn = ownRuns[ownRuns.length - 1];
  if (!latestOwn?.parent_thread_id) return null;

  const siblings = latestRunsByThread(
    runs.filter((run) => isSpawnedWorkerRun(run) && run.parent_thread_id === latestOwn.parent_thread_id),
  );
  const index = siblings.findIndex((run) => run.thread_id != null && ids.has(run.thread_id));
  return {
    parentThreadId: latestOwn.parent_thread_id,
    assignment: assignmentFromRun(latestOwn),
    siblings,
    index: index < 0 ? 0 : index,
  };
}

function isSpawnedWorkerRun(run: AgentRun): boolean {
  if (run.thread_id == null || run.parent_thread_id == null || run.thread_id === run.parent_thread_id) {
    return false;
  }
  return run.source === "subagent" || run.source === "spawn_subagent";
}

function assignmentFromRun(run: AgentRun): DelegatedAssignment {
  const description = run.description.trim() || run.title.trim() || "Delegated task";
  const classification = run.classification.trim();
  const phase = classification && classification !== "mixed"
    ? classification
    : run.runtime?.trim() || "worker";
  return { phase, description };
}

function latestRunsByThread(runs: readonly AgentRun[]): AgentRun[] {
  const byThread = new Map<string, { latest: AgentRun; earliest: number }>();
  for (const run of runs) {
    const threadId = run.thread_id;
    if (!threadId) continue;
    const current = byThread.get(threadId);
    if (!current) {
      byThread.set(threadId, { latest: run, earliest: run.created_at });
      continue;
    }
    current.earliest = Math.min(current.earliest, run.created_at);
    if (compareRuns(run, current.latest) > 0) current.latest = run;
  }
  return [...byThread.values()]
    .sort((left, right) => left.earliest - right.earliest || left.latest.id.localeCompare(right.latest.id))
    .map((entry) => entry.latest);
}

function compareRuns(left: AgentRun, right: AgentRun): number {
  if (left.created_at !== right.created_at) return left.created_at - right.created_at;
  return left.id.localeCompare(right.id);
}

function identitySet(thread: SessionThreadIdentity): Set<string> {
  const ids = new Set<string>();
  if (thread.id) ids.add(thread.id);
  if (thread.daemonThreadId) ids.add(thread.daemonThreadId);
  return ids;
}
