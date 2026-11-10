import { describe, expect, it } from "vitest";
import { filterExplorerEntries, searchWorkspaceFiles } from "./workspaceExplorerFilter";

describe("workspace file filter", () => {
  const root = [
    { name: "src", path: "src" },
    { name: "README.md", path: "README.md" },
  ];
  const children = (path: string) => path === "src"
    ? [{ name: "session.ts", path: "src/session.ts" }, { name: "main.rs", path: "src/main.rs" }]
    : [];

  it("keeps a folder visible when a loaded child matches, so search reaches files you have already opened", () => {
    expect(filterExplorerEntries(root, "session", children).map((entry) => entry.path)).toEqual(["src"]);
    expect(filterExplorerEntries(root, "", children)).toEqual(root);
    expect(filterExplorerEntries(children("src"), "main.rs", () => [])).toEqual([{ name: "main.rs", path: "src/main.rs" }]);
  });
});
describe("workspace file search", () => {
  const files = ["README.md", "src/generate.py", "src/utils/config_loader.py", "configs/base.yaml", "deep/a/b/c/config.yaml"];

  it("finds files in folders that were never expanded, not only at the workspace root", () => {
    expect(searchWorkspaceFiles(files, "config")).toEqual(["src/utils/config_loader.py", "deep/a/b/c/config.yaml", "configs/base.yaml"]);
  });

  it("ranks file-name matches above folder-name matches, ignores case and stays empty without a query", () => {
    expect(searchWorkspaceFiles(files, "CONFIG")[0]).toBe("src/utils/config_loader.py");
    expect(searchWorkspaceFiles(files, "   ")).toEqual([]);
    expect(searchWorkspaceFiles(files, "py", 1)).toEqual(["src/generate.py"]);
  });

  it("accepts wildcards like *.jsonl against file names, and against the whole path when the pattern has a slash", () => {
    const data = ["data/questions.jsonl", "output/run.jsonl.bak", "output/a/b.jsonl", "notes.md"];
    expect(searchWorkspaceFiles(data, "*.jsonl")).toEqual(["data/questions.jsonl", "output/a/b.jsonl"]);
    expect(searchWorkspaceFiles(data, "output/*.jsonl")).toEqual(["output/a/b.jsonl"]);
    expect(searchWorkspaceFiles(data, "b.json?")).toEqual(["output/a/b.jsonl"]);
    expect(searchWorkspaceFiles(data, "jsonl")).toHaveLength(3);
  });
});
