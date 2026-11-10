import { describe, expect, it } from "vitest";
import { buildGitChangeTree, type GitChangeTreeNode } from "./gitChangeTree";

const status = (path: string) => ({ path, indexStatus: " ", worktreeStatus: "M" }) as ZoraiWorkspaceGitStatus;
const shape = (nodes: GitChangeTreeNode[]): unknown[] => nodes.map((node) => node.kind === "folder" ? { [node.name]: shape(node.children) } : node.name);

describe("git change tree", () => {
  it("groups changes by folder, folders first, and compacts single-child folder chains like VS Code", () => {
    const tree = buildGitChangeTree([
      status("frontend/src/zorai/features/code/CodeView.tsx"),
      status("frontend/src/zorai/features/code/gitMenu.ts"),
      status("frontend/src/zorai/styles/zorai.css"),
      status("crates/zorai-cli/src/cli.rs"),
      status("README.md"),
    ]);
    expect(shape(tree)).toEqual([
      { "crates/zorai-cli/src": ["cli.rs"] },
      { "frontend/src/zorai": [{ "features/code": ["CodeView.tsx", "gitMenu.ts"] }, { styles: ["zorai.css"] }] },
      "README.md",
    ]);
  });

  it("keeps the full folder path on compacted nodes so collapsing stays stable", () => {
    const [folder] = buildGitChangeTree([status("a/b/c.ts")]);
    expect(folder.kind === "folder" && folder.path).toBe("a/b");
  });
});
