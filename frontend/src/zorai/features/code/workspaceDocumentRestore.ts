import { mediaKindForPath } from "@/zorai/features/threads/ThreadFilePreviewOverlay";

export function codeMediaKind(path: string): "image" | "video" | "audio" | null {
  return /\.svg$/i.test(path) ? null : mediaKindForPath(path);
}

export function shouldRestoreWorkspaceDocument(
  activeFile: string | null | undefined,
  documents: Record<string, unknown>,
): boolean {
  if (!activeFile || codeMediaKind(activeFile)) return false;
  return !Object.prototype.hasOwnProperty.call(documents, activeFile);
}
