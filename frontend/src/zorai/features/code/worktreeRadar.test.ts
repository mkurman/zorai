import { describe, expect, it } from "vitest";
import { conflictRadar, firstOverlap } from "./worktreeRadar";

describe("worktree conflict radar", () => {
  it("warns about files two agents both touched, or that you are editing while an agent changed them", () => {
    const conflicts = conflictRadar([
      { path: "/w/a", label: "task-a", files: ["src/lib.rs", "src/a.rs"] },
      { path: "/w/b", label: "task-b", files: ["src/lib.rs", "src/b.rs"] },
    ], ["src/a.rs", "README.md"]);
    expect(conflicts).toEqual([
      { file: "src/a.rs", owners: ["task-a", "your working tree"], overlap: null },
      { file: "src/lib.rs", owners: ["task-a", "task-b"], overlap: null },
    ]);
  });

  it("ranks real line overlaps first, because those become merge conflicts", () => {
    const conflicts = conflictRadar([
      { path: "/w/a", label: "a", files: ["x.rs", "y.rs"], ranges: { "x.rs": [[10, 20]], "y.rs": [[1, 2]] } },
      { path: "/w/b", label: "b", files: ["x.rs", "y.rs"], ranges: { "x.rs": [[18, 30]], "y.rs": [[50, 60]] } },
    ], []);
    expect(conflicts.map((conflict) => [conflict.file, conflict.overlap])).toEqual([["x.rs", [18, 20]], ["y.rs", null]]);
  });

  it("stays quiet when every worktree works on separate files", () => {
    expect(conflictRadar([{ path: "/w/a", label: "a", files: ["x"] }, { path: "/w/b", label: "b", files: ["y"] }], ["z"])).toEqual([]);
    expect(firstOverlap([[1, 5]], [[6, 9]])).toBeNull();
  });
});
