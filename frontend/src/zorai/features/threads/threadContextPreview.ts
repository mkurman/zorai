import type { WorkContextEntry } from "@/lib/agentWorkContext";

export type ThreadContextPreviewRequest =
  | {
    type: "git-diff";
    repoRoot: string;
    filePath: string;
  }
  | {
    type: "file-preview";
    path: string;
  };

export function threadContextEntryKey(entry: WorkContextEntry): string {
  return `${entry.source}:${entry.repoRoot ?? ""}:${entry.path}:${entry.operationId ?? ""}`;
}

export function threadContextEntryDisplayPath(entry: WorkContextEntry, shortenPath: (path: string) => string): string {
  return entry.repoRoot ? `${shortenPath(entry.repoRoot)}/${entry.path}` : shortenPath(entry.path);
}

function joinRepoPath(repoRoot: string, path: string): string {
  const cleanRoot = repoRoot.replace(/\/+$/, "");
  const cleanPath = path.replace(/^\/+/, "");
  return cleanPath ? `${cleanRoot}/${cleanPath}` : cleanRoot;
}

export function previewRequestsForWorkContextEntry(entry: WorkContextEntry): ThreadContextPreviewRequest[] {
  const filePreviewPath = entry.repoRoot ? joinRepoPath(entry.repoRoot, entry.path) : entry.path;
  const requests: ThreadContextPreviewRequest[] = [{
    type: "file-preview",
    path: filePreviewPath,
  }];

  if (entry.repoRoot) {
    requests.push({
      type: "git-diff",
      repoRoot: entry.repoRoot,
      filePath: entry.path,
    });
  }

  return requests;
}

export function previewRequestForWorkContextEntry(entry: WorkContextEntry): ThreadContextPreviewRequest {
  return previewRequestsForWorkContextEntry(entry)[0];
}

export function isMarkdownPath(path: string): boolean {
  const lower = path.toLowerCase();
  return lower.endsWith(".md") || lower.endsWith(".markdown") || lower.endsWith(".mdx");
}
