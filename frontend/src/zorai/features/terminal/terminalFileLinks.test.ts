import { describe, expect, it } from "vitest";
import { findTerminalFileLinks, workspaceRelativeLinkPath } from "./terminalFileLinks";

const only = (text: string) => findTerminalFileLinks(text).map(({ path, line, column }) => `${path}:${line}:${column}`);

describe("terminal file:line links", () => {
  it("recognises the error locations compilers and test runners actually print", () => {
    expect(only("  --> crates/zorai-daemon/src/agent/code_fix.rs:101:13")).toEqual(["crates/zorai-daemon/src/agent/code_fix.rs:101:13"]);
    expect(only("src/App.tsx(12,5): error TS2304: Cannot find name 'x'.")).toEqual(["src/App.tsx:12:5"]);
    expect(only('  File "scripts/build.py", line 42, in main')).toEqual(["scripts/build.py:42:1"]);
    expect(only("    at render (/mnt/e/gitlab/it/zorai/frontend/src/main.tsx:8:3)")).toEqual(["/mnt/e/gitlab/it/zorai/frontend/src/main.tsx:8:3"]);
    expect(only("--- FAIL: TestX (0.00s)\n    handler_test.go:27: bad")).toEqual(["handler_test.go:27:1"]);
  });

  it("leaves URLs and plain text alone so dev-server addresses stay normal web links", () => {
    expect(only("Local: http://localhost:5173/ and https://example.com/a.js:10")).toEqual([]);
    expect(only("Compiled 12 files in 3.2s")).toEqual([]);
  });

  it("only opens files inside the workspace, resolving relative and absolute paths", () => {
    expect(workspaceRelativeLinkPath("/repo/src/a.ts", "/repo/")).toBe("src/a.ts");
    expect(workspaceRelativeLinkPath("./src/../lib/b.ts", "/repo")).toBe("lib/b.ts");
    expect(workspaceRelativeLinkPath("/elsewhere/c.ts", "/repo")).toBeNull();
    expect(workspaceRelativeLinkPath("../outside.ts", "/repo")).toBeNull();
  });
});
