import { describe, expect, it } from "vitest";
import { computeLineHunks } from "./codeInlineEdit";
import { blameText, mapLineToBlamed, relativeTime } from "./codeGitBlame";

describe("inline git blame", () => {
  it("follows lines that moved because of unsaved edits and reports edited lines as uncommitted", () => {
    const hunks = computeLineHunks("a\nb\nc\nd", "a\nNEW\nb\nc\nd");
    expect(mapLineToBlamed(hunks, 1)).toBe(1);
    expect(mapLineToBlamed(hunks, 2)).toBeNull();
    expect(mapLineToBlamed(hunks, 4)).toBe(3);
    expect(mapLineToBlamed(computeLineHunks("a\nb\nc", "a\nc"), 2)).toBe(3);
  });

  it("reads like a sentence a reviewer would say", () => {
    const now = Date.UTC(2026, 10, 26);
    expect(relativeTime(now - 3 * 86400000, now)).toBe("3 days ago");
    expect(relativeTime(now - 30000, now)).toBe("just now");
    expect(blameText({ hash: "a".repeat(40), author: "Ada", time: now - 3600000, summary: "Fix parser", uncommitted: false }, now)).toBe("Ada, 1 hour ago · Fix parser");
    expect(blameText(null, now)).toBe("You · Uncommitted change");
  });
});
