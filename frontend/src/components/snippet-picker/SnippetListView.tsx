import type { RefObject } from "react";
import type { Snippet } from "../../lib/snippetStore";
import { closeBtnStyle, headerStyle, inputStyle } from "./shared";

export function SnippetListView({
    className,
    style,
    inputRef,
    query,
    setQuery,
    ownerFilter,
    setOwnerFilter,
    filtered,
    onCreate,
    onClose,
    onUse,
    onEdit,
    onDelete,
    onToggleFavorite,
}: {
    className?: string;
    style: React.CSSProperties;
    inputRef: RefObject<HTMLInputElement | null>;
    query: string;
    setQuery: (value: string) => void;
    ownerFilter: "both" | "user" | "assistant";
    setOwnerFilter: (value: "both" | "user" | "assistant") => void;
    filtered: Snippet[];
    onCreate: () => void;
    onClose: () => void;
    onUse: (snippet: Snippet) => void;
    onEdit: (snippet: Snippet) => void;
    onDelete: (snippet: Snippet) => void;
    onToggleFavorite: (snippet: Snippet) => void;
}) {
    return (
        <div style={style} className={className}>
            <div style={headerStyle}>
                <span>Snippets</span>
                <div style={{ display: "flex", gap: 6 }}>
                    <button onClick={onCreate} style={closeBtnStyle} title="New snippet">+</button>
                    <button onClick={onClose} style={closeBtnStyle}>✕</button>
                </div>
            </div>

            <div style={{ padding: "8px 16px", borderBottom: "1px solid var(--border)" }}>
                <input
                    ref={inputRef}
                    value={query}
                    onChange={(event) => setQuery(event.target.value)}
                    placeholder="Search snippets..."
                    style={{ ...inputStyle, width: "100%" }}
                    onKeyDown={(event) => {
                        if (event.key === "Escape") onClose();
                        if (event.key === "Enter" && filtered.length > 0) onUse(filtered[0]);
                    }}
                />
                <div style={{ display: "flex", gap: 6, marginTop: 8 }}>
                    {([
                        ["both", "Both"],
                        ["user", "User"],
                        ["assistant", "Assistant"],
                    ] as const).map(([value, label]) => (
                        <button
                            key={value}
                            type="button"
                            onClick={() => setOwnerFilter(value)}
                            style={{
                                ...closeBtnStyle,
                                border: "1px solid var(--border)",
                                background: ownerFilter === value ? "var(--zorai-bg-surface)" : "transparent",
                                color: ownerFilter === value ? "var(--text-primary)" : "var(--text-secondary)",
                                padding: "4px 8px",
                                fontSize: "var(--text-xs)",
                            }}
                        >
                            {label}
                        </button>
                    ))}
                </div>
            </div>

            <div style={{ flex: 1, overflow: "auto", maxHeight: 380 }}>
                {filtered.length === 0 ? (
                    <div style={{ padding: 20, textAlign: "center", color: "var(--text-secondary)", fontSize: "var(--text-sm)" }}>
                        No snippets found.
                    </div>
                ) : null}
                {filtered.map((snippet) => (
                    <div
                        key={snippet.id}
                        style={{
                            display: "flex",
                            alignItems: "center",
                            padding: "8px 16px",
                            borderBottom: "1px solid color-mix(in srgb, var(--text-primary) 3%, transparent)",
                            cursor: "pointer",
                            gap: 8,
                        }}
                        onClick={() => onUse(snippet)}
                        onMouseEnter={(event) => {
                            event.currentTarget.style.background = "var(--zorai-bg-surface)";
                        }}
                        onMouseLeave={(event) => {
                            event.currentTarget.style.background = "transparent";
                        }}
                    >
                        <button
                            onClick={(event) => {
                                event.stopPropagation();
                                onToggleFavorite(snippet);
                            }}
                            style={{ background: "none", border: "none", cursor: "pointer", fontSize: 14, color: snippet.isFavorite ? "var(--warning)" : "var(--text-secondary)", padding: 0 }}
                            title={snippet.isFavorite ? "Unfavorite" : "Favorite"}
                        >
                            {snippet.isFavorite ? "★" : "☆"}
                        </button>

                        <div style={{ flex: 1, minWidth: 0 }}>
                            <div style={{ fontSize: "var(--text-sm)", fontWeight: 600, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                                {snippet.name}
                            </div>
                            <div style={{ fontSize: "var(--text-xs)", color: "var(--text-secondary)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", fontFamily: "var(--font-mono)" }}>
                                {snippet.content}
                            </div>
                            <div style={{ fontSize: "var(--text-xs)", color: "var(--text-secondary)", opacity: 0.7, marginTop: 1 }}>
                                {snippet.category} · {snippet.owner}{snippet.useCount > 0 && ` · used ${snippet.useCount}×`}
                            </div>
                        </div>

                        <div style={{ display: "flex", gap: 4, flexShrink: 0 }}>
                            <button
                                onClick={(event) => {
                                    event.stopPropagation();
                                    onEdit(snippet);
                                }}
                                style={{ ...closeBtnStyle, fontSize: "var(--text-xs)" }}
                                title="Edit"
                            >
                                ✎
                            </button>
                            <button
                                onClick={(event) => {
                                    event.stopPropagation();
                                    onDelete(snippet);
                                }}
                                style={{ ...closeBtnStyle, color: "var(--danger)", fontSize: "var(--text-xs)" }}
                                title="Delete"
                            >
                                ✕
                            </button>
                        </div>
                    </div>
                ))}
            </div>
        </div>
    );
}
