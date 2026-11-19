import { describe, expect, it } from "vitest";
import { modelModalityKey, nextModelModalities } from "./CustomModelModalities";
import { normalizeModelModalities } from "@/lib/agentStore/settings";

describe("custom model modalities", () => {
  const key = modelModalityKey("openrouter", "qwen/qwen3-vl-custom");

  it("uses the daemon's provider::model key so the daemon capability lookup sees the same entry", () => {
    expect(key).toBe("openrouter::qwen/qwen3-vl-custom");
  });

  it("always keeps text and stores modalities in a stable order", () => {
    const withVideo = nextModelModalities({}, key, "video", true);
    expect(nextModelModalities(withVideo, key, "image", true)[key]).toEqual(["text", "image", "video"]);
  });

  it("drops the entry once only text is left, so the model falls back to the default text-only behaviour", () => {
    const withAudio = nextModelModalities({ other: ["text", "image"] }, key, "audio", true);
    expect(nextModelModalities(withAudio, key, "audio", false)).toEqual({ other: ["text", "image"] });
  });

  it("ignores malformed daemon entries when hydrating", () => {
    expect(normalizeModelModalities({ [key]: ["video", "bogus"], nokey: ["image"], bad: "x" })).toEqual({ [key]: ["text", "video"] });
  });
});
