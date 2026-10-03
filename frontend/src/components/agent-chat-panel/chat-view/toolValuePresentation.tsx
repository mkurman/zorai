import { getBridge } from "../../../lib/bridge";

import { TOOL_NAMES } from "@/lib/agentTools/toolNames";

export type ToolStructuredField = {
    key: string;
    value: string;
};

export type ToolFileTarget = {
    path: string;
};

type ToolValueSource = "arguments" | "result";

const CONTENT_FIELD_NAMES = new Set(["content", "contents", "text", "data", "body"]);
const MAX_FIELDS = 24;
const MAX_AGENT_ROWS = 48;

export function getToolFileTarget(toolName: string, toolArguments: string): ToolFileTarget | null {
    if (toolName !== TOOL_NAMES.createFile) {
        return null;
    }

    const args = parseObject(toolArguments);
    if (!args) {
        return null;
    }

    const path = getPathArg(args);
    return path ? { path } : null;
}

export function getToolStructuredFields(
    toolName: string,
    rawValue: string,
    source: ToolValueSource,
): ToolStructuredField[] | null {
    const parsed = parseJson(rawValue);
    if (parsed === null) {
        return null;
    }

    const fields: ToolStructuredField[] = [];
    flattenValue(toolName, source, "", parsed, fields);

    return fields.length > 0 ? fields : null;
}

export function ToolStructuredValueView({
    label,
    fields,
}: {
    label: string;
    fields: ToolStructuredField[];
}) {
    return (
        <div>
            <div className="acp-field-label">{label}</div>
            <div className="acp-struct">
                {fields.map((field) => (
                    <div key={field.key} className="acp-struct__row">
                        <div className="acp-struct__key">{field.key}</div>
                        <div className="acp-struct__value">{field.value || "-"}</div>
                    </div>
                ))}
            </div>
        </div>
    );
}

export function ToolFileTargetView({
    label,
    path,
    summaryText,
}: {
    label: string;
    path: string;
    summaryText?: string;
}) {
    const bridge = getBridge();

    async function openPath() {
        try {
            await bridge?.openFsPath?.(path);
        } catch {
            // Best-effort UI action.
        }
    }

    async function revealPath() {
        try {
            await bridge?.revealFsPath?.(path);
        } catch {
            // Best-effort UI action.
        }
    }

    return (
        <div>
            <div className="acp-field-label">{label}</div>
            <div className="acp-struct">
                <button
                    type="button"
                    onClick={openPath}
                    className="acp-file-target"
                >
                    {path}
                </button>
                {bridge?.revealFsPath && (
                    <div>
                        <button
                            type="button"
                            onClick={revealPath}
                            className="acp-toggle-btn"
                        >
                            Reveal in folder
                        </button>
                    </div>
                )}
                {summaryText && (
                    <div className="acp-file-summary">{summaryText}</div>
                )}
            </div>
        </div>
    );
}

function flattenValue(
    toolName: string,
    source: ToolValueSource,
    prefix: string,
    value: unknown,
    fields: ToolStructuredField[],
) {
    if (fields.length >= MAX_FIELDS) {
        return;
    }

    if (value === null || typeof value === "boolean" || typeof value === "number") {
        fields.push({ key: prefix || "value", value: String(value) });
        return;
    }

    if (typeof value === "string") {
        fields.push({
            key: prefix || "value",
            value: summarizeStringValue(toolName, source, prefix, value),
        });
        return;
    }

    if (Array.isArray(value)) {
        if (appendRecordListingRows(toolName, prefix, value, fields)) {
            return;
        }
        fields.push({
            key: prefix || "items",
            value: summarizeArrayValue(value),
        });
        return;
    }

    if (!isRecord(value)) {
        fields.push({ key: prefix || "value", value: String(value) });
        return;
    }

    const entries = Object.entries(value);
    if (entries.length === 0) {
        fields.push({ key: prefix || "value", value: "{}" });
        return;
    }

    for (const [key, nestedValue] of entries) {
        if (fields.length >= MAX_FIELDS) {
            break;
        }
        const nextPrefix = prefix ? `${prefix}.${key}` : key;
        flattenValue(toolName, source, nextPrefix, nestedValue, fields);
    }
}

