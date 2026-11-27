import type { AgentMessage } from "@/lib/agentStore";
import { activityPreview, humanizeToolName, toolIntentLabel } from "@/components/agent-chat-panel/chat-view/activityPreview";
import { visibleGeminiReasoning } from "@/lib/geminiThought";

export type VoiceStatus = "idle" | "connecting" | "listening" | "transcribing" | "thinking" | "speaking" | "error";

export type VadConfig = { threshold: number; startMs: number; endMs: number; minSpeechMs: number };
export type VadState = { speaking: boolean; aboveSince: number | null; belowSince: number | null; speechStartedAt: number | null };
export type VadEvent = "speech-start" | "speech-end" | "speech-discarded" | null;

export const LISTEN_VAD: VadConfig = { threshold: 0.035, startMs: 120, endMs: 900, minSpeechMs: 350 };
export const BARGE_IN_VAD: VadConfig = { threshold: 0.09, startMs: 280, endMs: 900, minSpeechMs: 0 };

export const INITIAL_VAD: VadState = { speaking: false, aboveSince: null, belowSince: null, speechStartedAt: null };

export function stepVad(state: VadState, level: number, now: number, config: VadConfig): { state: VadState; event: VadEvent } {
  const above = level >= config.threshold;
  if (!state.speaking) {
    if (!above) return { state: { ...state, aboveSince: null }, event: null };
    const aboveSince = state.aboveSince ?? now;
    if (now - aboveSince < config.startMs) return { state: { ...state, aboveSince }, event: null };
    return { state: { speaking: true, aboveSince, belowSince: null, speechStartedAt: aboveSince }, event: "speech-start" };
  }
  if (above) return { state: { ...state, belowSince: null }, event: null };
  const belowSince = state.belowSince ?? now;
  if (now - belowSince < config.endMs) return { state: { ...state, belowSince }, event: null };
  const spoke = belowSince - (state.speechStartedAt ?? belowSince);
  return { state: INITIAL_VAD, event: spoke >= config.minSpeechMs ? "speech-end" : "speech-discarded" };
}

export function rmsLevel(samples: Uint8Array): number {
  let sum = 0;
  for (const sample of samples) {
    const centered = (sample - 128) / 128;
    sum += centered * centered;
  }
  return samples.length ? Math.sqrt(sum / samples.length) : 0;
}

export function speechText(markdown: string): string {
  return markdown
    .replace(/```[\s\S]*?```/g, " (code omitted) ")
    .replace(/`([^`]+)`/g, "$1")
    .replace(/!\[[^\]]*\]\([^)]*\)/g, "")
    .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
    .replace(/^\s{0,3}#{1,6}\s+/gm, "")
    .replace(/^\s*[-*+]\s+/gm, "")
    .replace(/^\s*>\s?/gm, "")
    .replace(/^\s*\|?(\s*:?-{3,}:?\s*\|)+\s*:?-*:?\s*\|?\s*$/gm, "")
    .replace(/\|/g, " ")
    .replace(/-{3,}/g, " ")
    .replace(/(\*\*|__|\*|_|~~)(.+?)\1/g, "$2")
    .replace(/[ \t]+/g, " ")
    .replace(/\n{3,}/g, "\n\n")
    .trim();
}

export function replyAfter(messages: AgentMessage[], sentAtCount: number): AgentMessage | null {
  const replies = messages.slice(sentAtCount).filter((message) => message.role === "assistant" && message.content.trim() && !message.isStreaming);
  return replies.length ? replies[replies.length - 1] : null;
}

export function cleanTranscript(text: string): string {
  return text.replace(/\[[^\]\n]{1,40}\]/g, " ").replace(/\s+/g, " ").trim();
}

export function voiceActivityLabel(messages: AgentMessage[], sentAt: number): string {
  const recent = messages.slice(sentAt);
  const last = recent[recent.length - 1];
  if (!last) return "Thinking…";
  if (last.role === "tool") {
    const label = toolIntentLabel(last.toolArguments ?? "") || humanizeToolName(last.toolName ?? "");
    return label || "Working…";
  }
  if (last.role === "assistant" && last.isStreaming && !last.content.trim()) {
    const reasoning = visibleGeminiReasoning(last.reasoning);
    if (reasoning) return activityPreview(reasoning) || "Thinking…";
  }
  return "Thinking…";
}

export function captionWords(text: string, progress: number | undefined): { words: string[]; spoken: number } {
  const words = text.split(/\s+/).filter(Boolean);
  if (progress === undefined) return { words, spoken: words.length };
  return { words, spoken: Math.min(words.length, Math.ceil(Math.max(0, Math.min(1, progress)) * words.length)) };
}
