import { describe, expect, it } from "vitest";
import { parseToolSandbox } from "./toolSandbox";

describe("tool sandbox marker", () => {
  it("shows the operator a command was confined and hides the daemon's marker from the output", () => {
    expect(parseToolSandbox("[sandboxed: bubblewrap, network blocked]\nexit 0\nhello")).toEqual({
      label: "bubblewrap, network blocked",
      body: "exit 0\nhello",
    });
  });

  it("never claims a sandbox for ordinary output that merely mentions one", () => {
    expect(parseToolSandbox("exit 0\n[sandboxed: bubblewrap, network blocked]")).toBeNull();
    expect(parseToolSandbox("ran without isolation")).toBeNull();
  });
});
