import { describe, expect, it } from "vitest";
import { filterExplorerEntries } from "./workspaceExplorerFilter";

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