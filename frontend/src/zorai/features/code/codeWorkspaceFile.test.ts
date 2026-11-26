import { describe, expect, it } from "vitest";
import { isWorkspaceFile, workspaceFileFolders } from "./codeWorkspaceFile";

describe("multi-root workspace files", () => {
  it("resolves folders relative to the workspace file and skips the already-open root", () => {
    const content = '{ // team\n "folders": [{ "path": "." }, { "path": "../shared" }, { "path": "/opt/lib" }, { "name": "x" }], }';
    expect(workspaceFileFolders(content, "app.code-workspace", "/work/app")).toEqual(["/work/shared", "/opt/lib"]);
    expect(workspaceFileFolders(content, "ws/all.code-workspace", "/work/app")).toEqual(["/work/app/ws", "/work/app/shared", "/opt/lib"]);
    expect(workspaceFileFolders("nope", "a.code-workspace", "/w")).toEqual([]);
    expect(isWorkspaceFile("team.code-workspace")).toBe(true);
  });
});
