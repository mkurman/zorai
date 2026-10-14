import type { ReactNode } from "react";

export function Overlay({ onClose, children }: { onClose: () => void; children: ReactNode }) {
    return (
        <div
            onClick={onClose}
            style={{
                position: "fixed",
                inset: 0,
                background: "color-mix(in srgb, var(--shade) 50%, transparent)",
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                zIndex: 960,
            }}
        >
            <div onClick={(event) => event.stopPropagation()}>{children}</div>
        </div>
    );
}
