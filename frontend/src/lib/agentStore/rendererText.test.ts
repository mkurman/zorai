import { describe, expect, it } from "vitest";
import { boundRendererText, MAX_RENDERER_TEXT_CHARS } from "./rendererText";

describe("boundRendererText", () => {
  it("keeps a normal tool result intact", () => {
    expect(boundRendererText("exit 0")).toBe("exit 0");
  });

  it("drops the tail of a multi-megabyte tool payload before it enters the renderer heap", () => {
    const payload = "x".repeat(MAX_RENDERER_TEXT_CHARS + 50_000);
    const bounded = boundRendererText(payload);
    expect(bounded.length).toBeLessThan(payload.length);
    expect(bounded.startsWith("x".repeat(MAX_RENDERER_TEXT_CHARS))).toBe(true);
    expect(bounded.endsWith("memory limit]")).toBe(true);
  });
});
