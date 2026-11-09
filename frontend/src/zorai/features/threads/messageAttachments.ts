import type { AgentContentBlock } from "@/lib/agentStore/types";
import type { AttachmentTileModel } from "./attachmentTiles";

const ATTACHED_FILE_PATTERN = /<attached_file name="([^"]*)">\r?\n?([\s\S]*?)\r?\n?<\/attached_file>/g;
const ATTACHED_PATH_PATTERN = /<attached_file name="([^"]*)" path="([^"]*)"[^>]*\/>/g;
const MEDIA_PLACEHOLDER_LINE = /^\[(Image|Audio|Video) attachment(?:: [^\]\n]*)?\]$/;

export function splitMessageAttachments(
  content: string,
  contentBlocks?: AgentContentBlock[],
): {
  displayText: string;
  tiles: AttachmentTileModel[];
} {
  const tiles: AttachmentTileModel[] = [];
  (contentBlocks ?? []).forEach((block, index) => {
    if (block.type === "text") {
      return;
    }
    const previewUrl = block.data_url || block.url;
    tiles.push({
      id: `block:${block.type}:${index}`,
      kind: block.type,
      name: mediaBlockFileName(block.url) ?? mediaBlockName(block.type, block.mime_type, index),
      previewUrl,
      inventoryPath: block.type === "image" ? block.inventory_path : undefined,
    });
  });

  const fileMatches = [...content.matchAll(ATTACHED_FILE_PATTERN)];
  fileMatches.forEach((match, index) => {
    const name = match[1]?.trim() || `file-${index + 1}`;
    tiles.push({
      id: `attached-file:${name}:${index}`,
      kind: "text",
      name,
    });
  });

  [...content.matchAll(ATTACHED_PATH_PATTERN)].forEach((match, index) => {
    const name = match[1]?.trim() || `file-${index + 1}`;
    tiles.push({
      id: `attached-path:${match[2]}:${index}`,
      kind: "file",
      name,
    });
  });

  const mediaKinds = new Set(tiles.map((tile) => tile.kind));
  const displayText = content
    .replace(ATTACHED_FILE_PATTERN, "")
    .replace(ATTACHED_PATH_PATTERN, "")
    .split("\n")
    .filter((line) => {
      const placeholder = line.trim().match(MEDIA_PLACEHOLDER_LINE);
      return !placeholder || !mediaKinds.has(placeholder[1].toLowerCase() as AttachmentTileModel["kind"]);
    })
    .join("\n")
    .trim();
  return { displayText, tiles };
}

function mediaBlockFileName(url: string | undefined): string | null {
  if (!url?.startsWith("file://")) return null;
  const name = decodeURIComponent(url.split(/[?#]/)[0].split("/").pop() ?? "");
  return name || null;
}

function mediaBlockName(kind: "image" | "audio" | "video", mimeType: string | undefined, index: number): string {
  const subtype = mimeType?.split("/")[1]?.split(";")[0];
  if (subtype) {
    return `${kind}.${subtype}`;
  }
  return `${kind}-${index + 1}`;
}
