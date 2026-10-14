import { memo } from "react";
import { Handle, Position, type NodeProps } from "@xyflow/react";

interface ToolNodeData {
  label: string;
  exitCode: number | null;
  durationMs: number | null;
  timestamp: number;
  isRunning: boolean;
  entryId: string;
  [key: string]: unknown;
}

export const ToolNode = memo(function ToolNode({ data, selected }: NodeProps) {
  const d = data as ToolNodeData;
  const statusColor = d.isRunning
    ? "var(--accent)"
    : d.exitCode === 0
      ? "var(--success)"
      : d.exitCode !== null
        ? "var(--danger)"
        : "var(--text-secondary)";

  return (
    <>
      <Handle type="target" position={Position.Left} style={handleStyle} />
      <div
        style={{
          minWidth: 160,
          maxWidth: 220,
          padding: "10px 14px",
          borderRadius: 0,
          background: selected
            ? "var(--bg-secondary)"
            : "var(--zorai-bg)",
          border: `1px solid ${selected ? "color-mix(in srgb, var(--info) 36%, transparent)" : "var(--glass-border)"}`,
          boxShadow: "none",
          transition: "all 0.15s ease",
        }}
      >
        {/* Command text */}
        <div
          style={{
            fontFamily: "var(--font-mono, monospace)",
            fontSize: "var(--text-sm)",
            color: "var(--text-primary)",
            whiteSpace: "pre-wrap",
            wordBreak: "break-word",
            lineHeight: 1.4,
            maxHeight: 52,
            overflow: "hidden",
          }}
        >
          {d.label}
        </div>

        {/* Status row */}
        <div
          style={{
            display: "flex",
            justifyContent: "space-between",
            alignItems: "center",
            gap: 8,
            marginTop: 8,
            fontSize: "var(--text-sm)",
          }}
        >
          <span style={{ color: statusColor, fontWeight: 600 }}>
            {d.isRunning ? "running" : d.exitCode === 0 ? "ok" : d.exitCode !== null ? `exit ${d.exitCode}` : ""}
          </span>
          {d.durationMs !== null && (
            <span style={{ color: "var(--text-secondary)" }}>
              {d.durationMs < 1000 ? `${d.durationMs}ms` : `${(d.durationMs / 1000).toFixed(1)}s`}
            </span>
          )}
        </div>

        {/* Running indicator */}
        {d.isRunning && (
          <div
            style={{
              position: "absolute",
              top: -3,
              right: -3,
              width: 8,
              height: 8,
              borderRadius: "50%",
              background: "var(--accent)",
              boxShadow: "none",
              animation: "glow-pulse 1.5s ease-in-out infinite",
            }}
          />
        )}
      </div>
      <Handle type="source" position={Position.Right} style={handleStyle} />
    </>
  );
});

const handleStyle: React.CSSProperties = {
  width: 8,
  height: 8,
  background: "var(--accent)",
  border: "2px solid var(--zorai-bg)",
};
