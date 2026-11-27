import { AGENT_TOOL, AGENT_TOOL_NAME, type RealtimeEvent } from "./realtimeEvents";

export const GEMINI_INPUT_RATE = 16000;
export const GEMINI_OUTPUT_RATE = 24000;
const GEMINI_LIVE_URL = "wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContentConstrained";

type GeminiServerMessage = {
  setupComplete?: unknown;
  serverContent?: {
    modelTurn?: { parts?: Array<{ inlineData?: { data?: string; mimeType?: string } }> };
    interrupted?: boolean;
    turnComplete?: boolean;
    inputTranscription?: { text?: string };
    outputTranscription?: { text?: string };
  };
  toolCall?: { functionCalls?: Array<{ id?: string; name?: string; args?: unknown }> };
};

export type GeminiTurnState = { responding: boolean; listening: boolean; userText: string };

export function geminiLiveUrl(token: string): string {
  return `${GEMINI_LIVE_URL}?access_token=${encodeURIComponent(token)}`;
}

export function geminiSetup(model: string, voice: string, instructions: string): Record<string, unknown> {
  const { type: _type, ...declaration } = AGENT_TOOL;
  return {
    setup: {
      model: model.startsWith("models/") ? model : `models/${model}`,
      generationConfig: { responseModalities: ["AUDIO"], speechConfig: { voiceConfig: { prebuiltVoiceConfig: { voiceName: voice } } } },
      systemInstruction: { parts: [{ text: instructions }] },
      tools: [{ functionDeclarations: [declaration] }],
      inputAudioTranscription: {},
      outputAudioTranscription: {},
    },
  };
}

export function geminiAudioMessage(base64Pcm: string): Record<string, unknown> {
  return { realtimeInput: { audio: { data: base64Pcm, mimeType: `audio/pcm;rate=${GEMINI_INPUT_RATE}` } } };
}

export function geminiServerEvents(message: GeminiServerMessage, state: GeminiTurnState): RealtimeEvent[] {
  const events: RealtimeEvent[] = [];
  const content = message.serverContent;
  const startResponse = () => {
    if (state.responding) return;
    state.responding = true;
    state.listening = false;
    events.push({ type: "response.created" });
  };
  if (content?.interrupted) {
    state.responding = false;
    events.push({ type: "input_audio_buffer.speech_started" });
  }
  if (content?.inputTranscription?.text) {
    if (!state.listening) {
      state.listening = true;
      state.userText = "";
      if (!content.interrupted) events.push({ type: "input_audio_buffer.speech_started" });
    }
    state.userText += content.inputTranscription.text;
    events.push({ type: "conversation.item.input_audio_transcription.completed", transcript: state.userText });
  }
  for (const part of content?.modelTurn?.parts ?? []) {
    if (part.inlineData?.data && (part.inlineData.mimeType ?? "audio/pcm").startsWith("audio/")) {
      startResponse();
      events.push({ type: "response.output_audio.delta", delta: part.inlineData.data });
    }
  }
  if (content?.outputTranscription?.text) {
    startResponse();
    events.push({ type: "response.output_audio_transcript.delta", delta: content.outputTranscription.text });
  }
  for (const call of message.toolCall?.functionCalls ?? []) {
    if (!call.id || !call.name) continue;
    events.push({ type: "response.function_call_arguments.done", call_id: call.id, name: call.name, arguments: JSON.stringify(call.args ?? {}) });
  }
  if (content?.turnComplete) {
    state.responding = false;
    state.listening = false;
    events.push({ type: "response.done" });
  }
  return events;
}

export function geminiOutgoing(event: Record<string, unknown>): Record<string, unknown> | null {
  if (event.type === "response.create") return null;
  const item = event.item as { type?: string; call_id?: string; output?: string } | undefined;
  if (event.type === "conversation.item.create" && item?.type === "function_call_output" && item.call_id) {
    let response: unknown = { output: item.output };
    try { response = JSON.parse(item.output ?? "{}"); } catch { response = { output: item.output }; }
    return { toolResponse: { functionResponses: [{ id: item.call_id, name: AGENT_TOOL_NAME, response }] } };
  }
  return event;
}
