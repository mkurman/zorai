import type { AgentContentBlock } from "@/lib/agentStore/types";
import type { AttachmentTileModel } from "./attachmentTiles";

const ATTACHED_FILE_PATTERN = /<attached_file name="([^"]*)">\r?\n?([\s\S]*?)\r?\n?<\/attached_file>/g;

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
      name: mediaBlockName(block.type, block.mime_type, index),
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

  const displayText = content.replace(ATTACHED_FILE_PATTERN, "").trim();
  return { displayText, tiles };
}

function mediaBlockName(kind: "image" | "audio", mimeType: string | undefined, index: number): string {
  const subtype = mimeType?.split("/")[1]?.split(";")[0];
  if (subtype) {
    return `${kind}.${subtype}`;
  }
  return `${kind}-${index + 1}`;
}
