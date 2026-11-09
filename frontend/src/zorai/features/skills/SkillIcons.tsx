export type SkillIconName =
  | "newSkill"
  | "newFolder"
  | "importFile"
  | "importFolder"
  | "sync"
  | "refresh"
  | "lock"
  | "unlock"
  | "disable"
  | "enable"
  | "preferred"
  | "rename"
  | "delete"
  | "save"
  | "close"
  | "preview"
  | "editor";

const PATHS: Record<SkillIconName, string[]> = {
  newSkill: ["M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z", "M14 3v5h5", "M12 11v6M9 14h6"],
  newFolder: ["M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z", "M12 10v6M9 13h6"],
  importFile: ["M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z", "M14 3v5h5", "M12 10v7M9 14l3 3 3-3"],
  importFolder: ["M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z", "M12 9v7M9 13l3 3 3-3"],
  sync: ["M4 12a8 8 0 0 1 13.7-5.6L20 8", "M20 4v4h-4", "M20 12a8 8 0 0 1-13.7 5.6L4 16", "M4 20v-4h4"],
  refresh: ["M20 11a8 8 0 1 0-2.3 5.7", "M20 5v6h-6"],
  lock: ["M6 11h12v9H6z", "M8.5 11V8a3.5 3.5 0 0 1 7 0v3"],
  unlock: ["M6 11h12v9H6z", "M8.5 11V8a3.5 3.5 0 0 1 6.8-1.2"],
  disable: ["M3 3l18 18", "M10.6 6.1A9.7 9.7 0 0 1 12 6c5 0 8.5 4 9.5 6a12 12 0 0 1-2.7 3.5", "M6.3 7.8A12.7 12.7 0 0 0 2.5 12c1 2 4.5 6 9.5 6a9.4 9.4 0 0 0 4.2-1"],
  enable: ["M2.5 12C3.5 10 7 6 12 6s8.5 4 9.5 6c-1 2-4.5 6-9.5 6s-8.5-4-9.5-6z", "M12 9.5a2.5 2.5 0 1 0 0 5 2.5 2.5 0 0 0 0-5z"],
  preferred: ["M12 3.5l2.6 5.3 5.9.9-4.2 4.1 1 5.8-5.3-2.8-5.3 2.8 1-5.8-4.2-4.1 5.9-.9z"],
  rename: ["M4 20h4L19 9l-4-4L4 16z", "M13.5 6.5l4 4"],
  delete: ["M4 7h16", "M9 7V4h6v3", "M6.5 7l1 13h9l1-13", "M10 11v6M14 11v6"],
  save: ["M5 4h11l3 3v13H5z", "M8 4v5h7V4", "M8 20v-6h8v6"],
  close: ["M6 6l12 12M18 6L6 18"],
  preview: ["M2.5 12C3.5 10 7 6 12 6s8.5 4 9.5 6c-1 2-4.5 6-9.5 6s-8.5-4-9.5-6z", "M12 9.5a2.5 2.5 0 1 0 0 5 2.5 2.5 0 0 0 0-5z"],
  editor: ["M8 8l-4 4 4 4", "M16 8l4 4-4 4", "M13.5 5l-3 14"],
};

export function SkillIcon({ name }: { name: SkillIconName }) {
  return (
    <svg className="zorai-skill-icon" viewBox="0 0 24 24" aria-hidden="true" focusable="false">
      {PATHS[name].map((d) => <path key={d} d={d} />)}
    </svg>
  );
}

export function SkillIconButton({ icon, label, onClick, disabled = false, active = false, danger = false, busy = false, expanded }: {
  icon: SkillIconName;
  label: string;
  onClick: () => void;
  disabled?: boolean;
  active?: boolean;
  danger?: boolean;
  busy?: boolean;
  expanded?: boolean;
}) {
  return (
    <button
      type="button"
      className={[
        "zorai-skill-icon-button",
        active ? "zorai-skill-icon-button--active" : "",
        danger ? "zorai-skill-icon-button--danger" : "",
        busy ? "zorai-skill-icon-button--busy" : "",
      ].filter(Boolean).join(" ")}
      aria-label={label}
      aria-pressed={active || undefined}
      aria-expanded={expanded}
      aria-busy={busy || undefined}
      title={label}
      disabled={disabled}
      onClick={onClick}
    >
      <SkillIcon name={icon} />
    </button>
  );
}
