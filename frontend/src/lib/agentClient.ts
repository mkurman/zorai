/**
 * LLM API client for agent chat.
 *
 * Supports two API formats:
 *  - OpenAI-compatible (most providers)
 *  - Anthropic Messages API
 *
 * All providers are called directly from the frontend via fetch().
 */
import {
  getDefaultApiTransport,
  getProviderApiType,
  getSupportedApiTransports,
} from "./agentStore";
import { resolveProviderAuth } from "./agent-client/auth";
import { sendAnthropic } from "./agent-client/anthropic";
import {
  buildApiMessagesForRequest,
  messagesToApiFormat,
  prepareOpenAIRequest,
} from "./agent-client/context";
import { sendNativeAssistant, sendOpenAICompatible, sendOpenAIResponses } from "./agent-client/openai";
import { TransportCompatibilityError } from "./agent-client/shared";
import type {
  ChatChunk,
  ChatRequest,
} from "./agent-client/types";

export type {
  ApiChatMessage,
  ChatChunk,
  ChatRequest,
  ContextCompactionSettings,
  PreparedOpenAIRequest,
} from "./agent-client/types";

export {
  buildApiMessagesForRequest,
  messagesToApiFormat,
  prepareOpenAIRequest,
};

/**
 * Send a chat completion request. Returns an async iterator of content chunks
 * for streaming, or a single chunk for non-streaming.
 */
export async function* sendChatCompletion(
  req: ChatRequest,
): AsyncGenerator<ChatChunk> {
  try {
    if (req.provider === "cursor") {
      yield {
        type: "error",
        content: "Cursor subscription runs in the daemon. Set the agent backend to daemon, then use Subscription login in Auth settings.",
      };
      return;
    }

    const resolvedAuth = await resolveProviderAuth(req);
    const resolvedRequest: ChatRequest = {
      ...req,
      config: {
        ...req.config,
        api_key: resolvedAuth.api_key,
      },
    };

    if (!resolvedRequest.config.api_key && resolvedRequest.provider !== "ollama") {
      yield {
        type: "error",
        content: `No API key configured for ${req.provider}. Open Settings > Agent to add your key.`,
      };
      return;
    }

    if (!resolvedRequest.config.base_url) {
      yield {
        type: "error",
        content: `No base URL configured for ${req.provider}.`,
      };
      return;
    }

    const supportedTransports = getSupportedApiTransports(
      resolvedRequest.provider,
    );
    const selectedTransport = supportedTransports.includes(
      resolvedRequest.config.api_transport,
    )
      ? resolvedRequest.config.api_transport
      : getDefaultApiTransport(resolvedRequest.provider);

    if (
      selectedTransport === "anthropic_messages" ||
      getProviderApiType(
        resolvedRequest.provider,
        resolvedRequest.config.model,
        resolvedRequest.config.base_url,
      ) === "anthropic"
    ) {
      yield* sendAnthropic(resolvedRequest);
    } else if (selectedTransport === "native_assistant") {
      try {
        yield* sendNativeAssistant({
          ...resolvedRequest,
          config: {
            ...resolvedRequest.config,
            api_transport: "native_assistant",
          },
        });
      } catch (err: any) {
        if (err instanceof TransportCompatibilityError) {
          yield {
            type: "transport_fallback",
            content: err.message,
            fromTransport: "native_assistant",
            toTransport: "chat_completions",
          };
          yield* sendOpenAICompatible({
            ...resolvedRequest,
            config: {
              ...resolvedRequest.config,
              api_transport: "chat_completions",
            },
            previousResponseId: undefined,
            upstreamThreadId: undefined,
          });
        } else {
          throw err;
        }
      }
    } else if (selectedTransport === "responses") {
      try {
        yield* sendOpenAIResponses({
          ...resolvedRequest,
          config: { ...resolvedRequest.config, api_transport: "responses" },
          _chatgptAccountId: resolvedAuth.accountId,
        });
      } catch (err: any) {
        if (err instanceof TransportCompatibilityError) {
          yield {
            type: "transport_fallback",
            content: err.message,
            fromTransport: "responses",
            toTransport: "chat_completions",
          };
          yield* sendOpenAICompatible({
            ...resolvedRequest,
            config: {
              ...resolvedRequest.config,
              api_transport: "chat_completions",
            },
            previousResponseId: undefined,
            upstreamThreadId: undefined,
          });
        } else {
          throw err;
        }
      }
    } else {
      yield* sendOpenAICompatible(resolvedRequest, resolvedAuth.accountId);
    }
  } catch (err: any) {
    if (err.name === "AbortError") {
      yield { type: "done", content: "" };
    } else {
      yield { type: "error", content: `API error: ${err.message || String(err)}` };
    }
  }
}
