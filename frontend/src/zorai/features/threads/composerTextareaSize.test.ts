import { describe, expect, it, vi } from "vitest";
import {
  COMPOSER_TEXTAREA_MAX_ROWS,
  COMPOSER_TEXTAREA_MIN_ROWS,
  applyComposerTextareaSize,
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

describe("applyComposerTextareaSize", () => {
  it("sizes an empty composer to its placeholder so a long Rarog next-reply suggestion is fully visible", () => {
    vi.stubGlobal("window", { getComputedStyle: () => ({ lineHeight: "20px", paddingTop: "9px", paddingBottom: "9px" }) });
    const measured: string[] = [];
    const el = {
      value: "",
      placeholder: "a long suggested follow-up",
      style: { height: "" },
      get scrollHeight() { measured.push(this.value); return this.value ? 98 : 38; },
    };
    applyComposerTextareaSize(el as unknown as HTMLTextAreaElement);
    vi.unstubAllGlobals();
    expect(measured).toEqual(["a long suggested follow-up"]);
    expect(el.value).toBe("");
    expect(el.style.height).toBe("98px");
  });
});
