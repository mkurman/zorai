import { getBridge } from "@/lib/bridge";
import { getTerminalSnapshot } from "@/lib/terminalRegistry";
import { latestCodeProblems } from "@/zorai/features/code/codeProblems";
import { agentTerminalViewPaneId, threadTerminalPaneId, useThreadTerminalStore } from "@/zorai/features/terminal/threadTerminalStore";
import { clampMentionContent, folderListing, formatCodebaseHits, formatProblems, symbolSnippet, type MentionOption } from "./composerMentions";

const TERMINAL_MENTION_LINES = 200;

function threadTerminalOutput(sessionKey: string | null): string {
  if (!sessionKey) return "";
  const state = useThreadTerminalStore.getState();
  const shells = state.shells[sessionKey] ?? [];
  const viewing = state.viewing[sessionKey] ?? shells[shells.length - 1] ?? null;
  if (!viewing) return "";
  const paneId = shells.includes(viewing) ? threadTerminalPaneId(sessionKey, viewing) : agentTerminalViewPaneId(viewing);
  return getTerminalSnapshot(paneId).split("\n").slice(-TERMINAL_MENTION_LINES).join("\n").trim();
}

export async function resolveMentionContent(option: MentionOption, context: { root: string | null; terminalSessionKey: string | null; files: string[] }): Promise<string> {
  const kind = option.kind;
  if (kind === "problems") return clampMentionContent(formatProblems(latestCodeProblems()));
  if (kind === "web") return `Read this page with your web tools before answering: ${option.id}`;
  if (kind === "folder") return clampMentionContent(folderListing(option.id, context.files));
  if (kind === "terminal") {
    const sessionKeys = [context.terminalSessionKey, context.root ? `code:${context.root}` : null];
    const output = sessionKeys.map(threadTerminalOutput).find(Boolean) ?? "";
    if (!output) throw new Error("No terminal output to add yet. Open a terminal in this thread or the Code tab first.");
    return clampMentionContent(output);
  }
  const bridge = getBridge();
  if (!context.root) throw new Error("This thread has no workspace folder to read from.");
  if (kind === "codebase") {
    if (!bridge?.agentCodeIndex) throw new Error("Semantic code search needs the Zorai desktop app.");
    const response = await bridge.agentCodeIndex("search", context.root, option.id, 8);
    if (response.error) throw new Error(response.error);
    return clampMentionContent(formatCodebaseHits(option.id, Array.isArray(response.result) ? response.result : []));
  }
  if (kind === "symbol") {
    const path = option.path ?? "";
    if (!bridge?.workspaceReadFile || !path) throw new Error("Could not read the symbol's file.");
    const file = await bridge.workspaceReadFile(context.root, path);
    return clampMentionContent(symbolSnippet(path, option.line ?? 1, file.content));
  }
  if (kind === "commit") {
    if (!bridge?.workspaceGitCommitDetail) throw new Error("Commit details need the Zorai desktop app.");
    const detail = await bridge.workspaceGitCommitDetail(context.root, option.id);
    const files = detail.files.map((file) => `${file.status} ${file.path}`).join("\n");
    return clampMentionContent(`commit ${detail.hash}\nAuthor: ${detail.author}\nDate: ${detail.date}\n\n${detail.subject}\n${detail.body ? `\n${detail.body}\n` : ""}\nChanged files:\n${files}`);
  }
  if (!bridge?.workspaceGitDiff) throw new Error("This thread has no workspace folder with Git to read a diff from.");
  const diff = await bridge.workspaceGitDiff(context.root, null, kind === "staged" ? { staged: true } : { includeUntracked: true });
  return clampMentionContent(diff.trim() || (kind === "staged" ? "Nothing is staged." : "No uncommitted changes."));
}
