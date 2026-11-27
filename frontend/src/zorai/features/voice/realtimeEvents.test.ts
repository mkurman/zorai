import { describe, expect, it } from "vitest";
import type { VoiceSession } from "./useBundledVoice";
import type { AgentMessage } from "@/lib/agentStore";
import { AGENT_TOOL_NAME, agentFunctionCall, agentFunctionResult, estimatedSpeechProgress, base64ToPcm16, bytesToBase64, floatToPcm16, pcm16ToFloat, realtimeEffect, threadContextInstructions } from "./realtimeEvents";

const session: VoiceSession = { status: "listening", level: 0, userText: "", replyText: "Hel", error: null, muted: false };

describe("realtime events", () => {
  it("interrupts playback the moment the user starts talking", () => {
    expect(realtimeEffect({ type: "input_audio_buffer.speech_started" }, session)).toEqual({ patch: { status: "listening" }, interrupt: true });
  });

  it("builds captions from transcript deltas under both OpenAI and xAI event names", () => {
    expect(realtimeEffect({ type: "response.output_audio_transcript.delta", delta: "lo" }, session)?.patch.replyText).toBe("Hello");
    expect(realtimeEffect({ type: "response.audio_transcript.delta", delta: "!" }, session)?.patch.replyText).toBe("Hel!");
    expect(realtimeEffect({ type: "conversation.item.input_audio_transcription.completed", transcript: " hi " }, session)?.patch.userText).toBe("hi");
  });

  it("goes back to listening once the voice finishes a response", () => {
    expect(realtimeEffect({ type: "response.done" }, { ...session, status: "speaking" })?.patch.status).toBe("listening");
  });

  it("hands audio chunks to the player and surfaces provider errors", () => {
    expect(realtimeEffect({ type: "response.output_audio.delta", delta: "AAA=" }, session)).toEqual({ patch: { status: "speaking" }, audioDelta: "AAA=" });
    expect(realtimeEffect({ type: "error", error: { message: "bad key" } }, session)?.patch.error).toBe("bad key");
    expect(realtimeEffect({ type: "rate_limits.updated" }, session)).toBeNull();
  });

  it("round-trips 16-bit PCM through base64 for the xAI socket", () => {
    const pcm = floatToPcm16(new Float32Array([0, 0.5, -1, 1.5]));
    expect(Array.from(pcm)).toEqual([0, 16383, -32768, 32767]);
    const decoded = base64ToPcm16(bytesToBase64(new Uint8Array(pcm.buffer)));
    expect(Array.from(decoded)).toEqual(Array.from(pcm));
    expect(pcm16ToFloat(new Int16Array([-32768]))[0]).toBe(-1);
  });
});

describe("realtime voice ↔ agent bridge", () => {
  const message = (role: AgentMessage["role"], content: string) => ({ id: content, role, content }) as AgentMessage;

  it("gives the voice model the thread's recent turns, newest kept when trimming", () => {
    const text = threadContextInstructions([message("user", "Plan the **eval**"), message("tool", "ignored"), message("assistant", "Use 200 prompts")], 1000);
    expect(text).toContain("User: Plan the eval\nAgent: Use 200 prompts");
    expect(text).not.toContain("ignored");
    const trimmed = threadContextInstructions([message("user", "old ".repeat(100)), message("user", "newest")], 40);
    expect(trimmed).toContain("User: newest");
    expect(trimmed).not.toContain("old old");
  });

  it("recognises the agent hand-off call and ignores other tools or empty requests", () => {
    expect(agentFunctionCall({ type: "response.function_call_arguments.done", name: AGENT_TOOL_NAME, call_id: "c1", arguments: JSON.stringify({ request: "check the news" }) })).toEqual({ callId: "c1", request: "check the news" });
    expect(agentFunctionCall({ type: "response.function_call_arguments.done", name: "other", call_id: "c1", arguments: "{}" })).toBeNull();
    expect(agentFunctionCall({ type: "response.function_call_arguments.done", name: AGENT_TOOL_NAME, call_id: "c1", arguments: "{bad" })).toBeNull();
  });

  it("returns the agent's reply as plain text and asks the voice model to speak it", () => {
    const [item, respond] = agentFunctionResult("c1", "**Done**: saved `x`");
    expect(item).toEqual({ type: "conversation.item.create", item: { type: "function_call_output", call_id: "c1", output: JSON.stringify({ reply: "Done: saved x" }) } });
    expect(respond).toEqual({ type: "response.create" });
  });
});

describe("caption pacing without playback timing", () => {
  it("reveals the reply at a natural speaking pace instead of all at once", () => {
    const text = "one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen sixteen seventeen eighteen nineteen twenty twenty-one twenty-two twenty-three twenty-four twenty-five twenty-six twenty-seven";
    expect(estimatedSpeechProgress(text, null, 0)).toBeUndefined();
    expect(estimatedSpeechProgress(text, 0, 5000)).toBeCloseTo(0.5, 1);
    expect(estimatedSpeechProgress(text, 0, 60000)).toBe(1);
  });
});
