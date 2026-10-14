import { useMemo, useCallback } from "react";
import { useCommandLogStore } from "../lib/commandLogStore";
import { useWorkspaceStore } from "../lib/workspaceStore";

function OpenCanvasButton() {
  const toggleCanvas = useWorkspaceStore((s) => s.toggleCanvas);
  const handleClick = useCallback(() => toggleCanvas(), [toggleCanvas]);
  return (
    <button onClick={handleClick} style={openCanvasBtnStyle} title="Open Infinite Canvas">
      ◈ Canvas
    </button>
  );
}

const openCanvasBtnStyle: React.CSSProperties = {
  background: "color-mix(in srgb, var(--text-primary) 4%, transparent)",
  border: "1px solid color-mix(in srgb, var(--text-primary) 8%, transparent)",
  color: "var(--agent)",
  cursor: "pointer",
  fontSize: "var(--text-sm)",
  fontWeight: 600,
  padding: "5px 10px",
  borderRadius: 10,
  fontFamily: "inherit",
};

type GraphStep = {
  id: string;
  label: string;
  connectorBefore: string | null;
};

function splitCommand(command: string): GraphStep[] {
  const parts = command
    .split(/(\|\||&&|\||;)/g)
    .map((part) => part.trim())
    .filter(Boolean);
  const steps: GraphStep[] = [];
  let connector: string | null = null;

  for (const part of parts) {
    if (["|", "&&", "||", ";"].includes(part)) {
      connector = part;
      continue;
    }

    steps.push({
      id: `${steps.length}_${part}`,
      label: part,
      connectorBefore: connector,
    });
    connector = null;
  }

  return steps.length > 0 ? steps : [{ id: "single", label: command, connectorBefore: null }];
}

export function AgentExecutionGraph({ paneId }: { paneId?: string | null }) {
  const entries = useCommandLogStore((s) => s.entries);
  const activePaneId = useWorkspaceStore((s) => s.activePaneId());
  const scopePaneId = paneId ?? activePaneId;

  const recent = useMemo(() => {
    const filtered = scopePaneId ? entries.filter((entry) => entry.paneId === scopePaneId) : entries;
    return filtered.slice(0, 6).reverse();
  }, [entries, scopePaneId]);

  if (recent.length === 0) {
    return (
      <div className="zorai-empty-state">
        <div className="zorai-empty-state__icon">◈</div>
        <div className="zorai-empty-state__title">No execution graph yet</div>
        <div className="zorai-empty-state__description">Run a few commands in this pane to build the pipeline visualization</div>
      </div>
    );
  }

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-3)" }}>
      <div style={{ display: "flex", justifyContent: "flex-end" }}>
        <OpenCanvasButton />
      </div>
      {recent.map((entry) => {
        const steps = splitCommand(entry.command);
        return (
          <div
            key={entry.id}
            style={{
              border: "1px solid var(--glass-border)",
              borderRadius: "var(--radius-lg)",
              padding: "var(--space-4)",
              background: "var(--bg-secondary)",
            }}
          >
            <div
              style={{
                display: "flex",
                justifyContent: "space-between",
                gap: "var(--space-3)",
                marginBottom: "var(--space-3)",
                fontSize: "var(--text-xs)",
                color: "var(--text-muted)",
              }}
            >
              <span>
                {new Date(entry.timestamp).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
              </span>
              <span
                style={{
                  color:
                    entry.exitCode === null
                      ? "var(--text-muted)"
                      : entry.exitCode === 0
                        ? "var(--success)"
                        : "var(--danger)",
                }}
              >
                {entry.exitCode === null ? "running" : entry.exitCode === 0 ? "success" : `exit ${entry.exitCode}`}
              </span>
            </div>

            <div style={{ display: "flex", alignItems: "center", gap: "var(--space-2)", overflowX: "auto", paddingBottom: "var(--space-1)" }}>
              {steps.map((step, index) => (
                <div key={step.id} style={{ display: "flex", alignItems: "center", gap: "var(--space-2)" }}>
                  {step.connectorBefore && <ConnectorChip connector={step.connectorBefore} />}
                  <div style={nodeStyle}>
                    <span style={{ opacity: 0.5, marginRight: "var(--space-1)" }}>{index + 1}.</span>
                    {step.label}
                  </div>
                </div>
              ))}
            </div>
          </div>
        );
      })}
    </div>
  );
}

function ConnectorChip({ connector }: { connector: string }) {
  const connectorLabels: Record<string, string> = {
    "|": "pipe",
    "&&": "and",
    "||": "or",
    ";": "seq",
  };

  return (
    <div
      style={{
        padding: "var(--space-1) var(--space-2)",
        borderRadius: "var(--radius-full)",
        background: "var(--agent-soft)",
        border: "1px solid var(--agent-glow)",
        color: "var(--agent)",
        fontSize: "var(--text-xs)",
        fontWeight: 600,
        flexShrink: 0,
      }}
    >
      {connectorLabels[connector] || connector}
    </div>
  );
}

const nodeStyle: React.CSSProperties = {
  minWidth: 140,
  maxWidth: 240,
  padding: "var(--space-2) var(--space-3)",
  borderRadius: "var(--radius-md)",
  background: "var(--bg-tertiary)",
  border: "1px solid var(--glass-border)",
  fontFamily: "var(--font-mono)",
  fontSize: "var(--text-xs)",
  whiteSpace: "pre-wrap",
  wordBreak: "break-word",
  flexShrink: 0,
};
