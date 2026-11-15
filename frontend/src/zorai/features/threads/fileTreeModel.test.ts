import { describe, expect, it } from "vitest";
import { buildFileTree, type FileTreeNode } from "./fileTreeModel";

const outline = <T,>(nodes: FileTreeNode<T>[], depth = 0): string[] => nodes.flatMap((node) => (
  node.kind === "folder" ? [`${"  ".repeat(depth)}${node.name}/`, ...outline(node.children, depth + 1)] : [`${"  ".repeat(depth)}${node.name}`]
));

describe("buildFileTree", () => {
  it("groups files under their folders and collapses single-child folder chains so deep paths stay readable", () => {
    const paths = [
      "/mnt/e/gitlab/ai/pivo/tools/run.py",
      "/mnt/e/gitlab/ai/pivo/tools/modal.py",
      "/mnt/e/gitlab/ai/pivo/README.md",
      "~/.zorai/threads/goal_1/working-checkpoint.md",
      "src/app.ts",
      "notes.md",
    ];
    expect(outline(buildFileTree(paths, (path) => path, (path) => path))).toEqual([
      "/mnt/e/gitlab/ai/pivo/",
      "  tools/",
      "    modal.py",
      "    run.py",
      "  README.md",
      "~/.zorai/threads/goal_1/",
      "  working-checkpoint.md",
      "src/",
      "  app.ts",
      "notes.md",
    ]);
  });

  it("keeps every entry even when two share a path, so none disappear from review", () => {
    const items = [{ path: "a/x.md", id: "1" }, { path: "a/x.md", id: "2" }];
    const tree = buildFileTree(items, (item) => item.path, (item) => item.id);
    expect(tree[0].kind === "folder" && tree[0].children.map((node) => node.key)).toEqual(["1", "2"]);
  });
});
