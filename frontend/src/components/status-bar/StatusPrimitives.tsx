export function StatusIndicator({ label, status }: { label: string; status: "success" | "warning" | "neutral" | "info" }) {
    const colors = {
        success: { dot: "var(--success)" },
        warning: { dot: "var(--warning)" },
        neutral: { dot: "var(--text-muted)" },
        info: { dot: "var(--agent)" },
    }[status];

    return (
        <div style={{ display: "flex", alignItems: "center", gap: "var(--space-1)" }}>
            <span
                style={{
                    width: 6,
                    height: 6,
                    borderRadius: "50%",
                    background: colors.dot,
                }}
            />
            <span>{label}</span>
        </div>
    );
}

export function StatusBadge({
    label,
    tone,
    onClick,
}: {
    label: string;
    tone: "success" | "warning" | "agent" | "neutral" | "info";
    onClick?: () => void;
}) {
    const colors = {
        success: { bg: "var(--success-soft)", text: "var(--success)", border: "color-mix(in srgb, var(--success) 20%, transparent)" },
        warning: { bg: "var(--warning-soft)", text: "var(--warning)", border: "color-mix(in srgb, var(--warning) 20%, transparent)" },
        agent: { bg: "var(--agent-soft)", text: "var(--agent)", border: "color-mix(in srgb, var(--info) 20%, transparent)" },
        neutral: { bg: "var(--bg-tertiary)", text: "var(--text-muted)", border: "var(--glass-border)" },
        info: { bg: "var(--agent-soft)", text: "var(--agent)", border: "color-mix(in srgb, var(--info) 20%, transparent)" },
    }[tone];

    return (
        <span
            onClick={onClick}
            style={{
                display: "inline-flex",
                alignItems: "center",
                padding: "var(--space-1) var(--space-2)",
                borderRadius: "var(--radius-full)",
                border: `1px solid ${colors.border}`,
                background: colors.bg,
                color: colors.text,
                fontSize: "var(--text-xs)",
                fontWeight: 500,
                cursor: onClick ? "pointer" : "default",
                transition: "all var(--transition-fast)",
            }}
            onMouseEnter={(event) => {
                if (onClick) {
                    event.currentTarget.style.opacity = "0.8";
                }
            }}
            onMouseLeave={(event) => {
                event.currentTarget.style.opacity = "1";
            }}
        >
            {label}
        </span>
    );
}
