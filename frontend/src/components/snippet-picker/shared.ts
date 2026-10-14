import type { CSSProperties } from "react";
import type { Snippet } from "../../lib/snippetStore";

export type SnippetPickerProps = {
    style?: CSSProperties;
    className?: string;
};

export type SnippetFormData = Partial<Snippet>;

export const modalStyle: CSSProperties = {
    background: "var(--bg-secondary)",
    border: "1px solid var(--border)",
    borderRadius: 0,
    width: 520,
    maxWidth: "90vw",
    maxHeight: "85vh",
    overflow: "hidden",
    display: "flex",
    flexDirection: "column",
    boxShadow: "none",
};

export const headerStyle: CSSProperties = {
    display: "flex",
    alignItems: "center",
    justifyContent: "space-between",
    padding: "12px 16px",
    borderBottom: "1px solid var(--border)",
    fontSize: 14,
    fontWeight: 600,
};

export const closeBtnStyle: CSSProperties = {
    background: "none",
    border: "none",
    color: "var(--text-secondary)",
    cursor: "pointer",
    fontSize: 14,
    padding: "2px 6px",
};

export const inputStyle: CSSProperties = {
    background: "var(--zorai-bg-surface)",
    border: "1px solid var(--border)",
    borderRadius: 0,
    color: "var(--text-primary)",
    fontSize: 12,
    padding: "6px 10px",
    fontFamily: "inherit",
    outline: "none",
    width: "100%",
};

export const labelStyle: CSSProperties = {
    fontSize: 11,
    color: "var(--text-secondary)",
    fontWeight: 600,
};

export const actionBtnStyle: CSSProperties = {
    background: "var(--accent)",
    border: "none",
    color: "var(--shade)",
    cursor: "pointer",
    fontSize: 12,
    fontWeight: 600,
    padding: "6px 16px",
    borderRadius: 0,
};
