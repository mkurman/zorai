import type { AgentMessage } from "@/lib/agentStore";

const MAX_TOOL_MESSAGES = 400;

function stringLeaves(value: unknown, out: string[], depth = 0): void {
  if (depth > 6) return;
  if (typeof value === "string") out.push(value);
  else if (Array.isArray(value)) value.forEach((item) => stringLeaves(item, out, depth + 1));
  else if (value && typeof value === "object") Object.values(value).forEach((item) => stringLeaves(item, out, depth + 1));
}

function joinPath(dir: string, relative: string): string {
  return `${dir.replace(/[\\/]+$/, "")}/${relative.replace(/^\.\//, "")}`;
}

function escapeRegExp(text: string): string {
  return text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

export function fileLinkCandidatesFromTools(href: string, messages: AgentMessage[], homeDir?: string | null): string[] {
  const target = href.replace(/^\.\//, "");
  if (!target) return [];
  const pattern = new RegExp(`(?:^|[\\s"'=(\`])((?:[~/.\\w-][^\\s"'\`()]*/)?${escapeRegExp(target)})(?=$|[\\s"'\`),:;])`, "g");
  const candidates: string[] = [];
  const add = (path: string) => {
    const expanded = path.startsWith("~/") && homeDir ? joinPath(homeDir, path.slice(2)) : path;
    if (!candidates.includes(expanded)) candidates.push(expanded);
  };
  const tools = messages.filter((message) => message.role === "tool").slice(-MAX_TOOL_MESSAGES).reverse();
  for (const message of tools) {
    let args: unknown = null;
    try {
      args = message.toolArguments ? JSON.parse(message.toolArguments) : null;
    } catch {
      args = null;
    }
    const cwd = args && typeof args === "object" && typeof (args as { cwd?: unknown }).cwd === "string" ? (args as { cwd: string }).cwd : null;
    const texts: string[] = [];
    stringLeaves(args, texts);
    if (message.content) texts.push(message.content);
    for (const text of texts) {
      for (const match of text.matchAll(pattern)) {
        const found = match[1];
        if (found.startsWith("/") || /^[A-Za-z]:[\\/]/.test(found) || found.startsWith("~/")) add(found);
        else if (cwd) add(joinPath(cwd, found));
      }
    }
    if (cwd) add(joinPath(cwd, target));
  }
  return candidates;
}
