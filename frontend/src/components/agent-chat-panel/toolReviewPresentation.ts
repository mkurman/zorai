import type { WelesReviewMeta } from "../../lib/agentTools";

export type ToolReviewTone = "blocked" | "flagged";

export type ToolReviewPresentation = {
  badgeLabel: "Blocked" | "Flagged";
  tone: ToolReviewTone;
  reasonText: string;
  overrideLabel?: string;
  degradedLabel?: "Degraded";
  auditLabel?: string;
};

export function mergeToolReviewMeta(
  primary?: WelesReviewMeta,
  secondary?: WelesReviewMeta,
): WelesReviewMeta | undefined {
  return secondary ?? primary;
}

export function isRoutineToolReview(review?: WelesReviewMeta): boolean {
  if (!review || review.verdict === "block") return false;
  if (review.verdict === "allow") return true;
  const reasons = review.reasons.map((reason) => reason.trim()).filter(Boolean);
  return reasons.length > 0 && reasons.every((reason) => reason.startsWith("allow_"));
}

export function buildToolReviewPresentation(
  review?: WelesReviewMeta,
): ToolReviewPresentation | null {
  if (!review) {
    return null;
  }

  const badgeLabel = review.verdict === "block" ? "Blocked" : "Flagged";
  const tone: ToolReviewTone = review.verdict === "block" ? "blocked" : "flagged";
  const reasonText = review.reasons.filter((reason) => reason.trim().length > 0).join("; ");
  const overrideLabel = typeof review.security_override_mode === "string" && review.security_override_mode.trim()
    ? review.security_override_mode.toUpperCase()
    : undefined;
  const degradedLabel = review.weles_reviewed ? undefined : "Degraded";
  const auditSuffix = typeof review.audit_id === "string" && review.audit_id.trim()
    ? review.audit_id.trim().slice(-8)
    : undefined;

  return {
    badgeLabel,
    tone,
    reasonText,
    overrideLabel,
    degradedLabel,
    auditLabel: auditSuffix ? `#${auditSuffix}` : undefined,
  };
}
