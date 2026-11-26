import { useState } from "react";
import type { ComposerMode } from "@/lib/workspaceContextStore";
import { CodeContextMenu } from "./CodeToolbarMenu";

const MODES: Array<{ id: ComposerMode; label: string; title: string }> = [
  { id: "ask", label: "Ask: read and explain only", title: "Ask mode: the agent can read and explain, but cannot edit files or run commands" },
  { id: "plan", label: "Plan: write a plan, change nothing", title: "Plan mode: the agent explores read-only and writes an implementation plan" },
  { id: "agent", label: "Agent: edit, run and delegate", title: "Agent mode: full access to edit files, run commands and delegate" },
];

const SHORT_LABELS: Record<ComposerMode, string> = { ask: "Ask", plan: "Plan", agent: "Agent" };

const ICON = { width: 16, height: 16, viewBox: "0 0 24 24", fill: "none", stroke: "currentColor", strokeWidth: 2, strokeLinecap: "round" as const, strokeLinejoin: "round" as const, "aria-hidden": true };

function ModeIcon({ mode }: { mode: ComposerMode }) {
  if (mode === "ask") return <svg {...ICON}><circle cx="12" cy="12" r="9" /><path d="M9.5 9a2.5 2.5 0 1 1 3.5 2.3c-.6.3-1 .9-1 1.6V14" /><path d="M12 17h.01" /></svg>;
  if (mode === "plan") return <svg {...ICON}><path d="M9 6h11M9 12h11M9 18h11" /><path d="M4 6h.01M4 12h.01M4 18h.01" /></svg>;
  return <svg {...ICON}><path d="M13 2 4 14h7l-1 8 9-12h-7z" /></svg>;
}

export function CodeAgentModePicker({ mode, onChange, onExecutePlan }: {
  mode: ComposerMode;
  onChange: (mode: ComposerMode) => void;
  onExecutePlan: () => void;
}) {
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const current = MODES.find((entry) => entry.id === mode) ?? MODES[2];

  return (
    <>
      <button
        type="button"
        className={mode === "agent" ? "zorai-mode-picker" : "zorai-mode-picker zorai-mode-picker--restricted"}
        title={`${current.title} (click to change)`}
        aria-label={`Agent mode: ${mode}`}
        aria-haspopup="menu"
        onClick={(event) => {
          const rect = event.currentTarget.getBoundingClientRect();
          setMenu({ x: rect.left, y: rect.top });
        }}
      >
        <ModeIcon mode={mode} />
        <span>{SHORT_LABELS[mode]}</span>
        <svg className="zorai-mode-picker__caret" width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" aria-hidden="true"><path d="m6 9 6 6 6-6" /></svg>
      </button>
      {menu ? (
        <CodeContextMenu
          x={menu.x}
          y={menu.y}
          label="Agent mode"
          screenshot={false}
          onClose={() => setMenu(null)}
          items={[
            ...MODES.map((entry) => ({ key: entry.id, label: entry.label, checked: entry.id === mode, onSelect: () => onChange(entry.id) })),
            ...(mode === "plan" ? [{ key: "execute", label: "Execute the plan in Agent mode", separatorBefore: true, onSelect: onExecutePlan }] : []),
          ]}
        />
      ) : null}
    </>
  );
}
