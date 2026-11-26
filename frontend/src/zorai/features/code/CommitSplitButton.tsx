import { useState } from "react";
import { CodeContextMenu } from "./CodeToolbarMenu";

export type CommitMode = "commit" | "amend" | "push" | "sync";

export function CommitSplitButton({ busy, canCommit, canAmend, canPush, onCommit }: {
  busy: boolean;
  canCommit: boolean;
  canAmend: boolean;
  canPush: boolean;
  onCommit: (mode: CommitMode) => void;
}) {
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  return (
    <div className="zorai-scm-commit-split">
      <button type="button" className="zorai-scm-commit" disabled={busy || !canCommit} onClick={() => onCommit("commit")}>
        <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false"><path d="M5 12.5l4.5 4.5L19 7.5" /></svg>
        {busy ? "Committing…" : "Commit"}
      </button>
      <button
        type="button"
        className="zorai-scm-commit zorai-scm-commit__more"
        disabled={busy}
        aria-label="More commit actions"
        aria-haspopup="menu"
        aria-expanded={Boolean(menu)}
        onPointerDown={(event) => { if (menu) event.stopPropagation(); }}
        onClick={(event) => {
          const rect = event.currentTarget.getBoundingClientRect();
          setMenu((current) => (current ? null : { x: rect.right - 180, y: rect.bottom + 2 }));
        }}
      >
        <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false"><path d="M6 9l6 6 6-6" /></svg>
      </button>
      {menu ? (
        <CodeContextMenu
          x={menu.x}
          y={menu.y}
          label="Commit actions"
          screenshot={false}
          onClose={() => setMenu(null)}
          items={[
            { key: "commit", label: "Commit", disabled: !canCommit, onSelect: () => onCommit("commit") },
            { key: "amend", label: "Commit (Amend)", disabled: !canAmend, onSelect: () => onCommit("amend") },
            { key: "push", label: "Commit & Push", separatorBefore: true, disabled: !canCommit || !canPush, onSelect: () => onCommit("push") },
            { key: "sync", label: "Commit & Sync", disabled: !canCommit || !canPush, onSelect: () => onCommit("sync") },
          ]}
        />
      ) : null}
    </div>
  );
}
