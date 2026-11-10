import { useMemo } from "react";
import type { WorkspaceGraphCommit } from "./workspaceRefresh";

export type GraphEdge = {
  from: number;
  to: number;
  color: number;
};

export type GraphLane = {
  lane: number;
  color: number;
  incoming: GraphEdge[];
  passing: GraphEdge[];
  outgoing: GraphEdge[];
  width: number;
};

export type LaidOutCommit = WorkspaceGraphCommit & { graph: GraphLane };

export function layoutCommitGraph(commits: WorkspaceGraphCommit[]): LaidOutCommit[] {
  let lanes: Array<string | null> = [];
  let colors: number[] = [];
  let nextColor = 0;
  const rows: LaidOutCommit[] = [];

  const freeSlot = (taken: Array<string | null>) => {
    const index = taken.indexOf(null);
    return index < 0 ? taken.length : index;
  };

  for (const commit of commits) {
    const before = lanes;
    const beforeColors = colors;
    const waiting = before.flatMap((hash, index) => (hash === commit.hash ? [index] : []));
    const lane = waiting[0] ?? freeSlot(before);
    const color = waiting.length > 0 ? beforeColors[lane] : nextColor++;

    const after = before.slice();
    const afterColors = beforeColors.slice();
    for (const index of waiting) after[index] = null;
    while (after.length <= lane) {
      after.push(null);
      afterColors.push(0);
    }

    const incoming: GraphEdge[] = waiting.map((index) => ({ from: index, to: lane, color: beforeColors[index] }));
    const passing: GraphEdge[] = before.flatMap((hash, index) => (hash !== null && hash !== commit.hash ? [{ from: index, to: index, color: beforeColors[index] }] : []));
    const outgoing: GraphEdge[] = [];

    commit.parents.forEach((parent, index) => {
      const existing = after.indexOf(parent);
      if (existing >= 0 && !(index === 0 && existing > lane && after[lane] === null)) {
        outgoing.push({ from: lane, to: existing, color: index === 0 ? color : afterColors[existing] });
        return;
      }
      const target = index === 0 && after[lane] === null ? lane : freeSlot(after);
      const targetColor = target === lane ? color : nextColor++;
      if (target === after.length) {
        after.push(parent);
        afterColors.push(targetColor);
      } else {
        after[target] = parent;
        afterColors[target] = targetColor;
      }
      outgoing.push({ from: lane, to: target, color: targetColor });
    });

    while (after.length > 0 && after[after.length - 1] === null) {
      after.pop();
      afterColors.pop();
    }

    const width = Math.max(before.length, after.length, lane + 1);
    rows.push({ ...commit, graph: { lane, color, incoming, passing, outgoing, width } });
    lanes = after;
    colors = afterColors;
  }
  return rows;
}

export function useCommitGraphLayout(commits: WorkspaceGraphCommit[]): LaidOutCommit[] {
  return useMemo(() => layoutCommitGraph(commits), [commits]);
}
