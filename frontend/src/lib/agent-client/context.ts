import type {
  AgentMessage,
  AgentProviderId,
  AgentThread,
  ApiTransportMode,
} from "../agentStore";
import {
  getDefaultApiTransport,
  getSupportedApiTransports,
  providerSupportsResponseContinuity,
} from "../agentStore";
import { resolveCompactionTargetTokens } from "../agentCompactionTarget.ts";
import type {
  ApiChatMessage,
  ContextCompactionSettings,
  PreparedOpenAIRequest,
} from "./types";
import { APPROX_CHARS_PER_TOKEN, MIN_CONTEXT_TARGET_TOKENS } from "./types";
import {
  countUnicodeScalars,
  pinnedMessageBudgetChars,
} from "./pinnedMessageBudget";

export function messagesToApiFormat(messages: AgentMessage[]): ApiChatMessage[] {
  const announcedToolCalls = new Set<string>();
  const emittedToolResults = new Set<string>();
  const apiMessages: ApiChatMessage[] = [];
  for (const m of messages) {
    if (m.isCompactionSummary) continue;
    if (!(m.role === "user" || m.role === "assistant" || m.role === "tool")) {
      continue;
    }
    if (
      m.role === "tool" &&
      !(m.toolCallId && (m.toolStatus === "done" || m.toolStatus === "error"))
    ) {
      continue;
    }

    if (m.role === "assistant" && Array.isArray(m.toolCalls)) {
      for (const toolCall of m.toolCalls) {
        if (toolCall.id?.trim()) {
          announcedToolCalls.add(toolCall.id);
        }
      }
    }

    if (m.role === "tool") {
      if (
        !m.toolCallId ||
        !announcedToolCalls.has(m.toolCallId) ||
        emittedToolResults.has(m.toolCallId)
      ) {
        continue;
      }
      emittedToolResults.add(m.toolCallId);
      apiMessages.push({
        role: "tool",
        content: m.content,
        tool_call_id: m.toolCallId,
        name: m.toolName,
      });
      continue;
    }

    apiMessages.push({
      role: m.role,
      content: m.content,
      reasoning: m.role === "assistant" ? m.reasoning : undefined,
      tool_calls: m.role === "assistant" ? m.toolCalls : undefined,
    });
  }
  return apiMessages;
}

export function buildApiMessagesForRequest(
  messages: AgentMessage[],
  settings: ContextCompactionSettings,
): ApiChatMessage[] {
  const compacted = compactMessagesForRequest(messages, settings);
  const requestMessages = compacted.some((message) =>
    message.content.startsWith("[Compacted earlier context]"),
  )
    ? appendPinnedMessagesAfterCompactionArtifact(compacted, messages, settings)
    : compacted;
  return messagesToApiFormat(requestMessages);
}

