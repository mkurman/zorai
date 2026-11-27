import type { AgentMessage } from "@/lib/agentStore";
import type { VoiceSession } from "./useBundledVoice";
import { speechText } from "./voiceConversation";

export type RealtimeEvent = { type?: string; delta?: string; transcript?: string; error?: { message?: string } | string; message?: string; call_id?: string; name?: string; arguments?: string };
export type RealtimeEffect = { patch: Partial<VoiceSession>; interrupt?: boolean; audioDelta?: string };

export const REALTIME_INSTRUCTIONS = "You are Zorai's voice assistant. Speak naturally and keep answers short and conversational unless asked for detail. Never read code or URLs character by character.";
export const AGENT_TOOL_NAME = "ask_zorai_agent";
const CONTEXT_MAX_CHARS = 6000;

export const AGENT_TOOL = {
  type: "function",
  name: AGENT_TOOL_NAME,
  description: "Hand a request to the user's Zorai agent, which works in the current conversation thread with tools: web search, files, code, the workspace, memory and long-running tasks. Use it for anything that needs fresh information, actions, the user's files or the ongoing work in this thread. Before calling, tell the user briefly that you're checking. Pass the user's request in full, in their words plus any detail from the conversation.",
  parameters: {
    type: "object",
    properties: { request: { type: "string", description: "The complete request for the agent." } },
    required: ["request"],
  },
};

export function threadContextInstructions(messages: AgentMessage[], maxChars = CONTEXT_MAX_CHARS): string {
  const lines: string[] = [];
  let used = 0;
  for (let index = messages.length - 1; index >= 0; index -= 1) {
    const message = messages[index];
    if ((message.role !== "user" && message.role !== "assistant") || !message.content.trim()) continue;
    const line = `${message.role === "user" ? "User" : "Agent"}: ${speechText(message.content).replace(/\s+/g, " ")}`;
    const clipped = line.length > 1200 ? `${line.slice(0, 1200)}…` : line;
    if (used + clipped.length > maxChars) break;
    lines.unshift(clipped);
    used += clipped.length;
  }
  const history = lines.length ? `\n\nRecent conversation in this thread (oldest first):\n${lines.join("\n")}` : "";
  return `${REALTIME_INSTRUCTIONS} You are the voice of the user's Zorai agent. Answer small talk and simple questions yourself; for anything else call ${AGENT_TOOL_NAME} and then speak its reply in your own words, briefly.${history}`;
}

export function agentFunctionCall(event: RealtimeEvent): { callId: string; request: string } | null {
  if (event.type !== "response.function_call_arguments.done" || event.name !== AGENT_TOOL_NAME || !event.call_id) return null;
  try {
    const request = String(JSON.parse(event.arguments || "{}").request ?? "").trim();
    return request ? { callId: event.call_id, request } : null;
  } catch {
    return null;
  }
}

export function agentFunctionResult(callId: string, reply: string): Array<Record<string, unknown>> {
  return [
    { type: "conversation.item.create", item: { type: "function_call_output", call_id: callId, output: JSON.stringify({ reply: speechText(reply).slice(0, 6000) }) } },
    { type: "response.create" },
  ];
}

export function realtimeEffect(event: RealtimeEvent, session: VoiceSession): RealtimeEffect | null {
  switch (event.type) {
    case "input_audio_buffer.speech_started":
      return { patch: { status: "listening" }, interrupt: true };
    case "input_audio_buffer.speech_stopped":
      return { patch: { status: "thinking" } };
    case "conversation.item.input_audio_transcription.completed":
      return { patch: { userText: (event.transcript ?? "").trim() } };
    case "response.created":
      return { patch: { status: "thinking", replyText: "" } };
    case "response.output_audio_transcript.delta":
    case "response.audio_transcript.delta":
      return { patch: { status: "speaking", replyText: session.replyText + (event.delta ?? "") } };
    case "response.output_audio.delta":
    case "response.audio.delta":
      return { patch: session.status === "speaking" ? {} : { status: "speaking" }, audioDelta: event.delta ?? "" };
    case "response.done":
      return { patch: { status: "listening" } };
    case "error": {
      const message = typeof event.error === "string" ? event.error : event.error?.message ?? event.message ?? "Realtime session error.";
      return { patch: { error: message } };
    }
    default:
      return null;
  }
}

export function floatToPcm16(samples: Float32Array): Int16Array {
  const out = new Int16Array(samples.length);
  for (let index = 0; index < samples.length; index += 1) {
    const clamped = Math.max(-1, Math.min(1, samples[index]));
    out[index] = clamped < 0 ? clamped * 0x8000 : clamped * 0x7fff;
  }
  return out;
}

export function pcm16ToFloat(pcm: Int16Array): Float32Array<ArrayBuffer> {
  const out = new Float32Array(pcm.length);
  for (let index = 0; index < pcm.length; index += 1) out[index] = pcm[index] / 0x8000;
  return out;
}

export function bytesToBase64(bytes: Uint8Array): string {
  let binary = "";
  for (let index = 0; index < bytes.length; index += 0x8000) binary += String.fromCharCode(...bytes.subarray(index, index + 0x8000));
  return btoa(binary);
}

export function base64ToPcm16(base64: string): Int16Array {
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) bytes[index] = binary.charCodeAt(index);
  return new Int16Array(bytes.buffer, 0, Math.floor(bytes.length / 2));
}

export const ESTIMATED_WORDS_PER_SECOND = 2.7;

export function estimatedSpeechProgress(text: string, startedAt: number | null, now: number): number | undefined {
  if (startedAt === null) return undefined;
  const words = speechText(text).split(/\s+/).filter(Boolean).length;
  if (!words) return undefined;
  return Math.min(1, ((now - startedAt) / 1000) * ESTIMATED_WORDS_PER_SECOND / words);
}
