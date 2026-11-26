import { describe, expect, it } from "vitest";
import { callHierarchyChildren } from "./lspProviders";

describe("call hierarchy", () => {
  it("shows callers for incoming and callees for outgoing, dropping malformed entries", () => {
    const caller = { name: "main", uri: "file:///repo/src/main.rs", range: { start: { line: 0, character: 0 } } };
    const callee = { name: "parse", uri: "file:///repo/src/lib.rs", range: { start: { line: 4, character: 0 } } };
    expect(callHierarchyChildren([{ from: caller, fromRanges: [] }, { from: null }], "incoming")).toEqual([caller]);
    expect(callHierarchyChildren([{ to: callee, fromRanges: [] }], "outgoing")).toEqual([callee]);
    expect(callHierarchyChildren(null, "incoming")).toEqual([]);
  });
});
