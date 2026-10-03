import type { ChatChunk, ChatRequest } from "./types";
import { GEMINI_THOUGHT_MARKER, geminiThoughtSignature } from "../geminiThought";

const GEMINI_API_REVISION = "2026-05-20";

function toolArgumentsJson(value: unknown): string {
  let current = value;
  for (let depth = 0; depth < 4 && typeof current === "string"; depth += 1) {
    const trimmed = current.trim();
    if (!trimmed.startsWith("{") && !trimmed.startsWith("[") && !trimmed.startsWith("\"")) break;
    try {
      const parsed = JSON.parse(trimmed) as unknown;
      if (parsed !== null && (typeof parsed === "object" || typeof parsed === "string")) {
        current = parsed;
        continue;
      }
    } catch {
      break;
    }
    break;
  }
  if (typeof current === "string") return JSON.stringify({ _raw: current });
  return JSON.stringify(current ?? {});
}

type JsonRecord = Record<string, unknown>;

export function buildGeminiInteractionBody(req: ChatRequest, stream: boolean): JsonRecord {
  const previousInteractionId = req.previousResponseId?.trim() || undefined;
  const input = req.messages.flatMap((message) => {
    if (message.role === "system" || message.role === "user") {
      const text = message.content?.trim() ?? "";
      return text ? [{ type: "user_input", content: [{ type: "text", text: message.content }] }] : [];
    }
    if (message.role === "assistant") {
      const steps: JsonRecord[] = [];
      const signature = geminiThoughtSignature(message.reasoning);
      if (signature) steps.push({ type: "thought", signature });
      if (message.content?.trim()) {
        steps.push({ type: "model_output", content: [{ type: "text", text: message.content }] });
      }
      for (const toolCall of message.tool_calls ?? []) {
        let args: unknown = {};
        try {
          args = JSON.parse(toolCall.function.arguments || "{}");
        } catch {
          args = { _raw: toolCall.function.arguments || "" };
        }
        steps.push({
          type: "function_call",
          id: toolCall.id,
          name: toolCall.function.name,
          arguments: args,
        });
      }
      return steps;
    }
    if (message.role === "tool" && message.tool_call_id) {
      return [{
        type: "function_result",
        name: message.name ?? "",
        call_id: message.tool_call_id,
        result: [{ type: "text", text: message.content ?? "" }],
      }];
    }
    return [];
  });

  const body: JsonRecord = {
    model: req.config.model,
    input,
    store: true,
    stream,
  };
  if (previousInteractionId) {
    body.previous_interaction_id = previousInteractionId;
  } else {
    const system = req.system_prompt?.trim();
    if (system) body.system_instruction = system;
  }
  if (req.tools && req.tools.length > 0) {
    body.tools = req.tools.map((tool) => ({
      type: "function",
      name: tool.function.name,
      description: tool.function.description,
      parameters: tool.function.parameters,
    }));
  }
  if (stream) body.generation_config = { thinking_summaries: "auto" };
  return body;
}

export async function* sendGemini(req: ChatRequest): AsyncGenerator<ChatChunk> {
  const base = req.config.base_url.replace(/\/$/, "");
  const response = await fetch(`${base}/interactions?alt=sse`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "x-goog-api-key": req.config.api_key,
      "Api-Revision": GEMINI_API_REVISION,
    },
    body: JSON.stringify(buildGeminiInteractionBody(req, true)),
    signal: req.signal,
  });

  if (!response.ok) {
    const text = await response.text().catch(() => "");
    yield { type: "error", content: `Gemini API returned ${response.status}: ${text.slice(0, 200)}` };
    return;
  }

  if (!response.body) {
    yield { type: "error", content: "Gemini API returned an empty response." };
    return;
  }

  yield* parseGeminiSse(response.body, req.signal);
}