function summarizeStringValue(
    toolName: string,
    source: ToolValueSource,
    keyPath: string,
    value: string,
): string {
    const leafKey = keyPath.split(".").pop() ?? keyPath;
    if (toolName === TOOL_NAMES.createFile && source === "arguments" && CONTENT_FIELD_NAMES.has(leafKey)) {
        return summarizeContentValue(value);
    }

    // if (value.includes("\n")) {
    //     return summarizeMultilineValue(value);
    // }

    return value;
}

function summarizeContentValue(value: string): string {
    const lineCount = value.length === 0 ? 0 : value.split(/\r\n?|\n/).length;
    return `${value.length} chars, ${lineCount} line${lineCount === 1 ? "" : "s"}`;
}

// function summarizeMultilineValue(value: string): string {
//     const lines = value.split(/\r\n?|\n/);
//     const preview = lines.slice(0, 3).join(" ").trim();
//     if (preview.length > 180) {
//         return `${preview.slice(0, 180)}... (+${lines.length - 3} more lines)`;
//     }
//     return `${preview}${lines.length > 3 ? ` ... (+${lines.length - 3} more lines)` : ""}`;
// }

function appendRecordListingRows(
    toolName: string,
    prefix: string,
    value: unknown[],
    fields: ToolStructuredField[],
): boolean {
    if (value.length === 0 || !value.every(isRecord)) {
        return false;
    }
    if (toolName === TOOL_NAMES.listAgents) {
        appendLabeledRows(prefix, value, fields, agentRowLabel, formatAgentListing);
        return true;
    }
    if (toolName === TOOL_NAMES.listThreads) {
        appendLabeledRows(prefix, value, fields, threadRowLabel, formatThreadListing);
        return true;
    }
    if (toolName === TOOL_NAMES.workspaceListTasks) {
        appendLabeledRows(prefix, value, fields, threadRowLabel, formatWorkspaceTaskListing);
        return true;
    }
    return false;
}

function appendLabeledRows(
    prefix: string,
    value: Record<string, unknown>[],
    fields: ToolStructuredField[],
    labelFor: (record: Record<string, unknown>, index: number) => string,
    format: (record: Record<string, unknown>) => string,
) {
    const used = new Map<string, number>();
    const visible = value.slice(0, MAX_AGENT_ROWS);
    for (const [index, record] of visible.entries()) {
        const label = labelFor(record, index);
        const key = uniqueFieldKey(used, prefix ? `${prefix}.${label}` : label);
        fields.push({ key, value: format(record) });
    }
    if (value.length > visible.length) {
        fields.push({
            key: uniqueFieldKey(used, prefix ? `${prefix}.more` : "more"),
            value: `+${value.length - visible.length} more`,
        });
    }
}

function threadRowLabel(record: Record<string, unknown>, index: number): string {
    return scalarText(record.title) || scalarText(record.id) || `#${index + 1}`;
}

function formatThreadListing(record: Record<string, unknown>): string {
    const title = scalarText(record.title);
    const parts: string[] = [];
    const id = scalarText(record.id);
    if (id && id !== title) {
        parts.push(id);
    }
    const agent = scalarText(record.agent_name);
    if (agent) {
        parts.push(agent);
    }
    if (record.pinned === true) {
        parts.push("pinned");
    }
    const updated = scalarText(record.updated_at);
    if (updated) {
        parts.push(`updated ${formatEpoch(updated)}`);
    }
    return parts.join(" · ") || "{}";
}

function formatWorkspaceTaskListing(record: Record<string, unknown>): string {
    const parts: string[] = [];
    const status = scalarText(record.status);
    if (status) {
        parts.push(status.replaceAll("_", " "));
    }
    const taskType = scalarText(record.task_type);
    if (taskType) {
        parts.push(taskType.replaceAll("_", " "));
    }
    const priority = scalarText(record.priority);
    if (priority) {
        parts.push(priority);
    }
    const assignee = formatActor(record.assignee);
    if (assignee) {
        parts.push(`assignee ${assignee}`);
    }
    const reviewer = formatActor(record.reviewer);
    if (reviewer) {
        parts.push(`reviewer ${reviewer}`);
    }
    return parts.join(" · ") || "{}";
}

