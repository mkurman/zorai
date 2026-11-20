import { describe, expect, it } from "vitest";
import { joinVoiceTranscript, waveformBarLevels } from "./voiceTranscript";

describe("joinVoiceTranscript", () => {
  it("replaces the live text instead of appending it, so each chunk update never duplicates words", () => {
    const base = "Draft:";
    const first = joinVoiceTranscript(base, "run the");
    const second = joinVoiceTranscript(base, "run the tests");
    expect(first).toBe("Draft: run the");
    expect(second).toBe("Draft: run the tests");
  });

  it("keeps the typed text when the transcript is empty", () => {
    expect(joinVoiceTranscript("typed", "  ")).toBe("typed");
    expect(joinVoiceTranscript("", "hello")).toBe("hello");
  });
});

describe("waveformBarLevels", () => {
  it("stays flat for silence and rises for loud samples so the bars follow the mic", () => {
    expect(waveformBarLevels(new Uint8Array(64).fill(128), 4)).toEqual([0, 0, 0, 0]);
    const loud = new Uint8Array(64).fill(128);
    loud[0] = 255;
    expect(waveformBarLevels(loud, 4)[0]).toBe(1);
  });
});
