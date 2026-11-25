import { describe, expect, it } from "vitest";
import {
  COMPOSER_TEXTAREA_MAX_ROWS,
  COMPOSER_TEXTAREA_MIN_ROWS,
  composerTextareaHeightPx,
} from "./composerTextareaSize";

describe("composerTextareaHeightPx", () => {
  it("grows from 1 row to a 10-row cap so wrapped drafts push the thread up, then shrink back", () => {
    // Why: a fixed 3-row box hides long drafts; unbounded growth covers the
    // message list. The cap is 10 rows of content plus vertical padding.
    const lineHeight = 20;
    const paddingY = 18;
    expect(composerTextareaHeightPx(10, lineHeight, paddingY)).toBe(lineHeight * COMPOSER_TEXTAREA_MIN_ROWS + paddingY);
    expect(composerTextareaHeightPx(140, lineHeight, paddingY)).toBe(140);
    expect(composerTextareaHeightPx(800, lineHeight, paddingY)).toBe(lineHeight * COMPOSER_TEXTAREA_MAX_ROWS + paddingY);
  });
});
