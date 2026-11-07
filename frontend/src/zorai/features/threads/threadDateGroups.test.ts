import { describe, expect, it } from "vitest";
import { threadDateGroupLabel } from "./threadDateGroups";

describe("thread list date groups", () => {
  const now = new Date(2026, 10, 7, 13, 0).getTime();
  const at = (day: number, hour = 12) => new Date(2026, 10, day, hour).getTime();

  it("groups by calendar day, so last night is Yesterday even if it was only a few hours ago", () => {
    expect(threadDateGroupLabel(at(7, 0), now)).toBe("Today");
    expect(threadDateGroupLabel(at(6, 23), now)).toBe("Yesterday");
    expect(threadDateGroupLabel(at(6, 0), now)).toBe("Yesterday");
  });

  it("puts older threads into wider buckets", () => {
    expect(threadDateGroupLabel(at(2), now)).toBe("Previous 7 days");
    expect(threadDateGroupLabel(new Date(2026, 9, 20).getTime(), now)).toBe("Previous 30 days");
    expect(threadDateGroupLabel(new Date(2026, 5, 1).getTime(), now)).toBe("Older");
  });
});
