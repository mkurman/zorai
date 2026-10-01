import { describe, expect, it } from "vitest";
import { rememberOpenSessionId } from "./openSessionTabs";

describe("open session tabs", () => {
  it("appends a new session and leaves the existing order in place when one is selected again", () => {
    const opened = rememberOpenSessionId(["a", "b"], "c");
    expect(opened).toEqual(["a", "b", "c"]);
    expect(rememberOpenSessionId(opened, "b")).toBe(opened);
    expect(rememberOpenSessionId(["a", "b", "c"], "b")).toEqual(["a", "b", "c"]);
  });
});