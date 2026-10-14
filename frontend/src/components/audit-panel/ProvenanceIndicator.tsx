export function ProvenanceIndicator({
  hashValid,
  signatureValid,
  chainValid,
}: {
  hashValid: boolean;
  signatureValid: boolean;
  chainValid: boolean;
}) {
  return (
    <div style={{ display: "flex", flexWrap: "wrap", gap: 8, marginTop: 8 }}>
      <Badge label="Hash" valid={hashValid} />
      <Badge label="Signature" valid={signatureValid} />
      <Badge label="Chain" valid={chainValid} />
    </div>
  );
}

function Badge({ label, valid }: { label: string; valid: boolean }) {
  return (
    <span
      style={{
        fontSize: "var(--text-xs)",
        padding: "2px 8px",
        borderRadius: 999,
        border: "1px solid var(--border)",
        color: valid ? "var(--success, var(--success))" : "var(--danger, var(--danger))",
        background: valid ? "color-mix(in srgb, var(--success) 8%, transparent)" : "color-mix(in srgb, var(--danger) 8%, transparent)",
      }}
    >
      {label}: {valid ? "ok" : "failed"}
    </span>
  );
}