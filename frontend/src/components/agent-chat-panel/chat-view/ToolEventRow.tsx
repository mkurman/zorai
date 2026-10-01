import { memo, useMemo, useState } from "react";
import { buildToolReviewPresentation } from "../toolReviewPresentation";
import type { ToolEventGroup } from "./types";
import { getToolDiffPresentation, ToolDiffView } from "./toolDiffPresentation";
import { ToolStatusIcon } from "./ToolStatusIcon";
import { extractToolArtifacts } from "./toolArtifacts";
import { ToolArtifactChips } from "./ToolArtifactChips";
import { RawToolPayload } from "./RawToolPayload";
import {
  getToolFileTarget,
  getToolStructuredFields,
  ToolFileTargetView,
  ToolStructuredValueView,
} from "./toolValuePresentation";

export function ToolEventRow({ group }: { group: ToolEventGroup }) {
  const [collapsed, setCollapsed] = useState(true);
  const artifacts = useMemo(
    () => extractToolArtifacts(group.toolArguments, group.resultContent),
    [group.resultContent, group.toolArguments],
  );
  const statusLabel = group.status.toUpperCase();
  const toolDiff = group.toolArguments
    ? getToolDiffPresentation(group.toolName, group.toolArguments)
    : null;
  const fileTarget = group.toolArguments
    ? getToolFileTarget(group.toolName, group.toolArguments)
    : null;
  const structuredArgs = group.toolArguments
    ? getToolStructuredFields(group.toolName, group.toolArguments, "arguments")
    : null;
  const structuredArgDetails = fileTarget && structuredArgs
    ? structuredArgs.filter((field) => field.key !== "path")
    : structuredArgs;
  const structuredResult = group.resultContent
    ? getToolStructuredFields(group.toolName, group.resultContent, "result")
    : null;
  const reviewPresentation = buildToolReviewPresentation(group.welesReview);
  const reviewToneClass = reviewPresentation?.tone === "blocked"
    ? "acp-tool-review--blocked"
    : "acp-tool-review--flagged";

  const toolName = useMemo(() => group.toolName.split("_").map((word) => word.charAt(0).toUpperCase() + word.slice(1)).join(" "), [group.toolName]);

  return (
    <div className="acp-tool-row">
      <div className="acp-tool-row__header">
        <button
          type="button"
          aria-expanded={!collapsed}
          className="acp-tool-row__toggle"
          onClick={() => setCollapsed((prev) => !prev)}
        >
          <span className="acp-tool-row__caret">{collapsed ? "▶" : "▼"}</span>
          <span className="acp-tool-row__name">{toolName}</span>
        </button>
        <ToolArtifactChips artifacts={artifacts} createdAt={group.createdAt} compact />
        <div className="acp-tool-row__status">
          {reviewPresentation && (
            <span className="acp-tool-row__badge">
              {reviewPresentation.badgeLabel === "Blocked" ? "blocked" : null}
            </span>
          )}
          <span
            className="acp-tool-row__badge acp-tool-row__badge--status"
            data-status={group.status}
            title={statusLabel.toLowerCase()}
          >
            <ToolStatusIcon status={group.status} />
          </span>
        </div>
      </div>

      {!collapsed && (
        <div className="acp-tool-row__body">
          {reviewPresentation && (
            <div className={`acp-tool-review ${reviewToneClass}`}>
              <div className="acp-tool-review__header">
                <span className="acp-tool-review__title">{reviewPresentation.badgeLabel}</span>
                {reviewPresentation.overrideLabel && (
                  <span className="acp-pill--outline acp-pill">{reviewPresentation.overrideLabel}</span>
                )}
                {reviewPresentation.degradedLabel && (
                  <span className="acp-pill--outline acp-pill">{reviewPresentation.degradedLabel}</span>
                )}
                {reviewPresentation.auditLabel && (
                  <span className="acp-tool-review__audit">{reviewPresentation.auditLabel}</span>
                )}
              </div>
              {reviewPresentation.reasonText && (
                <div className="acp-tool-review__reason">{reviewPresentation.reasonText}</div>
              )}
            </div>
          )}

          {artifacts.length > 0 ? (
            <ToolArtifactChips artifacts={artifacts} createdAt={group.createdAt} />
          ) : null}

          {fileTarget ? (
            <ToolFileTargetView label="file" path={fileTarget.path} summaryText={group.resultContent} />
          ) : toolDiff ? (
            <ToolDiffView sections={toolDiff} />
          ) : structuredArgDetails ? (
            <ToolStructuredValueView label="args" fields={structuredArgDetails} />
          ) : group.toolArguments ? (
            <div>
              <div className="acp-field-label">args</div>
              <pre className="acp-pre">
                {(() => {
                  try {
                    return JSON.stringify(JSON.parse(group.toolArguments), null, 2);
                  } catch {
                    return group.toolArguments;
                  }
                })()}
              </pre>
            </div>
          ) : null}

          {!fileTarget && structuredResult ? (
            <ToolStructuredValueView label="result" fields={structuredResult} />
          ) : !fileTarget && group.resultContent ? (
            <div>
              <div className="acp-field-label">result</div>
              <div className="acp-tool-result">{group.resultContent}</div>
            </div>
          ) : null}

          <RawToolPayload label="Raw arguments" raw={group.toolArguments} />
          <RawToolPayload label="Raw result" raw={group.resultContent} />

          <div className="acp-tool-row__footer">
            <button
              type="button"
              className="acp-btn acp-btn--ghost"
              onClick={() => setCollapsed(true)}
            >
              Collapse
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

/**
 * Memoized tool row: `buildDisplayItems` returns fresh group objects on every
 * rebuild, but groups for completed tool calls are content-stable, so compare
 * by value instead of identity. Without this, every streaming frame re-runs
 * JSON.parse on tool arguments/results for every historical tool row.
 */
export const MemoizedToolEventRow = memo(ToolEventRow, (prev, next) =>
  prev.group.toolCallId === next.group.toolCallId
  && prev.group.toolName === next.group.toolName
  && prev.group.toolArguments === next.group.toolArguments
  && prev.group.resultContent === next.group.resultContent
  && prev.group.status === next.group.status
  && prev.group.createdAt === next.group.createdAt
  && prev.group.welesReview === next.group.welesReview,
);