function formatActor(value: unknown): string | null {
    const text = scalarText(value);
    if (text) {
        return text.toLowerCase() === "user" ? "user" : text;
    }
    if (!isRecord(value)) {
        return null;
    }
    for (const key of ["Agent", "agent", "Subagent", "subagent"]) {
        const name = scalarText(value[key]);
        if (name) {
            return name;
        }
    }
    if ("User" in value || "user" in value) {
        return "user";
    }
    return null;
}

function formatEpoch(raw: string): string {
    const value = Number(raw);
    if (!Number.isFinite(value) || value < 1_000_000_000) {
        return raw;
    }
    const millis = value < 1_000_000_000_000 ? value * 1000 : value;
    const date = new Date(millis);
    if (Number.isNaN(date.getTime())) {
        return raw;
    }
    return date.toISOString().slice(0, 16).replace("T", " ");
}

function agentRowLabel(record: Record<string, unknown>, index: number): string {
    return scalarText(record.name)
        || scalarText(record.agent)
        || scalarText(record.id)
        || `#${index + 1}`;
}

function formatAgentListing(record: Record<string, unknown>): string {
    const parts: string[] = [];
    const provider = scalarText(record.provider);
    const model = scalarText(record.model);
    if (provider && model) {
        parts.push(`${provider} / ${model}`);
    } else if (provider) {
        parts.push(provider);
    } else if (model) {
        parts.push(model);
    }

    const effort = scalarText(record.reasoning_effort);
    if (effort) {
        parts.push(`effort ${effort}`);
    }

    const contextWindow = scalarText(record.context_window_tokens ?? record.context_window);
    if (contextWindow) {
        parts.push(`context ${formatTokenCount(contextWindow)}`);
    }

    const kind = scalarText(record.kind);
    if (kind) {
        parts.push(kind);
    }

    const role = scalarText(record.role);
    if (role) {
        parts.push(`role ${role}`);
    }

    const switchable = scalarText(record.switchable);
    if (switchable) {
        parts.push(`switchable ${switchable}`);
    }

    const spawnable = scalarText(record.spawnable);
    if (spawnable) {
        parts.push(`spawnable ${spawnable}`);
    }

    return parts.join(" · ") || "{}";
}

function formatTokenCount(raw: string): string {
    const value = Number(raw);
    if (!Number.isFinite(value)) {
        return raw;
    }
    return Math.trunc(value).toLocaleString("en-US");
}

function scalarText(value: unknown): string | null {
    if (typeof value === "string") {
        const trimmed = value.trim();
        return trimmed.length > 0 ? trimmed : null;
    }
    if (typeof value === "number" && Number.isFinite(value)) {
        return String(value);
    }
    if (typeof value === "boolean") {
        return value ? "true" : "false";
    }
    return null;
}

function uniqueFieldKey(used: Map<string, number>, label: string): string {
    const count = used.get(label) ?? 0;
    used.set(label, count + 1);
    return count === 0 ? label : `${label} #${count + 1}`;
}

function summarizeArrayValue(value: unknown[]): string {
    if (value.length === 0) {
        return "[]";
    }

    if (value.every((item) => item === null || ["string", "number", "boolean"].includes(typeof item))) {
        const preview = value.slice(0, 5).map((item) => String(item)).join(", ");
        return value.length > 5 ? `[${preview}, +${value.length - 5} more]` : `[${preview}]`;
    }

    return `${value.length} item${value.length === 1 ? "" : "s"}`;
}

function parseObject(rawValue: string): Record<string, unknown> | null {
    const parsed = parseJson(rawValue);
    return parsed && isRecord(parsed) ? parsed : null;
}

function parseJson(rawValue: string): unknown | null {
    if (!rawValue) {
        return null;
    }

    try {
        return JSON.parse(rawValue);
    } catch {
        return null;
    }
}

function getPathArg(args: Record<string, unknown>): string | null {
    return getStringArg(args, ["path", "file_path", "filepath", "filename", "file"]);
}

function getStringArg(args: Record<string, unknown>, names: string[]): string | null {
    for (const name of names) {
        const value = args[name];
        if (typeof value === "string") {
            return value;
        }
    }
    return null;
}

function isRecord(value: unknown): value is Record<string, unknown> {
    return !!value && typeof value === "object" && !Array.isArray(value);
}
