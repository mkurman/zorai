import { useState, type ReactNode } from "react";

export type ContextRowIcon = "runtime" | "workspace" | "context" | "todos" | "files" | "spawned" | "goal" | "pinned" | "active" | "review" | "failed" | "person" | "text" | "done" | "activity" | "terminal";

const ICON_PATHS: Record<ContextRowIcon, string[]> = {
  runtime: ["M4 6h16M4 12h16M4 18h16", "M8 4v4M16 10v4M10 16v4"],
  workspace: ["M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"],
  context: ["M12 3a9 9 0 1 0 9 9", "M12 7v5l3 2"],
  todos: ["M9 6h11M9 12h11M9 18h11", "M4 6l1 1 2-2M4 12l1 1 2-2M4 18l1 1 2-2"],
  files: ["M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z", "M14 3v5h5"],
  spawned: ["M6 4v10a4 4 0 0 0 4 4h4", "M18 18a2 2 0 1 0 0-.01M6 4a2 2 0 1 0 0-.01"],
  goal: ["M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18z", "M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8z"],
  pinned: ["M9 4h6l-1 6 3 3H7l3-3z", "M12 13v7"],
  active: ["M5 12h3l2-6 4 12 2-6h3"],
  review: ["M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12z", "M12 9a3 3 0 1 0 0 6 3 3 0 0 0 0-6z"],
  failed: ["M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18z", "M9 9l6 6M15 9l-6 6"],
  person: ["M12 4a4 4 0 1 0 0 8 4 4 0 0 0 0-8z", "M4 20a8 8 0 0 1 16 0"],
  text: ["M5 6h14M5 10h14M5 14h10M5 18h7"],
  done: ["M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18z", "M8 12l3 3 5-6"],
  activity: ["M12 3a9 9 0 1 0 9 9", "M12 7v5h5"],
  terminal: ["M4 5h16v14H4z", "M7 9l3 3-3 3M12 15h5"],
};

export function ContextRow({ icon, label, value, defaultOpen = false, children }: {
  icon: ContextRowIcon;
  label: string;
  value?: ReactNode;
  defaultOpen?: boolean;
  children?: ReactNode;
}) {
  const [open, setOpen] = useState(defaultOpen);
  const expandable = Boolean(children);
  const content = (
    <>
      <svg className="zorai-context-row__icon" viewBox="0 0 24 24" aria-hidden="true">
        {ICON_PATHS[icon].map((path) => <path key={path} d={path} />)}
      </svg>
      <span className="zorai-context-row__label">{label}</span>
      {expandable ? <svg className="zorai-context-row__chevron" viewBox="0 0 24 24" aria-hidden="true"><path d="M9 6l6 6-6 6" /></svg> : null}
      <span className="zorai-context-row__value">{typeof value === "string" || typeof value === "number" ? <span className="zorai-context-row__text">{value}</span> : value}</span>
    </>
  );
  return (
    <div className={["zorai-context-row", open ? "is-open" : ""].filter(Boolean).join(" ")}>
      {expandable ? (
        <button type="button" className="zorai-context-row__head" aria-expanded={open} onClick={() => setOpen((current) => !current)}>{content}</button>
      ) : (
        <div className="zorai-context-row__head is-static">{content}</div>
      )}
      {open && expandable ? <div className="zorai-context-row__body">{children}</div> : null}
    </div>
  );
}

export function ContextMeter({ percent }: { percent: number }) {
  return (
    <span className="zorai-context-row__meter" aria-hidden="true">
      <span style={{ width: `${Math.max(0, Math.min(100, percent))}%` }} />
    </span>
  );
}
