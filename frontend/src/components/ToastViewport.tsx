import { useToastStore } from "@/lib/toastStore";

const kindColors: Record<string, { border: string; text: string; icon: string }> = {
  error: { border: "color-mix(in srgb, var(--danger) 55%, transparent)", text: "var(--danger)", icon: "⚠" },
  info: { border: "color-mix(in srgb, var(--accent) 45%, transparent)", text: "var(--accent)", icon: "ℹ" },
  success: { border: "color-mix(in srgb, var(--accent) 45%, transparent)", text: "var(--accent)", icon: "✓" },
};

export function ToastViewport() {
  const toasts = useToastStore((state) => state.toasts);
  const dismissToast = useToastStore((state) => state.dismissToast);

  if (toasts.length === 0) return null;

  return (
    <div
      role="alert"
      aria-live="assertive"
      style={{
        position: "fixed",
        bottom: 20,
        right: 20,
        zIndex: 2147483646,
        display: "grid",
        gap: 8,
        maxWidth: 380,
        pointerEvents: "none",
      }}
    >
      {toasts.map((toast) => {
        const colors = kindColors[toast.kind] ?? kindColors.error;
        return (
          <div
            key={toast.id}
            className="zorai-toast"
            style={{
              pointerEvents: "auto",
              display: "flex",
              alignItems: "flex-start",
              gap: 10,
              background: "color-mix(in srgb, var(--shade) 96%, transparent)",
              border: `1px solid ${colors.border}`,
              borderRadius: 10,
              padding: "10px 12px",
              boxShadow: "0 8px 28px color-mix(in srgb, var(--shade) 50%, transparent)",
              backdropFilter: "blur(12px)",
              WebkitBackdropFilter: "blur(12px)",
            }}
          >
            <span style={{ color: colors.text, fontSize: "var(--text-sm)", lineHeight: 1.4, flexShrink: 0 }} aria-hidden="true">
              {colors.icon}
            </span>
            <span style={{ fontSize: "var(--text-sm)", color: "var(--text-primary, var(--text-primary))", lineHeight: 1.45, minWidth: 0, overflowWrap: "anywhere" }}>
              {toast.message}
            </span>
            <button
              type="button"
              aria-label="Dismiss notification"
              onClick={() => dismissToast(toast.id)}
              style={{
                flexShrink: 0,
                background: "transparent",
                border: "none",
                color: "var(--text-muted, var(--text-secondary))",
                cursor: "pointer",
                fontSize: "var(--text-base)",
                lineHeight: 1,
                padding: 2,
              }}
            >
              ×
            </button>
          </div>
        );
      })}
    </div>
  );
}
