export function SharedCursor({ mode }: { mode: "idle" | "human" | "agent" | "approval" }) {
    const palette = {
        idle: { label: "Idle", color: "var(--text-secondary)", glow: "color-mix(in srgb, var(--text-secondary) 18%, transparent)" },
        human: { label: "Human", color: "var(--success)", glow: "color-mix(in srgb, var(--success) 22%, transparent)" },
        agent: { label: "Agent", color: "var(--accent)", glow: "color-mix(in srgb, var(--info) 22%, transparent)" },
        approval: { label: "Approval", color: "var(--warning)", glow: "color-mix(in srgb, var(--warning) 22%, transparent)" },
    }[mode];

    return (
        <div
            style={{
                position: "absolute",
                top: 14,
                right: 14,
                zIndex: 4,
                display: "flex",
                alignItems: "center",
                gap: 8,
                padding: "6px 10px",
                borderRadius: 0,
                border: "1px solid color-mix(in srgb, var(--text-primary) 8%, transparent)",
                background: "color-mix(in srgb, var(--shade) 72%, transparent)",
                backdropFilter: "none",
                boxShadow: "none",
            }}
        >
            <span
                style={{
                    width: 8,
                    height: 8,
                    borderRadius: "50%",
                    background: palette.color,
                    boxShadow: "none",
                }}
            />
            <span style={{ fontSize: "var(--text-sm)", color: palette.color, letterSpacing: "0.08em", textTransform: "uppercase" }}>
                {palette.label} Cursor
            </span>
        </div>
    );
}