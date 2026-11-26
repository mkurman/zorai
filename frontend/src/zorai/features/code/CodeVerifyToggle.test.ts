import { describe, expect, it } from "vitest";
import { withRoot } from "./CodeVerifyToggle";

describe("per-workspace opt-in lists", () => {
  it("adds or removes only this workspace and never duplicates it", () => {
    expect(withRoot(["/a"], "/b", true)).toEqual(["/a", "/b"]);
    expect(withRoot(["/a", "/b"], "/b", true)).toEqual(["/a", "/b"]);
    expect(withRoot(["/a", "/b"], "/a", false)).toEqual(["/b"]);
  });
});
