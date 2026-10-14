import type { AuditEntry } from "../../lib/types";
import { findMatchingProvenanceEntry, useAuditStore } from "../../lib/auditStore";
import { ConfidenceBadge } from "./ConfidenceBadge";
import { ProvenanceIndicator } from "./ProvenanceIndicator";

/**
 * Expanded detail view for a single audit entry.
 * Shows full explanation, confidence breakdown, causal trace ID, and thread ID.
 */
export function AuditDetailView({ entry }: { entry: AuditEntry }) {
  const provenanceReport = useAuditStore((s) => s.provenanceReport);
  const provenanceEntry = findMatchingProvenanceEntry(entry, provenanceReport);

  return (
    <div
      style={{
        padding: "var(--space-4)",
        background: "color-mix(in srgb, var(--reasoning) 10%, transparent)",
        borderTop: "1px solid var(--border)",
      }}
    >
      {entry.explanation && (
        <div
          style={{
            fontSize: "var(--text-base)",
            color: "var(--text-secondary)",
            marginBottom: "var(--space-2)",
            lineHeight: "var(--leading-normal)",
          }}
        >
          {entry.explanation}
        </div>
      )}

      {entry.confidence != null && entry.confidenceBand && (
        <div style={{ marginBottom: "var(--space-2)" }}>
          <ConfidenceBadge confidence={entry.confidence} band={entry.confidenceBand} />
        </div>
      )}

      {entry.causalTraceId && (
        <div
          style={{
            fontSize: "var(--text-xs)",
            color: "var(--text-secondary)",
            marginBottom: "var(--space-1)",
          }}
        >
          Causal trace: <span style={{ color: "var(--reasoning)" }}>{entry.causalTraceId}</span>
        </div>
      )}

      {entry.threadId && (
        <div
          style={{
            fontSize: "var(--text-xs)",
            color: "var(--text-secondary)",
          }}
        >
          Thread: <span style={{ color: "var(--agent)" }}>{entry.threadId}</span>
        </div>
      )}

      {(entry.goalRunId || entry.taskId) && (
        <div
          style={{
            fontSize: "var(--text-xs)",
            color: "var(--text-secondary)",
            marginTop: "var(--space-1)",
          }}
        >
          {entry.goalRunId ? <>Goal: <span style={{ color: "var(--reasoning)" }}>{entry.goalRunId}</span> </> : null}
          {entry.taskId ? <>Task: <span style={{ color: "var(--approval)" }}>{entry.taskId}</span></> : null}
        </div>
      )}

      {provenanceEntry ? (
        <div style={{ marginTop: "var(--space-2)" }}>
          <div style={{ fontSize: "var(--text-xs)", color: "var(--text-secondary)", marginBottom: 4 }}>
            Integrity verification
          </div>
          <ProvenanceIndicator
            hashValid={provenanceEntry.hashValid}
            signatureValid={provenanceEntry.signatureValid}
            chainValid={provenanceEntry.chainValid}
          />
          {provenanceEntry.complianceMode ? (
            <div style={{ fontSize: "var(--text-xs)", color: "var(--text-secondary)", marginTop: 8 }}>
              Compliance mode: <span style={{ color: "var(--approval)" }}>{provenanceEntry.complianceMode}</span>
            </div>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}
