import { describe, expect, it } from "vitest";
import { advanceToolTitleFrame } from "./ToolEventList";

describe("advanceToolTitleFrame", () => {
  it("keeps the previous working tool mounted so it can leave upward while the next enters from below", () => {
    expect(advanceToolTitleFrame({ current: "Read File", previous: null }, "Python", false)).toEqual({
      current: "Python",
      previous: "Read File",
    });
  });

  it("swaps the working tool immediately when motion is reduced", () => {
    expect(advanceToolTitleFrame({ current: "Read File", previous: null }, "Python", true)).toEqual({
      current: "Python",
      previous: null,
    });
  });

  it("does not restart the rise when the working tool name stays the same", () => {
    const frame = { current: "Python", previous: null };
    expect(advanceToolTitleFrame(frame, "Python", false)).toBe(frame);
  });
});
