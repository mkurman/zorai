import { describe, expect, it } from "vitest";
import { buildGeminiInteractionBody } from "./gemini";
import { GEMINI_THOUGHT_MARKER } from "../geminiThought";
import type { ChatRequest } from "./types";

function request(messages: ChatRequest["messages"]): ChatRequest {
  return {
    provider: "gemini",
    config: {
      base_url: "https://generativelanguage.googleapis.com/v1beta",
      model: "gemini-3.8-flash",
      custom_model_name: "",
      api_key: "test-key",
      assistant_id: "",
      api_transport: "chat_completions",
      auth_source: "api_key",
      context_window_tokens: null,
    },
    system_prompt: "Be brief",
    messages,
    streaming: true,
    tools: [{
      type: "function",
      function: {
        name: "get_weather",
        description: "Weather",
        parameters: { type: "object" },
      },
    }],
  };
}

describe("Gemini Interactions requests", () => {
  it("stores the first turn so later turns can reuse the interaction cache", () => {
    const body = buildGeminiInteractionBody(request([
      { role: "user", content: "Weather in London?" },
      {
        role: "assistant",
        content: "",
        reasoning: `${GEMINI_THOUGHT_MARKER}sig-1`,
        tool_calls: [{
          id: "call_1",
          type: "function",
          function: { name: "get_weather", arguments: "{\"location\":\"London\"}" },
        }],
      },
      { role: "tool", content: "{\"temperature\":\"22\"}", tool_call_id: "call_1", name: "get_weather" },
    ]), true);

    expect(body.store).toBe(true);
    expect(body.previous_interaction_id).toBeUndefined();
    expect(body.system_instruction).toBe("Be brief");
    expect(body.input).toEqual([
      { type: "user_input", content: [{ type: "text", text: "Weather in London?" }] },
      { type: "thought", signature: "sig-1" },
      { type: "function_call", id: "call_1", name: "get_weather", arguments: { location: "London" } },
      {
        type: "function_result",
        name: "get_weather",
        call_id: "call_1",
        result: [{ type: "text", text: "{\"temperature\":\"22\"}" }],
      },
    ]);
  });

  it("continues a stored interaction with only the new function result", () => {
    const continued = request([
      { role: "tool", content: "{\"temperature\":\"22\"}", tool_call_id: "call_1", name: "get_weather" },
    ]);
    continued.previousResponseId = "v1_stored";
    const body = buildGeminiInteractionBody(continued, true);
    expect(body.previous_interaction_id).toBe("v1_stored");
    expect(body.system_instruction).toBeUndefined();
    expect(body.input).toEqual([
      {
        type: "function_result",
        name: "get_weather",
        call_id: "call_1",
        result: [{ type: "text", text: "{\"temperature\":\"22\"}" }],
      },
    ]);
  });
});
