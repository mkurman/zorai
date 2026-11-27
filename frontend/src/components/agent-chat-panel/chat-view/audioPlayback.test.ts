import { describe, expect, it } from "vitest";
import type { AgentMessage } from "@/lib/agentStore";
import {
  buildTtsCacheKey,
  findLatestAgentToolTextToSpeechPlayback,
  resolveAudioPlaybackSource,
  resolveToolResultAudioPlaybackSource,
} from "./audioPlayback";

describe("buildTtsCacheKey", () => {
  it("is stable for identical inputs", () => {
    expect(buildTtsCacheKey("openrouter", "x-ai/grok-voice-tts-1.0", "eve", "hello there")).toBe(
      buildTtsCacheKey("openrouter", "x-ai/grok-voice-tts-1.0", "eve", "hello there"),
    );
  });

  it("changes when voice, model, provider, or text differ", () => {
    const base = buildTtsCacheKey("openrouter", "x-ai/grok-voice-tts-1.0", "eve", "hello there");
    expect(buildTtsCacheKey("openrouter", "x-ai/grok-voice-tts-1.0", "ara", "hello there")).not.toBe(base);
    expect(buildTtsCacheKey("openrouter", "x-ai/grok-voice-tts-1.0", "eve", "different")).not.toBe(base);
    expect(buildTtsCacheKey("openai", "x-ai/grok-voice-tts-1.0", "eve", "hello there")).not.toBe(base);
    expect(buildTtsCacheKey("openrouter", "openai/gpt-4o-mini-tts", "eve", "hello there")).not.toBe(base);
  });
});

function makeToolMessage(partial: Partial<AgentMessage>): AgentMessage {
  return {
    id: partial.id ?? "message-1",
    threadId: partial.threadId ?? "thread-1",
    createdAt: partial.createdAt ?? 1,
    role: partial.role ?? "tool",
    content: partial.content ?? "",
    inputTokens: 0,
    outputTokens: 0,
    totalTokens: 0,
    isCompactionSummary: false,
    ...partial,
  };
}

describe("audioPlayback helpers", () => {
  it("prefers the zorai-media URL because a dev-server page cannot load file:// audio", () => {
    expect(resolveAudioPlaybackSource({ media_url: "zorai-media://local/tmp/speech.mp3", file_url: "file:///tmp/speech.mp3", path: "/tmp/speech.mp3" })).toBe("zorai-media://local/tmp/speech.mp3");
  });

  it("prefers file_url before url and path", () => {
    expect(resolveAudioPlaybackSource({
      file_url: "file:///tmp/speech.mp3",
      url: "https://example.com/speech.mp3",
      path: "/tmp/speech.mp3",
    })).toBe("file:///tmp/speech.mp3");
  });

  it("parses a playable source from text_to_speech tool JSON", () => {
    expect(resolveToolResultAudioPlaybackSource(JSON.stringify({
      path: "/tmp/tool-speech.mp3",
    }))).toBe("/tmp/tool-speech.mp3");
  });

  it("finds the latest completed text_to_speech tool result that has audio", () => {
    const playback = findLatestAgentToolTextToSpeechPlayback([
      makeToolMessage({
        id: "tool-1",
        toolCallId: "call-1",
        toolName: "bash_command",
        toolStatus: "done",
        content: "/repo",
      }),
      makeToolMessage({
        id: "tool-2",
        toolCallId: "call-2",
        toolName: "text_to_speech",
        toolStatus: "done",
        content: JSON.stringify({ path: "/tmp/tool-speech.mp3" }),
      }),
    ]);

    expect(playback).toEqual({
      toolCallId: "call-2",
      source: "/tmp/tool-speech.mp3",
    });
  });

  it("does not replay the latest handled text_to_speech tool result", () => {
    const messages = [
      makeToolMessage({
        id: "tool-2",
        toolCallId: "call-2",
        toolName: "text_to_speech",
        toolStatus: "done",
        content: JSON.stringify({ path: "/tmp/tool-speech.mp3" }),
      }),
    ];

    expect(findLatestAgentToolTextToSpeechPlayback(messages, "call-2")).toBeNull();
  });
});
