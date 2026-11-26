import type { ReactNode } from "react";

export function SettingsGrid({ children, extraClassName }: { children: ReactNode; extraClassName?: string }) {
  return <div className={["zorai-settings-grid", extraClassName ?? ""].filter(Boolean).join(" ")}>{children}</div>;
}

export function Panel({ section, title, children, extraClassName, actions }: { section: string; title: string; children: ReactNode; extraClassName?: string; actions?: ReactNode }) {
  return (
    <div className={["zorai-panel", extraClassName ?? ""].filter(Boolean).join(" ")}>
      {actions ? (
        <div className="zorai-panel__heading">
          <div><div className="zorai-section-label">{section}</div><h2>{title}</h2></div>
          <div className="zorai-panel__heading-actions">{actions}</div>
        </div>
      ) : (
        <div><div className="zorai-section-label">{section}</div><h2>{title}</h2></div>
      )}
      {children}
    </div>
  );
}

export function SettingRow({ label, description, children }: { label: ReactNode; description: string; children: ReactNode }) {
  return (
    <div className="zorai-setting-row">
      <div><strong>{label}</strong><span>{description}</span></div>
      {children}
    </div>
  );
}
