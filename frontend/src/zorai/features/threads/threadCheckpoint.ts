import { getBridge } from "@/lib/bridge";

export type CheckpointChange = { path: string; operation_id?: string | null; updated_at: number; is_text?: boolean; repo_root?: string | null };

export type CheckpointRestoreStep = { path: string; operationId: string; repoRoot: string | null };

export function toMillis(timestamp: number): number {
  return timestamp > 0 && timestamp < 1e12 ? timestamp * 1000 : timestamp;
}

export function checkpointRestorePlan(changes: CheckpointChange[], sinceMs: number): CheckpointRestoreStep[] {
  const earliest = new Map<string, CheckpointChange>();
  for (const change of changes) {
    if (!change.operation_id || change.is_text === false || toMillis(change.updated_at) < sinceMs) continue;
    const current = earliest.get(change.path);
    if (!current || change.updated_at < current.updated_at) earliest.set(change.path, change);
  }
  return [...earliest.values()]
    .sort((left, right) => left.path.localeCompare(right.path))
    .map((change) => ({ path: change.path, operationId: change.operation_id as string, repoRoot: change.repo_root ?? null }));
}

export async function loadCheckpointPlan(daemonThreadId: string, sinceMs: number): Promise<CheckpointRestoreStep[]> {
  const response = await getBridge()?.agentGetWorkContext?.(daemonThreadId) as { context?: { entries?: CheckpointChange[] }; entries?: CheckpointChange[] } | undefined;
  const entries = response?.context?.entries ?? response?.entries ?? [];
  return checkpointRestorePlan(entries, sinceMs);
}

export async function restoreCheckpoint(steps: CheckpointRestoreStep[]): Promise<{ restored: string[]; failed: string[] }> {
  const bridge = getBridge();
  const restored: string[] = [];
  const failed: string[] = [];
  for (const step of steps) {
    try {
      const before = await bridge?.agentGetFileOperationBefore?.(step.operationId, step.path);
      if (!before?.available || before.binary) throw new Error("no snapshot");
      if (before.before_existed) {
        const target = before.resolved_path ?? (step.repoRoot ? `${step.repoRoot.replace(/[\\/]+$/, "")}/${step.path}` : null);
        if (!target || !(await bridge?.writeFsText?.(target, before.content ?? ""))) throw new Error("write failed");
      } else if (!step.repoRoot || !(await bridge?.workspaceDeletePath?.(step.repoRoot, step.path))) {
        throw new Error("delete failed");
      }
      restored.push(step.path);
    } catch {
      failed.push(step.path);
    }
  }
  return { restored, failed };
}
