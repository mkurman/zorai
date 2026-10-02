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
  const reviewPresentation = useMemo(
    () => buildToolReviewPresentation(group.welesReview),
    [group.welesReview],
  );
  const details = useMemo(
    () => (collapsed ? null : expandedToolDetails(group.toolName, group.toolArguments, group.resultContent)),
    [collapsed, group.toolName, group.toolArguments, group.resultContent],
  );
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

          {details?.fileTarget ? (
            <ToolFileTargetView label="file" path={details.fileTarget.path} summaryText={group.resultContent} />
          ) : details?.toolDiff ? (
            <ToolDiffView sections={details.toolDiff} />
          ) : details?.structuredArgDetails ? (
            <ToolStructuredValueView label="args" fields={details.structuredArgDetails} />
          ) : details?.formattedArguments ? (
            <div>
              <div className="acp-field-label">args</div>
              <pre className="acp-pre">{details.formattedArguments}</pre>
            </div>
          ) : null}

          {details && !details.fileTarget && details.structuredResult ? (
            <ToolStructuredValueView label="result" fields={details.structuredResult} />
          ) : details && !details.fileTarget && group.resultContent ? (
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

export function sameToolEventGroup(prev: ToolEventGroup, next: ToolEventGroup): boolean {
  return prev.toolCallId === next.toolCallId
    && prev.toolName === next.toolName
    && prev.toolArguments === next.toolArguments
    && prev.resultContent === next.resultContent
    && prev.status === next.status
    && prev.createdAt === next.createdAt
    && prev.welesReview === next.welesReview
    && prev.key === next.key;
}

function expandedToolDetails(toolName: string, toolArguments: string, resultContent: string) {
  const toolDiff = toolArguments
    ? getToolDiffPresentation(toolName, toolArguments)
    : null;
  const fileTarget = toolArguments
    ? getToolFileTarget(toolName, toolArguments)
    : null;
  const structuredArgs = toolArguments
    ? getToolStructuredFields(toolName, toolArguments, "arguments")
    : null;
  const structuredArgDetails = fileTarget && structuredArgs
    ? structuredArgs.filter((field) => field.key !== "path")
    : structuredArgs;
  const structuredResult = resultContent
    ? getToolStructuredFields(toolName, resultContent, "result")
    : null;
  const formattedArguments = !fileTarget && !toolDiff && !structuredArgDetails && toolArguments
    ? formatToolJson(toolArguments)
    : null;
  return { toolDiff, fileTarget, structuredArgDetails, structuredResult, formattedArguments };
}

function formatToolJson(raw: string): string {
  try {
    return JSON.stringify(JSON.parse(raw), null, 2);
  } catch {
    return raw;
  }
}

/**
 * Memoized tool row: `buildDisplayItems` returns fresh group objects on every
 * rebuild, but groups for completed tool calls are content-stable, so compare
 * by value instead of identity. Body JSON is parsed only after the row is expanded.
 */
export const MemoizedToolEventRow = memo(ToolEventRow, (prev, next) =>
  sameToolEventGroup(prev.group, next.group),
);
