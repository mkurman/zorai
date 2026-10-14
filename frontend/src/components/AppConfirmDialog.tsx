import { useEffect } from "react";

type AppConfirmDialogProps = {
  open: boolean;
  title: string;
  message: string;
  confirmLabel?: string;
  cancelLabel?: string;
  tone?: "danger" | "warning" | "neutral";
  onConfirm: () => void;
  onCancel: () => void;
};

export function AppConfirmDialog({
  open,
  title,
  message,
  confirmLabel = "Confirm",
  cancelLabel = "Cancel",
  tone = "danger",
  onConfirm,
  onCancel,
}: AppConfirmDialogProps) {
  useEffect(() => {
    if (!open) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        onCancel();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onCancel, open]);

  if (!open) return null;

  const confirmStyle = tone === "danger"
    ? {
      border: "1px solid color-mix(in srgb, var(--danger) 40%, transparent)",
      background: "var(--danger-soft)",
      color: "var(--danger)",
    }
    : tone === "warning"
      ? {
        border: "1px solid color-mix(in srgb, var(--warning) 40%, transparent)",
        background: "var(--warning-soft)",
        color: "var(--warning)",
      }
      : {
        border: "1px solid var(--accent-soft)",
        background: "var(--accent-soft)",
        color: "var(--accent)",
      };

  return (
    <div
      onClick={onCancel}
      style={{
        position: "fixed",
        inset: 0,
        background: "var(--bg-overlay)",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        zIndex: 5200,
        padding: "var(--space-6)",
      }}
    >
      <div
        onClick={(event) => event.stopPropagation()}
        style={{
          width: "min(520px, 92vw)",
          borderRadius: "var(--radius-xl)",
          overflow: "hidden",
          border: "1px solid var(--border-strong)",
          background: "var(--zorai-bg)",
          animation: "slideInUp var(--transition-base) ease",
          boxShadow: "var(--shadow-sm)",
        }}
      >
        <div
          style={{
            padding: "var(--space-5)",
            borderBottom: "1px solid var(--border)",
            display: "flex",
            flexDirection: "column",
            gap: "var(--space-2)",
          }}
        >
          <div style={{ fontSize: "var(--text-lg)", fontWeight: 700, color: "var(--text-primary)" }}>
            {title}
          </div>
          <div style={{ fontSize: "var(--text-sm)", color: "var(--text-secondary)", lineHeight: 1.5 }}>
            {message}
          </div>
        </div>

        <div
          style={{
            padding: "var(--space-4) var(--space-5)",
            display: "flex",
            justifyContent: "flex-end",
            gap: "var(--space-2)",
            borderTop: "1px solid var(--border)",
            background: "var(--bg-secondary)",
          }}
        >
          <button
            type="button"
            onClick={onCancel}
            style={{
              padding: "var(--space-2) var(--space-4)",
              borderRadius: "var(--radius-md)",
              border: "1px solid var(--glass-border)",
              background: "transparent",
              color: "var(--text-secondary)",
              fontSize: "var(--text-sm)",
              fontWeight: 500,
              cursor: "pointer",
            }}
          >
            {cancelLabel}
          </button>

          <button
            type="button"
            onClick={onConfirm}
            style={{
              padding: "var(--space-2) var(--space-4)",
              borderRadius: "var(--radius-md)",
              fontSize: "var(--text-sm)",
              fontWeight: 600,
              cursor: "pointer",
              ...confirmStyle,
            }}
          >
            {confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}

