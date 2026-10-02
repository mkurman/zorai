import {
  compactionArtifactDisplayText,
  isCompactionArtifactMessage,
} from "@/components/agent-chat-panel/chat-view/compactionArtifact";
import type { AgentMessage } from "@/lib/agentStore";

const HANDOFF_EVENT_MARKER = "[[handoff_event]]";
const SINGLE_OPERATION_PREFIX = "Background operation finished.";
const BATCH_OPERATION_PREFIX = "Background operations finished.";
const SINGLE_OFFLOADED_RESULT_HEADING = "Operation result saved to file.";
const BATCH_OFFLOADED_RESULT_HEADING = "Operation results saved to file.";
const MAX_PAYLOAD_CHARS = 256_000;
const MAX_BATCH_OPERATIONS = 100;

export type OperationActivityItem = {
  operationId: string;
  tool: string | null;
  state: "accepted" | "started" | "completed" | "failed" | "unknown";
  registeredAt: number | null;
  raw: unknown;
};

export type ThreadActivity =
  | {
      kind: "metacognition";
      subtype: "warning" | "reflection" | "intervention";
      title: string;
      rawText: string;
    }
  | {
      kind: "operation";
      title: string;
      operations: OperationActivityItem[];
      rawText: string;
    }
  | {
      kind: "handoff";
      fromAgentName: string | null;
      toAgentName: string | null;
      stackDepthBefore: number | null;
      stackDepthAfter: number | null;
      rawText: string;
    }
  | {
      kind: "budget";
      title: string;
      rawText: string;
    }
  | {
      kind: "replan";
      title: string;
      rawText: string;
    }
  | {
      kind: "compaction";
      title: string;
      rawText: string;
    };

type JsonRecord = Record<string, unknown>;
type OperationState = OperationActivityItem["state"];

const activityByMessage = new WeakMap<AgentMessage, ThreadActivity | null>();

export function classifyThreadActivityMessage(message: AgentMessage): ThreadActivity | null {
  if (activityByMessage.has(message)) {
    return activityByMessage.get(message) ?? null;
  }
  const activity = classifyThreadActivityMessageUncached(message);
  activityByMessage.set(message, activity);
  return activity;
}

function classifyThreadActivityMessageUncached(message: AgentMessage): ThreadActivity | null {
  const compaction = classifyCompaction(message);
  if (compaction) {
    return compaction;
  }

  if (message.role !== "system") {
    return null;
  }

  const rawText = message.content;
  const handoff = classifyHandoff(rawText);
  if (handoff) {
    return handoff;
  }

  const budget = classifyBudget(rawText);
  if (budget) {
    return budget;
  }

  const replan = classifyReplan(rawText);
  if (replan) {
    return replan;
  }

  const metacognition = classifyMetacognition(rawText);
  if (metacognition) {
    return metacognition;
  }

  return classifyOperation(rawText);
}

function classifyCompaction(message: AgentMessage): ThreadActivity | null {
  if (isCompactionArtifactMessage(message)) {
    return {
      kind: "compaction",
      title: "Compaction",
      rawText: compactionArtifactDisplayText(message),
    };
  }
  if (message.role !== "system") return null;
  const rawText = message.content.trimStart();
  if (!rawText.startsWith("Compaction\n")) return null;
  const body = rawText.slice("Compaction\n".length).trim();
  return {
    kind: "compaction",
    title: "Compaction",
    rawText: body || rawText,
  };
}

function classifyBudget(rawText: string): ThreadActivity | null {
  const content = rawText.trimStart();
  if (content.startsWith("Task budget exceeded for this thread")) {
    return { kind: "budget", title: "Thread budget exceeded", rawText };
  }
  if (/spawned thread/i.test(content) && /exhausted its execution budget/i.test(content)) {
    return { kind: "budget", title: "Subagent budget exceeded", rawText };
  }
  return null;
}

function classifyReplan(rawText: string): ThreadActivity | null {
  const firstLine = rawText.trimStart().split("\n", 1)[0]?.trim() ?? "";
  if (!firstLine.startsWith("[REPLAN]")) {
    return null;
  }
  return { kind: "replan", title: "Strategy refresh", rawText };
}

function classifyHandoff(rawText: string): ThreadActivity | null {
  if (!rawText.startsWith(HANDOFF_EVENT_MARKER)) {
    return null;
  }

  const payloadText = rawText
    .slice(HANDOFF_EVENT_MARKER.length)
    .split("\n", 1)[0]
    ?.trim();
  const payload = parseJson(payloadText);
  if (!isRecord(payload)) {
    return null;
  }

  return {
    kind: "handoff",
    fromAgentName: stringOrNull(payload.from_agent_name),
    toAgentName: stringOrNull(payload.to_agent_name),
    stackDepthBefore: numberOrNull(payload.stack_depth_before),
    stackDepthAfter: numberOrNull(payload.stack_depth_after),
    rawText,
  };
}

