import type { AgentMessage } from "@/lib/agentStore";

export function formatOperationAge(elapsedMs: number): string {
  const seconds = Math.max(0, Math.floor(elapsedMs / 1000));
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  return `${hours}h ${minutes % 60}m`;
}

function mentions(text: string, token: string): boolean {
  if (!token) return false;
  const escaped = token.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return new RegExp(`(^|[^A-Za-z0-9_-])${escaped}($|[^A-Za-z0-9_-])`).test(text);
}

function startsCommand(message: AgentMessage, toolName: string | undefined, command: string): boolean {
  const escaped = JSON.stringify(command).slice(1, -1);
  if (message.toolName === toolName && message.toolArguments?.includes(escaped)) return true;
  return Boolean(message.toolCalls?.some((call) => call.function.name === toolName && call.function.arguments.includes(escaped)));
}

export function latestMessageMentioningOperation(
  messages: AgentMessage[],
  operation: Pick<ZoraiThreadOperation, "operation_id" | "handle"> & Partial<Pick<ZoraiThreadOperation, "tool_name" | "command">>,
): AgentMessage | null {
  for (let index = messages.length - 1; index >= 0; index -= 1) {
    const message = messages[index];
    const text = message.content ?? "";
    if (mentions(text, operation.operation_id) || mentions(text, operation.handle)) return message;
  }
  const command = operation.command?.trim();
  if (!command) return null;
  for (let index = messages.length - 1; index >= 0; index -= 1) {
    if (startsCommand(messages[index], operation.tool_name, command)) return messages[index];
  }
  return null;
}
