import { memo, useState } from "react";
import type { ToolEventAttribution, ToolEventGroup } from "./types";
import { MemoizedToolEventRow, sameToolEventGroup } from "./ToolEventRow";

export const ToolEventList = memo(function ToolEventList({
  groups,
  attribution,
  fallbackAuthorName,
}: {
  groups: ToolEventGroup[];
  attribution?: ToolEventAttribution;
  fallbackAuthorName?: string;
}) {
  const [expanded, setExpanded] = useState(false);
  const summary = summarizeToolGroups(groups);

  if (groups.length === 0 || !summary) {
    return null;
  }

  const { doneCount, working, title } = summary;

  return (
    <div className="acp-tool-list">
      {attribution ? (
        <div className="acp-tool-list__attribution">
          <strong>{attribution.authorAgentName || fallbackAuthorName || "Zorai"}</strong>
          <time>{formatToolEventTime(attribution.createdAt)}</time>
        </div>
      ) : null}
      <button
        type="button"
        aria-expanded={expanded}
        className="acp-tool-list__header"
        onClick={() => setExpanded((prev) => !prev)}
      >
        <span
          className={`acp-tool-list__title${working ? " acp-tool-list__title--working" : ""}`}
          title={title}
        >
          {title}
        </span>
        <span className="acp-tool-list__stats">
          [{doneCount} / {groups.length}]
        </span>
      </button>
      {expanded && (
        <div className="acp-tool-list__body">
          {groups.map((group) => (
            <MemoizedToolEventRow key={group.key} group={group} />
          ))}
        </div>
      )}
    </div>
  );
}, (prev, next) => (
  prev.fallbackAuthorName === next.fallbackAuthorName
  && prev.attribution?.authorAgentName === next.attribution?.authorAgentName
  && prev.attribution?.createdAt === next.attribution?.createdAt
  && sameToolEventGroups(prev.groups, next.groups)
));

function sameToolEventGroups(prev: ToolEventGroup[], next: ToolEventGroup[]): boolean {
  if (prev.length !== next.length) return false;
  for (let index = 0; index < prev.length; index += 1) {
    if (!sameToolEventGroup(prev[index], next[index])) return false;
  }
  return true;
}

function summarizeToolGroups(groups: ToolEventGroup[]): { doneCount: number; working: boolean; title: string } | null {
  if (groups.length === 0) return null;
  let doneCount = 0;
  let working = false;
  for (const group of groups) {
    if (group.status === "done") doneCount += 1;
    else if (group.status === "requested" || group.status === "executing") working = true;
  }
  const rawName = groups[groups.length - 1]?.toolName || "Tools";
  const title = rawName.split("_").map((word) => word.charAt(0).toUpperCase() + word.slice(1)).join(" ");
  return { doneCount, working, title };
}

function formatToolEventTime(timestamp: number): string {
  const milliseconds = timestamp < 10_000_000_000 ? timestamp * 1_000 : timestamp;
  return new Date(milliseconds).toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
  });
}