function classifyMetacognition(rawText: string): ThreadActivity | null {
  const firstLine = rawText.trimStart().split("\n", 1)[0]?.trim() ?? "";
  if (!/^meta(?:-|\s)?cogniti(?:ve|on)\b/i.test(firstLine)) {
    return null;
  }

  let subtype: "warning" | "reflection" | "intervention";
  if (/\bwarning\b/i.test(firstLine)) {
    subtype = "warning";
  } else if (/\breflection\b/i.test(firstLine)) {
    subtype = "reflection";
  } else if (/\bintervention\b/i.test(firstLine)) {
    subtype = "intervention";
  } else {
    return null;
  }

  const title = subtype === "warning"
    ? "Metacognitive warning"
    : subtype === "reflection"
      ? "Metacognitive reflection"
      : "Metacognitive intervention";

  return { kind: "metacognition", subtype, title, rawText };
}

function classifyOperation(rawText: string): ThreadActivity | null {
  const content = rawText.trimStart();
  if (content.startsWith(BATCH_OPERATION_PREFIX)) {
    const operations = parseBatchOperations(content);
    return operations.length > 0
      ? { kind: "operation", title: "Background operations finished", operations, rawText }
      : null;
  }

  if (content.startsWith(SINGLE_OPERATION_PREFIX)) {
    const operation = parseSingleOperation(content);
    return operation
      ? { kind: "operation", title: "Background operation finished", operations: [operation], rawText }
      : null;
  }

  return null;
}

function parseSingleOperation(content: string): OperationActivityItem | null {
  const body = content.slice(SINGLE_OPERATION_PREFIX.length, MAX_PAYLOAD_CHARS);
  const offloadedSummary = parseOffloadedOperationSummary(body, SINGLE_OFFLOADED_RESULT_HEADING);
  if (offloadedSummary) {
    return offloadedSummary;
  }

  const statusMarker = /(?:^|\n)Operation status:\s*/i.exec(body);
  const labelsText = statusMarker ? body.slice(0, statusMarker.index) : body;
  const labels = parseLabels(labelsText);
  const rawStatusText = statusMarker
    ? body.slice(statusMarker.index + statusMarker[0].length).trim()
    : "";
  const parsedStatus = parseJson(rawStatusText);
  const statusRecord = isRecord(parsedStatus) ? parsedStatus : null;
  const malformedStatus = Boolean(rawStatusText) && !statusRecord;
  const source = statusRecord ?? labels;
  const operationId = firstString(labels.operation_id, source.operation_id, source.operationId);
  const tool = firstString(labels.tool, source.tool);
  const stateValue = firstString(labels.state, source.state);
  const registeredAt = firstNumber(labels.registered_at, source.registered_at, source.registeredAt);
  const labelCount = [operationId, tool, stateValue, registeredAt].filter((value) => value !== null).length;

  if (!operationId && labelCount < 2) {
    return null;
  }

  return {
    operationId: operationId ?? "unknown",
    tool,
    state: malformedStatus || !operationId ? "unknown" : normalizeOperationState(stateValue),
    registeredAt,
    raw: parsedStatus ?? (rawStatusText || { ...labels }),
  };
}

function parseBatchOperations(content: string): OperationActivityItem[] {
  const body = content.slice(BATCH_OPERATION_PREFIX.length, MAX_PAYLOAD_CHARS);
  const offloadedSummary = parseOffloadedOperationSummary(body, BATCH_OFFLOADED_RESULT_HEADING);
  if (offloadedSummary) {
    return [offloadedSummary];
  }

  const marker = /(?:^|\n)Operation results:\s*/i.exec(body);
  if (!marker) {
    return [];
  }

  const rawPayload = body
    .slice(marker.index + marker[0].length)
    .trim();
  const payload = parseJson(rawPayload);
  if (!Array.isArray(payload)) {
    return parseMalformedBatch(rawPayload);
  }

  return payload
    .slice(0, MAX_BATCH_OPERATIONS)
    .filter(isRecord)
    .map(operationFromRecord)
    .filter((operation): operation is OperationActivityItem => operation !== null);
}

