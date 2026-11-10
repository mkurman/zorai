import { describe, expect, it } from "vitest";
import { layoutCommitGraph } from "./commitGraphLayout";

const commit = (hash: string, parents: string[]) => ({ hash, shortHash: hash, author: "a", date: "", subject: hash, parents, refs: [] });

describe("commit graph layout", () => {
  it("keeps a straight history in one lane and one colour", () => {
    const rows = layoutCommitGraph([commit("c", ["b"]), commit("b", ["a"]), commit("a", [])]);
    expect(rows.map((row) => row.graph.lane)).toEqual([0, 0, 0]);
    expect(new Set(rows.map((row) => row.graph.color)).size).toBe(1);
    expect(rows[2].graph.outgoing).toEqual([]);
  });

  it("draws a merged branch in its own lane and joins it back at the fork, so lines follow history, not authors", () => {
    const rows = layoutCommitGraph([
      commit("m", ["d", "f"]),
      commit("f", ["base"]),
      commit("d", ["base"]),
      commit("base", []),
    ]);
    const [merge, feature, develop, base] = rows.map((row) => row.graph);
    expect(merge.lane).toBe(0);
    expect(merge.outgoing.map((edge) => edge.to)).toEqual([0, 1]);
    expect(feature.lane).toBe(1);
    expect(feature.passing.map((edge) => edge.from)).toEqual([0]);
    expect(feature.color).not.toBe(merge.color);
    expect(develop.lane).toBe(0);
    expect(develop.outgoing).toEqual([{ from: 0, to: 0, color: merge.color }]);
    expect(base.incoming.map((edge) => edge.from)).toEqual([0, 1]);
    expect(base.lane).toBe(0);
  });

  it("gives an unrelated branch tip a fresh lane when no lane is waiting for it", () => {
    const rows = layoutCommitGraph([commit("x", ["p"]), commit("y", ["q"])]);
    expect(rows[1].graph.lane).toBe(1);
    expect(rows[1].graph.passing.map((edge) => edge.from)).toEqual([0]);
  });
});
