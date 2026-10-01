import { useState } from "react";
import type { ToolEventAttribution, ToolEventGroup } from "./types";
import { MemoizedToolEventRow } from "./ToolEventRow";

export function ToolEventList({
  groups,
  attribution,
  fallbackAuthorName,
}: {
  groups: ToolEventGroup[];
  attribution?: ToolEventAttribution;
  fallbackAuthorName?: string;
}) {
  const [expanded, setExpanded] = useState(false);

  if (groups.length === 0) {
    return null;
  }

  const doneCount = groups.filter((group) => group.status === "done").length;
  const working = groups.some(
    (group) => group.status === "requested" || group.status === "executing",
  );
  const title = groups[groups.length - 1]?.toolName?.split("_").map((word) => word.charAt(0).toUpperCase() + word.slice(1)).join(" ") || "Tools";

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
}

function formatToolEventTime(timestamp: number): string {
  const milliseconds = timestamp < 10_000_000_000 ? timestamp * 1_000 : timestamp;
  return new Date(milliseconds).toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
  });
}