async function* parseGeminiSse(
  body: ReadableStream<Uint8Array>,
  signal?: AbortSignal,
): AsyncGenerator<ChatChunk> {
  const reader = body.getReader();
  const decoder = new TextDecoder();
  let buffer = "";
  let eventName = "";
  let data = "";
  let content = "";
  let summary = "";
  let signature = "";
  const toolCalls: NonNullable<ChatChunk["toolCalls"]> = [];
  let inputTokens = 0;
  let outputTokens = 0;
  let responseId: string | undefined;
  const steps = new Map<number, { kind: string; name: string; id: string; arguments: unknown; argumentsText: string }>();

  const stepAt = (index: number) => {
    const current = steps.get(index) ?? { kind: "", name: "", id: "", arguments: undefined, argumentsText: "" };
    steps.set(index, current);
    return current;
  };

  const applyPayload = function* (event: string, payload: any): Generator<ChatChunk> {
    if (event === "interaction.created" || event === "interaction.completed") {
      const interaction = payload.interaction ?? payload;
      if (typeof interaction.id === "string") responseId = interaction.id;
      const usage = interaction.usage ?? {};
      if (typeof usage.total_input_tokens === "number") inputTokens = usage.total_input_tokens;
      if (typeof usage.total_output_tokens === "number") outputTokens = usage.total_output_tokens;
      for (const [index, step] of (interaction.steps ?? []).entries()) {
        const current = stepAt(index);
        current.kind ||= step.type ?? "";
        current.name ||= step.name ?? "";
        current.id ||= step.id ?? "";
        if (step.arguments) current.arguments = step.arguments;
        if (step.type === "thought" && typeof step.signature === "string") signature ||= step.signature;
      }
      return;
    }
    if (event === "step.start") {
      const current = stepAt(Number(payload.index ?? 0));
      current.kind = payload.step?.type ?? current.kind;
      current.name = payload.step?.name ?? current.name;
      current.id = payload.step?.id ?? current.id;
      if (payload.step?.arguments) current.arguments = payload.step.arguments;
      if (typeof payload.step?.signature === "string") signature = payload.step.signature;
      return;
    }
    if (event !== "step.delta") return;
    const current = stepAt(Number(payload.index ?? 0));
    const delta = payload.delta ?? {};
    if (typeof delta.signature === "string") {
      signature = delta.signature;
      current.kind ||= "thought";
    }
    if (typeof delta.name === "string") current.name = delta.name;
    if (delta.arguments) current.arguments = delta.arguments;
    if (typeof delta.text !== "string") return;
    if (current.kind === "thought") {
      summary += delta.text;
      yield { type: "delta", content: "", reasoning: delta.text };
      return;
    }
    if (current.kind === "function_call") {
      current.argumentsText += delta.text;
      return;
    }
    content += delta.text;
    yield { type: "delta", content: delta.text };
  };

  const flush = function* (): Generator<ChatChunk> {
    if (!data.trim()) {
      eventName = "";
      data = "";
      return;
    }
    let payload: any = {};
    try {
      payload = JSON.parse(data);
    } catch {
      payload = {};
    }
    const event = eventName || payload.event_type || "";
    eventName = "";
    data = "";
    yield* applyPayload(event, payload);
  };

  try {
    while (true) {
      if (signal?.aborted) break;
      const { done, value } = await reader.read();
      if (done) break;
      buffer += decoder.decode(value, { stream: true });
      const lines = buffer.split("\n");
      buffer = lines.pop() ?? "";
      for (const line of lines) {
        if (line.trim() === "") {
          yield* flush();
          continue;
        }
        if (line.startsWith("event:")) eventName = line.slice(6).trim();
        else if (line.startsWith("data:")) data += (data ? "\n" : "") + line.slice(5).trim();
      }
    }
    if (buffer.trim()) {
      if (buffer.startsWith("event:")) eventName = buffer.slice(6).trim();
      else if (buffer.startsWith("data:")) data += (data ? "\n" : "") + buffer.slice(5).trim();
      else if (!eventName && buffer.trim().startsWith("{")) {
        const payload = JSON.parse(buffer);
        yield* applyPayload("interaction.completed", payload);
      }
    }
    yield* flush();
  } finally {
    reader.releaseLock();
  }

  if (toolCalls.length === 0) {
    for (const step of steps.values()) {
      if (step.kind !== "function_call" || !step.name) continue;
      let args: unknown = step.arguments;
      if (args === undefined || args === null || (typeof args === "object" && args !== null && Object.keys(args as object).length === 0 && step.argumentsText.trim())) {
        try {
          args = step.argumentsText ? JSON.parse(step.argumentsText) : {};
        } catch {
          args = { _raw: step.argumentsText };
        }
      }
      toolCalls.push({
        id: step.id || `gemini_call_${toolCalls.length}`,
        type: "function",
        function: { name: step.name, arguments: toolArgumentsJson(args) },
      });
    }
  }

  const reasoning = signature
    ? `${summary.trim()}${summary.trim() ? "\n" : ""}${GEMINI_THOUGHT_MARKER}${signature}`
    : summary.trim() || undefined;

  if (toolCalls.length > 0) {
    yield {
      type: "tool_calls",
      content,
      reasoning,
      toolCalls,
      inputTokens,
      outputTokens,
      responseId,
    };
    return;
  }

  yield {
    type: "done",
    content,
    reasoning,
    inputTokens,
    outputTokens,
    responseId,
  };
}
