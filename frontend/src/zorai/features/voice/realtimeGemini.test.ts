import { describe, expect, it } from "vitest";
import { AGENT_TOOL_NAME, agentFunctionResult } from "./realtimeEvents";
import { geminiOutgoing, geminiServerEvents, geminiSetup, type GeminiTurnState } from "./realtimeGemini";
import { isRealtimeModel, realtimeModelOptions } from "./realtimeCatalog";

const fresh = (): GeminiTurnState => ({ responding: false, listening: false, userText: "" });

describe("Gemini Live bridge", () => {
  it("sets up an audio-only session with the voice, thread context and the agent hand-off tool", () => {
    const setup = geminiSetup("gemini-3.8-live", "Kore", "context") as { setup: Record<string, any> };
    expect(setup.setup.model).toBe("models/gemini-3.8-live");
    expect(setup.setup.generationConfig.responseModalities).toEqual(["AUDIO"]);
    expect(setup.setup.generationConfig.speechConfig.voiceConfig.prebuiltVoiceConfig.voiceName).toBe("Kore");
    expect(setup.setup.tools[0].functionDeclarations[0].name).toBe(AGENT_TOOL_NAME);
    expect(setup.setup.tools[0].functionDeclarations[0].type).toBeUndefined();
  });

  it("translates a spoken turn into the same events the OpenAI and xAI engines produce", () => {
    const state = fresh();
    const types = (message: object) => geminiServerEvents(message, state).map((event) => event.type);
    expect(types({ serverContent: { inputTranscription: { text: "what's " } } })).toEqual(["input_audio_buffer.speech_started", "conversation.item.input_audio_transcription.completed"]);
    expect(geminiServerEvents({ serverContent: { inputTranscription: { text: "new?" } } }, state)[0].transcript).toBe("what's new?");
    expect(types({ serverContent: { modelTurn: { parts: [{ inlineData: { data: "AAA=", mimeType: "audio/pcm;rate=24000" } }] }, outputTranscription: { text: "Hi" } } }))
      .toEqual(["response.created", "response.output_audio.delta", "response.output_audio_transcript.delta"]);
    expect(types({ serverContent: { turnComplete: true } })).toEqual(["response.done"]);
  });

  it("turns barge-in and tool calls into interrupt and hand-off events", () => {
    const state = { ...fresh(), responding: true };
    expect(geminiServerEvents({ serverContent: { interrupted: true } }, state).map((event) => event.type)).toEqual(["input_audio_buffer.speech_started"]);
    const [call] = geminiServerEvents({ toolCall: { functionCalls: [{ id: "f1", name: AGENT_TOOL_NAME, args: { request: "check mail" } }] } }, state);
    expect(call).toEqual({ type: "response.function_call_arguments.done", call_id: "f1", name: AGENT_TOOL_NAME, arguments: JSON.stringify({ request: "check mail" }) });
  });

  it("sends the agent's reply back as a Gemini toolResponse and drops the OpenAI-only continue event", () => {
    const [item, respond] = agentFunctionResult("f1", "Done");
    expect(geminiOutgoing(item)).toEqual({ toolResponse: { functionResponses: [{ id: "f1", name: AGENT_TOOL_NAME, response: { reply: "Done" } }] } });
    expect(geminiOutgoing(respond)).toBeNull();
  });
});

describe("realtime model picker", () => {
  it("keeps only speech-to-speech models from a provider's full model list, plus the known defaults", () => {
    expect(isRealtimeModel("openai", "gpt-realtime-2.1")).toBe(true);
    expect(isRealtimeModel("openai", "gpt-6.2-sol")).toBe(false);
    expect(isRealtimeModel("gemini", "models/gemini-3.8-live")).toBe(true);
    expect(realtimeModelOptions("gemini", ["models/gemini-4-neon", "models/gemini-3.9-live-preview"], "custom-live")).toEqual([
      "gemini-3.8-live", "gemini-3.8-live-extended-thinking", "gemini-3.1-flash-live-preview", "gemini-3.9-live-preview", "custom-live",
    ]);
  });
});
