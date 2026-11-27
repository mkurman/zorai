import type { AgentMessage } from "@/lib/agentStore";
import { TOOL_NAMES } from "@/lib/agentTools/toolNames";

export function resolveAudioPlaybackSource(result: unknown): string {
  if (typeof result === "string") {
    return result.trim();
  }
  if (!result || typeof result !== "object") {
    return "";
  }

  if (typeof (result as { media_url?: unknown }).media_url === "string") {
    return (result as { media_url: string }).media_url;
  }
  if (typeof (result as { file_url?: unknown }).file_url === "string") {
    return (result as { file_url: string }).file_url;
  }
  if (typeof (result as { url?: unknown }).url === "string") {
    return (result as { url: string }).url;
  }
  if (typeof (result as { path?: unknown }).path === "string") {
    return (result as { path: string }).path;
  }
  return "";
}

export function buildTtsCacheKey(
  provider: string,
  model: string,
  voice: string,
  text: string,
): string {
  return [provider, model, voice, text].join("");
}

export function resolveToolResultAudioPlaybackSource(content: string): string {
  if (!content.trim()) {
    return "";
  }
  try {
    return resolveAudioPlaybackSource(JSON.parse(content));
  } catch {
    return resolveAudioPlaybackSource(content);
  }
}

export function findLatestAgentToolTextToSpeechPlayback(
  messages: AgentMessage[],
  lastHandledToolCallId?: string | null,
): { toolCallId: string; source: string } | null {
  const latestToolResult = [...messages]
    .reverse()
    .find((message) =>
      message.role === "tool"
      && message.toolName === TOOL_NAMES.textToSpeech
      && message.toolStatus === "done",
    );

  if (!latestToolResult) {
    return null;
  }

  const toolCallId = latestToolResult.toolCallId || latestToolResult.id;
  if (!toolCallId || toolCallId === lastHandledToolCallId) {
    return null;
  }

  const source = resolveToolResultAudioPlaybackSource(latestToolResult.content);
  if (!source) {
    return null;
  }

  return { toolCallId, source };
}
