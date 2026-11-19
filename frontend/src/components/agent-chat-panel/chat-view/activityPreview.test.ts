import { describe, expect, it } from "vitest";
import { activityPreview, formatActivityElapsed, toolIntentLabel } from "./activityPreview";

describe("activityPreview", () => {
  it("keeps short text whole so a model's intent reads exactly as written", () => {
    expect(activityPreview("Running unit tests now")).toBe("Running unit tests now");
  });

  it("cuts long text to six words plus an ellipsis so headers stay one line", () => {
    expect(activityPreview("one two three four five six seven eight")).toBe("one two three four five six…");
  });

  it("drops markdown punctuation so reasoning previews read as plain words", () => {
    expect(activityPreview("**Planning** the `fix`\n\n# next")).toBe("Planning the fix next");
  });
});

describe("toolIntentLabel", () => {
  it("shows the model-provided intent instead of the raw tool name", () => {
    expect(toolIntentLabel('{"path":"a.rs","intent":"Reading the parser"}')).toBe("Reading the parser");
  });

  it("decodes escaped characters inside the intent", () => {
    expect(toolIntentLabel('{"intent":"Checking \\"main\\" branch"}')).toBe('Checking "main" branch');
  });

  it("ignores intent-looking text inside other string arguments", () => {
    expect(toolIntentLabel('{"content":"{\\"intent\\":\\"fake\\"}"}')).toBe("");
  });

  it("falls back to empty when the model sent no intent", () => {
    expect(toolIntentLabel('{"path":"a.rs"}')).toBe("");
    expect(toolIntentLabel("not json")).toBe("");
  });
});

describe("formatActivityElapsed", () => {
  it("formats seconds, minutes and hours compactly", () => {
    expect(formatActivityElapsed(12_400)).toBe("12s");
    expect(formatActivityElapsed(65_000)).toBe("1m 5s");
    expect(formatActivityElapsed(3_720_000)).toBe("1h 2m");
  });
});
