import type { AgentMessage } from "../../../lib/agentStore";

const COMPACTION_HEADER_PREFIX = "Pre-compaction context:";
const COMPACTION_INLINE_CONTENT_MARKER = "\n\nContent:\n";

export function isCompactionArtifactMessage(message: AgentMessage): boolean {
  if (message.messageKind === "compaction_artifact") {
    return true;
  }
  if (message.isCompactionSummary) {
    return true;
  }
  const content = typeof message.content === "string" ? message.content.trim() : "";
  return content.startsWith(COMPACTION_HEADER_PREFIX);
}

export function compactionArtifactHeaderText(message: AgentMessage): string {
  const header = typeof message.content === "string" ? message.content.trim() : "";
  const markerIndex = header.indexOf(COMPACTION_INLINE_CONTENT_MARKER);
  if (markerIndex >= 0) {
    return header.slice(0, markerIndex).trim();
  }
  return header;
}

export function compactionArtifactPayloadText(message: AgentMessage): string {
  const separate = typeof message.compactionPayload === "string" ? message.compactionPayload.trim() : "";
  if (separate) {
    return separate;
  }
  const header = typeof message.content === "string" ? message.content.trim() : "";
  const markerIndex = header.indexOf(COMPACTION_INLINE_CONTENT_MARKER);
  if (markerIndex >= 0) {
    return header.slice(markerIndex + COMPACTION_INLINE_CONTENT_MARKER.length).trim();
  }
  return "";
}

export function compactionArtifactDisplayText(message: AgentMessage): string {
  if (!isCompactionArtifactMessage(message)) {
    return message.content;
  }

  const visibleHeader = compactionArtifactHeaderText(message);
  const payload = compactionArtifactPayloadText(message);

  if (!payload) {
    return visibleHeader;
  }
  if (!visibleHeader) {
    return payload;
  }
  if (visibleHeader.includes(payload)) {
    return visibleHeader;
  }

  return `${visibleHeader}${COMPACTION_INLINE_CONTENT_MARKER}${payload}`;
}

export function compactionArtifactHasExpandablePayload(message: AgentMessage): boolean {
  return compactionArtifactPayloadText(message).length > 0;
}
