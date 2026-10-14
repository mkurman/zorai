import { useState } from "react";
import { navigateZorai } from "@/zorai/shell/zoraiNavigationEvents";
import {
  codeReviewStateFor,
  formatFinding,
  formatFindingsForFixAll,
  NO_ISSUES_MARKER,
  parseReviewFindings,
  useCodeReviewStore,
} from "./codeReview";

export type AgentReviewPanelProps = {
  root: string;
  reviewerLabel: string;
  commitWindow: number;
  onFindIssues: () => Promise<string | null>;
  /** Prefill the code agent composer. mode "replace" sets the whole text; "append" adds after existing content. */
  onPrefillAgentInput: (text: string, mode: "replace" | "append") => void;
};

/**
 * Cursor-style AGENT REVIEW block: Find Issues → Reviewing… → Review Again,
 * with Skip / Fix All above a collapsible details list of findings.
 */
export function AgentReviewPanel({ root, reviewerLabel, commitWindow, onFindIssues, onPrefillAgentInput }: AgentReviewPanelProps) {
  const review = useCodeReviewStore((state) => codeReviewStateFor(state.byRoot, root));
  const skipFindings = useCodeReviewStore((state) => state.skipFindings);
  const [detailsOpen, setDetailsOpen] = useState(false);

  const run = () => void onFindIssues();
  const hasFindings = review.status === "done" && review.findings.length > 0;

  const fixAll = () => {
    if (!hasFindings) return;
    onPrefillAgentInput(formatFindingsForFixAll(review.findings), "replace");
  };

  const fixOne = (findingIndex: number) => {
    const finding = review.findings[findingIndex];
    if (!finding) return;
    onPrefillAgentInput(formatFinding(finding, findingIndex), "append");
  };

  return (
    <div className="zorai-agent-review" data-status={review.status}>
      <details className="zorai-agent-review-summary" open>
        <summary>
          <span>AGENT REVIEW</span>
        </summary>
        <div className="zorai-agent-review-body">
          <button
            type="button"
            className="zorai-agent-review-action"
            disabled={review.status === "running"}
            onClick={run}
          >
            {review.status === "running" ? (
              <><SpinnerIcon /> Reviewing…</>
            ) : review.status === "done" || review.status === "error" ? (
              <><RefreshIcon /> Review Again</>
            ) : (
              <><SearchIcon /> Find Issues</>
            )}
          </button>
          <div className="zorai-agent-review-statusline" role="status">
            {review.status === "running" && "Reviewing your changes…"}
            {review.status === "error" && (review.error ?? "Review failed.")}
            {review.status === "done" && (review.findings.length > 0
              ? `${review.findings.length} issue${review.findings.length === 1 ? "" : "s"} found`
              : "No issues found")}
            {review.status === "idle" && `Reviews the last ${commitWindow} commit${commitWindow === 1 ? "" : "s"} with ${reviewerLabel}.`}
          </div>
          <button type="button" className="zorai-agent-review-configure" onClick={() => navigateZorai({ view: "settings", settingsTab: "features" })}>
            Change reviewer or commit count
          </button>
          {hasFindings ? (
            <>
              <div className="zorai-agent-review-result-actions">
                <button type="button" className="zorai-agent-review-skip" onClick={() => skipFindings(root)}>Skip</button>
                <button type="button" className="zorai-agent-review-fixall" onClick={fixAll}>Fix All</button>
              </div>
              <details
                className="zorai-agent-review-details"
                open={detailsOpen}
                onToggle={(event) => setDetailsOpen((event.target as HTMLDetailsElement).open)}
              >
                <summary>Details ({review.findings.length})</summary>
                <ul className="zorai-agent-review-finding-list">
                  {review.findings.map((finding, index) => (
                    <li key={finding.id} className="zorai-agent-review-finding">
                      <div className="zorai-agent-review-finding-copy">
                        <strong>BUG {index + 1}: {finding.title}</strong>
                        {finding.detail ? <pre>{finding.detail}</pre> : null}
                      </div>
                      <button
                        type="button"
                        className="zorai-agent-review-finding-fix"
                        onClick={() => fixOne(index)}
                      >
                        Fix
                      </button>
                    </li>
                  ))}
                </ul>
              </details>
            </>
          ) : null}
        </div>
      </details>
    </div>
  );
}

function SpinnerIcon() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true" className="zorai-agent-review-spin">
      <path d="M12 3a9 9 0 1 0 9 9" fill="none" />
    </svg>
  );
}

function SearchIcon() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <circle cx="11" cy="11" r="6" fill="none" />
      <path d="M20 20l-4.5-4.5" fill="none" />
    </svg>
  );
}

function RefreshIcon() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path d="M4 10a8 8 0 0 1 14-4.5M20 14a8 8 0 0 1-14 4.5" fill="none" />
      <path d="M18 3v3h-3M6 21v-3h3" fill="none" />
    </svg>
  );
}

export { parseReviewFindings, NO_ISSUES_MARKER };
