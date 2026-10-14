import { getRuntimeModeDescription } from "@/lib/runtimeMode";

export function RuntimeModeBanner() {
  const runtimeMode = getRuntimeModeDescription();

  if (!runtimeMode) {
    return null;
  }

  return (
    <div
      style={{
        position: "fixed",
        top: 12,
        left: 12,
        right: 12,
        zIndex: 5000,
        border: "1px solid color-mix(in srgb, var(--warning) 40%, transparent)",
        background: "linear-gradient(180deg, color-mix(in srgb, var(--warning) 22%, var(--bg-elevated)), color-mix(in srgb, var(--warning) 10%, var(--bg-elevated)))",
        boxShadow: "0 18px 48px color-mix(in srgb, var(--shade) 35%, transparent)",
        color: "var(--warning)",
        padding: "12px 14px",
        display: "grid",
        gap: 4,
      }}
    >
      <strong style={{ fontSize: "var(--text-sm)", letterSpacing: "0.04em", textTransform: "uppercase" }}>
        {runtimeMode.title}
      </strong>
      <span style={{ fontSize: "var(--text-base)", fontWeight: 600 }}>{runtimeMode.summary}</span>
      <span style={{ fontSize: "var(--text-sm)", lineHeight: 1.45, color: "color-mix(in srgb, var(--warning) 92%, transparent)" }}>
        {runtimeMode.detail}
      </span>
    </div>
  );
}
