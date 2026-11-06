import { describe, expect, it } from "vitest";
import { composerPanelMaxHeight } from "./useComposerPanelHeight";

describe("composer panel height limit", () => {
  it("grows into the messages area but always leaves some of it visible, so the input never leaves the screen", () => {
    expect(composerPanelMaxHeight({ panelHeight: 400, composerBottom: 992, messagesHeight: 500, viewportHeight: 1000, minHeight: 180 })).toBe(780);
  });

  it("shrinks below the current height when the messages area is gone and the composer is pushed off screen", () => {
    expect(composerPanelMaxHeight({ panelHeight: 700, composerBottom: 1100, messagesHeight: 0, viewportHeight: 1000, minHeight: 180 })).toBe(592);
  });

  it("never goes under the minimum or over 80% of the window", () => {
    expect(composerPanelMaxHeight({ panelHeight: 200, composerBottom: 1400, messagesHeight: 0, viewportHeight: 1000, minHeight: 180 })).toBe(180);
    expect(composerPanelMaxHeight({ panelHeight: 200, composerBottom: 992, messagesHeight: 2000, viewportHeight: 1000, minHeight: 180 })).toBe(800);
  });
});
