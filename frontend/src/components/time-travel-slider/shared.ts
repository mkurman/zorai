import type { CSSProperties } from "react";

export interface SnapshotEntry {
    snapshot_id: string;
    label: string;
    command: string | null;
    created_at: number;
    status: string;
    workspace_id: string | null;
}

export type TimeTravelSliderProps = {
    style?: CSSProperties;
    className?: string;
};

export const closeBtnStyle: CSSProperties = {
    background: "color-mix(in srgb, var(--text-primary) 4%, transparent)",
    border: "1px solid color-mix(in srgb, var(--text-primary) 8%, transparent)",
    color: "var(--text-secondary)",
    cursor: "pointer",
    fontSize: 13,
    padding: "4px 8px",
    borderRadius: 0,
    lineHeight: 1,
};

export const actionBtnStyle: CSSProperties = {
    background: "color-mix(in srgb, var(--text-primary) 6%, transparent)",
    border: "1px solid color-mix(in srgb, var(--text-primary) 12%, transparent)",
    color: "var(--text-primary)",
    borderRadius: 0,
    padding: "6px 14px",
    fontSize: 12,
    cursor: "pointer",
    fontFamily: "inherit",
    fontWeight: 600,
};

export const chipStyle: CSSProperties = {
    fontSize: 10,
    padding: "2px 8px",
    borderRadius: 0,
    border: "1px solid",
    fontWeight: 600,
};
