import type { ToolEventGroup } from "@/components/agent-chat-panel/chat-view/types";
import { formatActivityElapsed } from "@/components/agent-chat-panel/chat-view/activityPreview";

const COMMAND_TOOLS = new Set(["bash", "bash_command", "execute_command", "managed_command", "execute_managed_command", "run_bash", "run_terminal", "run_terminal_command", "python_execute", "terminal"]);
const READ_TOOLS = new Set(["read_file"]);
const EDIT_TOOLS = new Set(["edit_file", "write_file", "apply_patch", "apply_file_patch", "replace_in_file", "append_to_file"]);
const CREATE_TOOLS = new Set(["create_file"]);
const EXPLORE_TOOLS = new Set(["search_files", "search_codebase", "find_symbol", "symbol_search", "list_files", "list_directory"]);
const WEB_TOOLS = new Set(["web_search", "web_read"]);

function plural(count: number, one: string, many: string): string {
  return `${count} ${count === 1 ? one : many}`;
}

function toolPath(group: ToolEventGroup): string | null {
  try {
    const args = JSON.parse(group.toolArguments || "{}") as Record<string, unknown>;
    const value = args.path ?? args.file_path ?? args.filePath;
    return typeof value === "string" && value.trim() ? value.trim() : null;
  } catch {
    return null;
  }
}

function countCalls(groups: ToolEventGroup[], names: Set<string>): number {
  return groups.filter((group) => names.has(group.toolName)).length;
}

function countFiles(groups: ToolEventGroup[], names: Set<string>): number {
  const matching = groups.filter((group) => names.has(group.toolName));
  const paths = new Set(matching.map(toolPath).filter((path): path is string => Boolean(path)));
  return paths.size > 0 ? paths.size : matching.length;
}

function stableIndex(key: string, length: number): number {
  let hash = 0;
  for (let index = 0; index < key.length; index += 1) hash = (hash * 31 + key.charCodeAt(index)) | 0;
  return Math.abs(hash) % length;
}

export function finishedTurnSummaries(groups: ToolEventGroup[], elapsedMs: number, lastTitle: string | null): string[] {
  const summaries: string[] = [];
  if (elapsedMs >= 1000) summaries.push(`Worked for ${formatActivityElapsed(elapsedMs)}`);
  const commands = countCalls(groups, COMMAND_TOOLS);
  if (commands > 0) summaries.push(`Ran ${plural(commands, "command", "commands")}`);
  const read = countFiles(groups, READ_TOOLS);
  if (read > 0) summaries.push(`Read ${plural(read, "file", "files")}`);
  const edited = countFiles(groups, EDIT_TOOLS);
  if (edited > 0) summaries.push(`Edited ${plural(edited, "file", "files")}`);
  const created = countFiles(groups, CREATE_TOOLS);
  if (created > 0) summaries.push(`Created ${plural(created, "file", "files")}`);
  const explored = countCalls(groups, EXPLORE_TOOLS);
  if (explored > 0) summaries.push(`Explored the codebase ${plural(explored, "time", "times")}`);
  const web = countCalls(groups, WEB_TOOLS);
  if (web > 0) summaries.push(`Searched the web ${plural(web, "time", "times")}`);
  if (lastTitle) summaries.push(lastTitle);
  return summaries;
}

export function finishedTurnSummary(turnKey: string, groups: ToolEventGroup[], elapsedMs: number, lastTitle: string | null): string {
  const summaries = finishedTurnSummaries(groups, elapsedMs, lastTitle);
  if (summaries.length === 0) return "Worked";
  return summaries[stableIndex(turnKey, summaries.length)];
}
