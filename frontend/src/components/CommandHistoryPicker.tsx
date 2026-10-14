import { useState, useRef, useEffect, type CSSProperties } from "react";
import { useWorkspaceStore } from "../lib/workspaceStore";
import { useCommandLogStore } from "../lib/commandLogStore";
import { getTerminalController } from "../lib/terminalRegistry";

/**
 * Command history picker (Ctrl+Alt+H).
 * Quick-search through recent unique commands and paste into active terminal.
 */
type CommandHistoryPickerProps = {
  style?: CSSProperties;
  className?: string;
};

export function CommandHistoryPicker({ style, className }: CommandHistoryPickerProps = {}) {
  const open = useWorkspaceStore((s) => s.commandHistoryOpen);
  const toggle = useWorkspaceStore((s) => s.toggleCommandHistory);
  const activePaneId = useWorkspaceStore((s) => s.activePaneId());
  const getRecentEntries = useCommandLogStore((s) => s.getRecentEntries);

  const inputRef = useRef<HTMLInputElement>(null);
  const [query, setQuery] = useState("");
  const [selectedIndex, setSelectedIndex] = useState(0);

  useEffect(() => {
    if (open) {
      setQuery("");
      setSelectedIndex(0);
      setTimeout(() => inputRef.current?.focus(), 0);
    }
  }, [open]);

  if (!open) return null;

  const recent = getRecentEntries(200);
  const filtered = query.trim()
    ? recent.filter((entry) =>
      entry.command.toLowerCase().includes(query.toLowerCase()) ||
      (entry.cwd ?? "").toLowerCase().includes(query.toLowerCase()),
    )
    : recent;

  useEffect(() => {
    setSelectedIndex((current) => {
      if (filtered.length === 0) return 0;
      return Math.min(current, filtered.length - 1);
    });
  }, [filtered.length]);

  const selectCommand = async (cmd: string, execute = true) => {
    const controller = getTerminalController(activePaneId);

    if (controller) {
      await controller.sendText(cmd, { execute, trackHistory: execute });
    } else {
      await navigator.clipboard.writeText(cmd);
    }

    toggle();
  };

  return (
    <div
      onClick={toggle}
      style={{
        position: "fixed",
        inset: 0,
        background: "color-mix(in srgb, var(--shade) 50%, transparent)",
        display: "flex",
        alignItems: "flex-start",
        justifyContent: "center",
        paddingTop: 64,
        zIndex: 960,
        ...(style ?? {}),
      }}
      className={className}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        style={{
          background: "var(--bg-secondary)",
          border: "1px solid var(--border)",
          borderRadius: 0,
          width: "min(900px, 92vw)",
          maxHeight: "72vh",
          overflow: "hidden",
          boxShadow: "none",
        }}
      >
        <input
          ref={inputRef}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Escape") toggle();
            if (e.key === "ArrowDown") {
              e.preventDefault();
              setSelectedIndex((current) => Math.min(current + 1, filtered.length - 1));
            }
            if (e.key === "ArrowUp") {
              e.preventDefault();
              setSelectedIndex((current) => Math.max(current - 1, 0));
            }
            if (e.key === "Enter" && filtered.length > 0) {
              void selectCommand(filtered[selectedIndex]?.command ?? filtered[0].command, !e.shiftKey);
            }
          }}
          placeholder="Search command history by command, cwd, exit code..."
          style={{
            width: "100%",
            padding: "14px 16px",
            background: "var(--zorai-bg)",
            border: "none",
            borderBottom: "1px solid var(--border)",
            color: "var(--text-primary)",
            fontSize: "var(--text-base)",
            fontFamily: "inherit",
            outline: "none",
          }}
        />
        <div style={{ maxHeight: "58vh", overflow: "auto" }}>
          {filtered.length === 0 ? (
            <div
              style={{
                padding: 24,
                textAlign: "center",
                color: "var(--text-secondary)",
                fontSize: "var(--text-sm)",
              }}
            >
              {query ? "No matching commands" : "No command history yet"}
            </div>
          ) : (
            filtered.map((entry, i) => (
              <div
                key={`${entry.id}-${i}`}
                onClick={() => void selectCommand(entry.command, true)}
                onMouseEnter={() => setSelectedIndex(i)}
                style={{
                  padding: "10px 16px",
                  cursor: "pointer",
                  display: "grid",
                  gap: 4,
                  background: selectedIndex === i ? "var(--zorai-bg-surface)" : "transparent",
                }}
              >
                <div style={{ fontSize: "var(--text-base)", fontFamily: "var(--font-mono)", color: "var(--text-primary)", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>
                  {entry.command}
                </div>
                <div style={{ fontSize: "var(--text-sm)", color: "var(--text-secondary)", display: "flex", justifyContent: "space-between", gap: 12 }}>
                  <span style={{ whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{entry.cwd ?? "No cwd"}</span>
                  <span>
                    {entry.exitCode === null ? "running" : `exit ${entry.exitCode}`}
                    {entry.durationMs !== null ? ` · ${entry.durationMs < 1000 ? `${entry.durationMs}ms` : `${(entry.durationMs / 1000).toFixed(1)}s`}` : ""}
                  </span>
                </div>
                <div style={{ fontSize: "var(--text-xs)", color: "var(--text-secondary)", opacity: 0.85, display: "flex", justifyContent: "space-between", gap: 12 }}>
                  <span>{entry.paneId ?? "No pane"}</span>
                  <span>{new Date(entry.timestamp).toLocaleString()}</span>
                </div>
              </div>
            ))
          )}
        </div>
        <div style={{ padding: "10px 16px", borderTop: "1px solid var(--border)", color: "var(--text-secondary)", fontSize: 12 }}>
          Up/Down selects. Enter runs. Shift+Enter types without running.
        </div>
      </div>
    </div>
  );
}
