export type TimelineEntry =
  | { kind: "local"; id: string; at: number; label: string }
  | { kind: "commit"; id: string; at: number; label: string; author: string };

export function mergeTimeline(local: Array<{ id: string; at: number; size: number }>, commits: Array<{ hash: string; shortHash: string; author: string; date: string; subject: string }>): TimelineEntry[] {
  return [
    ...local.map((entry) => ({ kind: "local" as const, id: entry.id, at: entry.at, label: "File saved" })),
    ...commits.map((commit) => ({ kind: "commit" as const, id: commit.hash, at: Date.parse(commit.date) || 0, label: commit.subject || commit.shortHash, author: commit.author })),
  ].sort((left, right) => right.at - left.at);
}
