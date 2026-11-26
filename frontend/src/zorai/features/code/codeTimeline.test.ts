import { describe, expect, it } from "vitest";
import { mergeTimeline } from "./codeTimeline";

describe("file timeline", () => {
  it("interleaves local saves and commits newest first, so uncommitted work never hides behind history", () => {
    const entries = mergeTimeline(
      [{ id: "s2", at: Date.parse("2026-11-26T10:00:00Z"), size: 1 }, { id: "s1", at: Date.parse("2026-11-20T10:00:00Z"), size: 1 }],
      [{ hash: "c1", shortHash: "c1", author: "Ada", date: "2026-11-24T10:00:00Z", subject: "Refactor" }],
    );
    expect(entries.map((entry) => `${entry.kind}:${entry.id}`)).toEqual(["local:s2", "commit:c1", "local:s1"]);
  });
});
