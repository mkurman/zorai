import { describe, expect, it } from "vitest";
import type { AgentMessage } from "@/lib/agentStore";
import { INITIAL_VAD, LISTEN_VAD, captionWords, cleanTranscript, replyAfter, rmsLevel, speechText, stepVad, voiceActivityLabel, type VadState } from "./voiceConversation";

function run(levels: Array<[number, number]>) {
  let state: VadState = INITIAL_VAD;
  const events: string[] = [];
  for (const [at, level] of levels) {
    const step = stepVad(state, level, at, LISTEN_VAD);
    state = step.state;
    if (step.event) events.push(`${step.event}@${at}`);
  }
  return events;
}

describe("voice activity detection", () => {
  it("ends a turn only after the speaker has been quiet for a while, so short pauses don't cut them off", () => {
    expect(run([[0, 0.2], [150, 0.2], [600, 0.01], [1000, 0.2], [1200, 0.01], [2000, 0.01], [2200, 0.01]])).toEqual(["speech-start@150", "speech-end@2200"]);
  });

  it("ignores a brief click or cough instead of sending an empty turn", () => {
    expect(run([[0, 0.2], [60, 0.01], [100, 0.01]])).toEqual([]);
    expect(run([[0, 0.2], [130, 0.2], [200, 0.01], [1200, 0.01]])).toEqual(["speech-start@130", "speech-discarded@1200"]);
  });

  it("measures loudness from analyser bytes centred on 128", () => {
    expect(rmsLevel(new Uint8Array([128, 128]))).toBe(0);
    expect(rmsLevel(new Uint8Array([0, 255]))).toBeGreaterThan(0.9);
  });
});

describe("spoken replies", () => {
  it("reads prose aloud without markdown symbols or code listings", () => {
    expect(speechText("## Plan\n- **Run** the `eval`\n```py\nprint(1)\n```\nSee [docs](http://x)."))
      .toBe("Plan\nRun the eval\n (code omitted) \nSee docs.");
  });

  it("turns Markdown tables into plain cells instead of reading out pipes and dashes", () => {
    expect(speechText("| Date | Release |\n|---|---|\n| **Nov 26** | GigaChat 4 |")).toBe("Date Release \n\n Nov 26 GigaChat 4");
  });

  it("speaks the finished assistant reply that came after the user's turn, not an older one", () => {
    const message = (id: string, role: AgentMessage["role"], content: string, isStreaming = false) => ({ id, role, content, isStreaming }) as AgentMessage;
    const messages = [message("a", "assistant", "old"), message("u", "user", "hi"), message("t", "assistant", ""), message("b", "assistant", "new answer")];
    expect(replyAfter(messages, 1)?.id).toBe("b");
    expect(replyAfter([...messages.slice(0, 3), message("b", "assistant", "partial", true)], 1)).toBeNull();
  });
});

describe("voice captions and activity", () => {
  const message = (fields: Partial<AgentMessage>) => ({ id: "m", role: "assistant", content: "", ...fields }) as AgentMessage;

  it("drops speech-to-text sound tags so the agent receives only what was said", () => {
    expect(cleanTranscript("[camera shutter] [typing] [camera shutter] And what about Google?")).toBe("And what about Google?");
  });

  it("shows the running tool's description while the agent works, like the thread does", () => {
    const tool = message({ role: "tool", toolName: "web_search", toolArguments: JSON.stringify({ query: "Gemini release" }), toolStatus: "executing" });
    expect(voiceActivityLabel([message({ role: "user", content: "hi" }), tool], 1)).not.toBe("Thinking…");
    expect(voiceActivityLabel([message({ role: "user", content: "hi" })], 1)).toBe("Thinking…");
  });

  it("lights up words in step with playback progress", () => {
    expect(captionWords("one two three four", 0.5)).toEqual({ words: ["one", "two", "three", "four"], spoken: 2 });
    expect(captionWords("one two", undefined).spoken).toBe(2);
  });
});