function parseOffloadedOperationSummary(
  body: string,
  expectedHeading: string,
): OperationActivityItem | null {
  const summary = body.trimStart();
  const headingEnd = summary.indexOf("\n");
  const heading = (headingEnd === -1 ? summary : summary.slice(0, headingEnd)).trimEnd();
  if (heading !== expectedHeading || headingEnd === -1) {
    return null;
  }

  const labels = parseMarkdownBulletLabels(summary.slice(headingEnd + 1));
  if (!labels) {
    return null;
  }

  const requiredLabels = expectedHeading === SINGLE_OFFLOADED_RESULT_HEADING
    ? ["operations", "state", "payload_id", "file_path", "read_with"]
    : ["operations", "payload_id", "file_path", "read_with"];
  const labelKeys = Object.keys(labels);
  if (
    labelKeys.length !== requiredLabels.length
    || requiredLabels.some((key) => firstString(labels[key]) === null)
  ) {
    return null;
  }

  const operationCount = firstNumber(labels.operations);
  const validOperationCount = expectedHeading === SINGLE_OFFLOADED_RESULT_HEADING
    ? operationCount === 1
    : operationCount !== null && operationCount >= 2;
  if (!Number.isSafeInteger(operationCount) || !validOperationCount) {
    return null;
  }

  return {
    operationId: "unknown",
    tool: null,
    state: normalizeOperationState(firstString(labels.state)),
    registeredAt: null,
    raw: labels,
  };
}

function parseMarkdownBulletLabels(text: string): JsonRecord | null {
  const allowedLabels = new Set(["operations", "state", "payload_id", "file_path", "read_with"]);
  const labels: JsonRecord = {};
  const lines = text.trim().split("\n", 64);
  if (lines.length === 0) {
    return null;
  }

  for (const line of lines) {
    const match = /^\s*-\s+([a-z][a-z0-9_]*)\s*:\s*(.+?)\s*$/i.exec(line);
    if (!match?.[1] || match[2] === undefined) {
      return null;
    }

    const key = match[1].toLowerCase();
    if (!allowedLabels.has(key) || key in labels) {
      return null;
    }
    labels[key] = match[2];
  }
  return labels;
}

function parseMalformedBatch(rawPayload: string): OperationActivityItem[] {
  const labels = parseLabels(rawPayload);
  const operationId = firstString(labels.operation_id, labels.operationId);
  const tool = firstString(labels.tool);
  const state = firstString(labels.state);
  const registeredAt = firstNumber(labels.registered_at, labels.registeredAt);
  const labelCount = [operationId, tool, state, registeredAt].filter((value) => value !== null).length;
  if (!operationId && labelCount < 2) {
    return [];
  }

  return [{
    operationId: operationId ?? "unknown",
    tool,
    state: "unknown",
    registeredAt,
    raw: rawPayload,
  }];
}

function operationFromRecord(raw: JsonRecord): OperationActivityItem | null {
  const operationId = firstString(raw.operation_id, raw.operationId, raw.id);
  const tool = firstString(raw.tool, raw.tool_name, raw.toolName);
  const state = firstString(raw.state, raw.status);
  const registeredAt = firstNumber(raw.registered_at, raw.registeredAt);
  const labelCount = [tool, state, registeredAt].filter((value) => value !== null).length;
  if (!operationId && labelCount < 2) {
    return null;
  }

  return {
    operationId: operationId ?? "unknown",
    tool,
    state: operationId ? normalizeOperationState(state) : "unknown",
    registeredAt,
    raw,
  };
}

function parseLabels(text: string): JsonRecord {
  const labels: JsonRecord = {};
  for (const line of text.split("\n", 64)) {
    const match = /^\s*([a-z][a-z0-9_]*)\s*:\s*(.*?)\s*$/i.exec(line);
    if (match?.[1] && match[2] !== undefined) {
      labels[match[1].toLowerCase()] = match[2];
    }
  }
  return labels;
}

function normalizeOperationState(value: string | null): OperationState {
  const state = value?.trim().toLowerCase().replace(/[\s-]+/g, "_");
  if (state === "succeeded" || state === "success" || state === "completed") {
    return "completed";
  }
  if (state === "error" || state === "failed") {
    return "failed";
  }
  if (state === "accepted" || state === "queued") {
    return "accepted";
  }
  if (state === "started" || state === "running" || state === "in_progress") {
    return "started";
  }
  return "unknown";
}

function parseJson(value: string | undefined): unknown {
  if (!value || value.length > MAX_PAYLOAD_CHARS) {
    return null;
  }
  try {
    return JSON.parse(value);
  } catch {
    return null;
  }
}

function isRecord(value: unknown): value is JsonRecord {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function firstString(...values: unknown[]): string | null {
  for (const value of values) {
    if (typeof value === "string" && value.trim()) {
      return value.trim();
    }
  }
  return null;
}

function firstNumber(...values: unknown[]): number | null {
  for (const value of values) {
    const parsed = typeof value === "number"
      ? value
      : typeof value === "string" && value.trim()
        ? Number(value)
        : Number.NaN;
    if (Number.isFinite(parsed)) {
      return parsed;
    }
  }
  return null;
}

function stringOrNull(value: unknown): string | null {
  return typeof value === "string" ? value : null;
}

function numberOrNull(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}