export function prepareOpenAIRequest(
  messages: AgentMessage[],
  settings: ContextCompactionSettings,
  provider: AgentProviderId,
  model: string,
  requestedTransport: ApiTransportMode,
  auth_source?: "api_key" | "chatgpt_subscription" | "github_copilot",
  assistant_id?: string,
  thread?: Pick<
    AgentThread,
    | "id"
    | "upstreamThreadId"
    | "upstreamTransport"
    | "upstreamProvider"
    | "upstreamModel"
    | "upstreamAssistantId"
  >,
): PreparedOpenAIRequest {
  const isChatGptSubscription = provider === "openai" &&
    auth_source === "chatgpt_subscription";
  let selectedTransport = getSupportedApiTransports(provider).includes(
    requestedTransport,
  )
    ? requestedTransport
    : getDefaultApiTransport(provider);
  if (isChatGptSubscription) {
    selectedTransport = "responses";
  }
  const compacted = compactMessagesForRequest(messages, settings);
  const compactionActive =
    compacted.length !== messages.length ||
    compacted.some((message) =>
      message.content.startsWith("[Compacted earlier context]"),
    );
  const requestMessages = compacted.some((message) =>
    message.content.startsWith("[Compacted earlier context]"),
  )
    ? appendPinnedMessagesAfterCompactionArtifact(compacted, messages, settings)
    : compacted;

  if (selectedTransport === "native_assistant" && assistant_id?.trim()) {
    const latestUserMessage = [...messages]
      .reverse()
      .find((message) => message.role === "user" && !message.isCompactionSummary);
    if (latestUserMessage) {
      return {
        messages: messagesToApiFormat([latestUserMessage]),
        transport: "native_assistant",
        upstreamThreadId:
          thread?.upstreamTransport === "native_assistant" &&
          thread.upstreamProvider === provider &&
          thread.upstreamModel === model &&
          thread.upstreamAssistantId === assistant_id
            ? thread.upstreamThreadId ?? undefined
            : undefined,
      };
    }
  }

  if (selectedTransport === "responses") {
    if (!isChatGptSubscription && !compactionActive && providerSupportsResponseContinuity(provider)) {
      const responseAnchorIndex = [...messages.keys()].reverse().find((index) => {
        const message = messages[index];
        return (
          message.role === "assistant" &&
          typeof message.responseId === "string" &&
          message.provider === provider &&
          message.model === model &&
          message.api_transport === "responses"
        );
      });

      if (responseAnchorIndex !== undefined) {
        const trailingMessages = messagesToApiFormat(
          messages.slice(responseAnchorIndex + 1),
        );
        if (trailingMessages.length > 0) {
          return {
            messages: trailingMessages,
            transport: "responses",
            previousResponseId: messages[responseAnchorIndex]?.responseId,
          };
        }
      }
    }

    return {
      messages: messagesToApiFormat(requestMessages),
      transport: "responses",
      upstreamThreadId: isChatGptSubscription
        ? thread?.upstreamThreadId ?? thread?.id
        : undefined,
    };
  }

  if (
    provider === "gemini" &&
    !compactionActive &&
    providerSupportsResponseContinuity(provider)
  ) {
    const responseAnchorIndex = [...requestMessages.keys()].reverse().find((index) => {
      const message = requestMessages[index];
      return (
        message.role === "assistant" &&
        typeof message.responseId === "string" &&
        message.responseId.trim().length > 0 &&
        message.provider === provider &&
        message.model === model
      );
    });
    if (responseAnchorIndex !== undefined) {
      const trailingMessages = messagesToApiFormat(
        requestMessages.slice(responseAnchorIndex + 1),
      );
      if (trailingMessages.length > 0) {
        return {
          messages: trailingMessages,
          transport: "chat_completions",
          previousResponseId: requestMessages[responseAnchorIndex]?.responseId,
        };
      }
    }
  }

  return {
    messages: messagesToApiFormat(requestMessages),
    transport: selectedTransport === "anthropic_messages"
      ? "anthropic_messages"
      : "chat_completions",
  };
}

function appendPinnedMessagesAfterCompactionArtifact(
  compacted: AgentMessage[],
  allMessages: AgentMessage[],
  settings: ContextCompactionSettings,
): AgentMessage[] {
  if (
    compacted.length === 0 ||
    !compacted[0]?.content.startsWith("[Compacted earlier context]")
  ) {
    return compacted;
  }

  const budgetChars = pinnedMessageBudgetChars(
    Number(settings.context_window_tokens || 0),
  );
  let usedChars = 0;
  const pinnedMessages = allMessages.filter((message) => message.pinnedForCompaction);
  const injectedPins: AgentMessage[] = [];

  for (const message of pinnedMessages) {
    const messageChars = countUnicodeScalars(message.content);
    if (usedChars + messageChars > budgetChars) {
      break;
    }
    usedChars += messageChars;
    injectedPins.push(message);
  }

  if (injectedPins.length === 0) {
    return compacted;
  }

  return [compacted[0], ...injectedPins, ...compacted.slice(1)];
}

