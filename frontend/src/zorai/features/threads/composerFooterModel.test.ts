import { describe, expect, it } from "vitest";
import { effortLabel, footerModelLabel } from "./composerFooterModel";

describe("composer footer labels", () => {
  it("shows the bare model name like Claude Code's footer, shortened when long", () => {
    expect(footerModelLabel("anthropic/claude-opus-5-5")).toBe("claude-opus-5-5");
    expect(footerModelLabel("x".repeat(40))).toHaveLength(28);
    expect(footerModelLabel("")).toBe("Choose model");
  });

  it("names reasoning effort in plain words", () => {
    expect(effortLabel("medium")).toBe("Medium");
    expect(effortLabel("xhigh")).toBe("Extra high");
    expect(effortLabel("")).toBe("None");
  });
});