function compactMessagesForRequest(
  messages: AgentMessage[],
  settings: ContextCompactionSettings,
): AgentMessage[] {
  if (messages.length === 0 || !settings.auto_compact_context) {
    return messages;
  }

  const targetTokens = resolveContextCompactionTargetTokens(settings);
  const maxMessages = Math.max(1, Number(settings.max_context_messages || 100));
  const strategy = settings.compaction?.strategy ?? "heuristic";
  const overMessageLimit =
    strategy === "heuristic" && messages.length > maxMessages;
  const overTokenLimit = estimateMessageTokens(messages) > targetTokens;
  if (
    !overTokenLimit &&
    !overMessageLimit
  ) {
    return messages;
  }

  const keepRecent = Math.min(
    Math.max(messages.length - 1, 0),
    Math.max(1, Number(settings.keep_recent_on_compact || 10)),
  );
  const splitAt = Math.max(messages.length - keepRecent, 0);
  const olderMessages = messages.slice(0, splitAt);
  const recentMessages = messages.slice(splitAt);

  if (olderMessages.length === 0) {
    return trimCompactedMessages([...messages], targetTokens);
  }

  const summaryMessage: AgentMessage = {
    id: `compacted_${olderMessages[0]?.id ?? "history"}`,
    threadId: olderMessages[0]?.threadId ?? messages[0]?.threadId ?? "",
    createdAt: olderMessages[0]?.createdAt ?? messages[0]?.createdAt ?? Date.now(),
    role: "assistant",
    content: buildCompactionSummary(olderMessages, targetTokens),
    provider: undefined,
    model: undefined,
    inputTokens: 0,
    outputTokens: 0,
    totalTokens: 0,
    reasoning: undefined,
    isCompactionSummary: false,
    isStreaming: false,
  };

  return trimCompactedMessages([summaryMessage, ...recentMessages], targetTokens);
}

function trimCompactedMessages(
  messages: AgentMessage[],
  targetTokens: number,
): AgentMessage[] {
  const trimmed = [...messages];
  while (trimmed.length > 1 && estimateMessageTokens(trimmed) > targetTokens) {
    trimmed.splice(0, 1);
  }
  return trimmed;
}

export function resolveContextCompactionTargetTokens(
  settings: ContextCompactionSettings,
): number {
  return Math.max(
    MIN_CONTEXT_TARGET_TOKENS,
    resolveCompactionTargetTokens(settings),
  );
}

function estimateMessageTokens(messages: AgentMessage[]): number {
  return messages.reduce(
    (sum, message) => sum + estimateSingleMessageTokens(message),
    0,
  );
}

function estimateSingleMessageTokens(message: AgentMessage): number {
  const text =
    message.content +
    (message.reasoning ?? "") +
    (message.toolArguments ?? "") +
    (message.toolName ?? "") +
    (message.toolCalls ? JSON.stringify(message.toolCalls) : "");
  return Math.ceil(text.length / APPROX_CHARS_PER_TOKEN) + 16;
}

function buildCompactionSummary(
  messages: AgentMessage[],
  targetTokens: number,
): string {
  const previewParts: string[] = [];

  for (const message of messages) {
    const summary = summarizeCompactedMessage(message);
    if (!summary) continue;
    previewParts.push(summary);
    const next = `[Compacted earlier context] Summary of older messages retained for continuity: ${previewParts.join(" | ")}`;
    if (Math.ceil(next.length / APPROX_CHARS_PER_TOKEN) >= targetTokens / 2) {
      break;
    }
  }

  const summaryBody =
    previewParts.length > 0
      ? previewParts.join(" | ")
      : `${messages.length} earlier messages were compacted.`;
  return `[Compacted earlier context] Summary of older messages retained for continuity: ${summaryBody}`;
}

function summarizeCompactedMessage(message: AgentMessage): string {
  let content = message.content.replace(/\s+/g, " ").trim();
  if (content.length > 160) {
    content = `${content.slice(0, 157)}...`;
  }

  if (message.role === "tool") {
    const toolName = message.toolName || "tool";
    return content
      ? `tool ${toolName}: ${content}`
      : `tool ${toolName} completed`;
  }

  const prefix = message.role === "assistant" ? "assistant" : message.role;
  return content ? `${prefix}: ${content}` : `${prefix}: (empty)`;
}
