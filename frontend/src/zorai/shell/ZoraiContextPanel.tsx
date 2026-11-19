import { useEffect, useState, type ReactNode } from "react";

type ZoraiContextPanelProps = {
  title: string;
  subtitle?: string;
  collapsedLabel?: string;
  headerless?: boolean;
  open: boolean;
  onToggle: () => void;
  children: ReactNode;
};

export function ZoraiContextPanel({
  title,
  subtitle,
  collapsedLabel = "Context",
  headerless = false,
  open,
  onToggle,
  children,
}: ZoraiContextPanelProps) {
  const [everOpened, setEverOpened] = useState(open);
  useEffect(() => {
    if (open) setEverOpened(true);
  }, [open]);
  const collapsedTab = open ? null : (
    <button
      type="button"
      className="zorai-context-tab"
      onClick={onToggle}
      title={collapsedLabel}
      aria-expanded={false}
      aria-controls="zorai-context-panel"
      aria-label={collapsedLabel}
    >
      <svg className="zorai-context-tab__icon" viewBox="0 0 16 16" aria-hidden="true"><rect x="2" y="2.5" width="12" height="11" rx="2" /><path d="M10 2.5v11" /></svg>
      <span className="zorai-context-tab__label">{collapsedLabel}</span>
    </button>
  );
  if (!open && !everOpened) return collapsedTab;

  return (
    <>
      {collapsedTab}
      <aside id="zorai-context-panel" className="zorai-context-panel" aria-label={title} style={open ? undefined : { display: "none" }}>
        {headerless ? null : <div className="zorai-context-header">
          <div>
            <div className="zorai-context-title">{title}</div>
            {subtitle && subtitle !== title && <div className="zorai-context-subtitle">{subtitle}</div>}
          </div>
          <button
            type="button"
            className="zorai-icon-button zorai-context-panel__close"
            onClick={onToggle}
            title="Collapse context"
            aria-expanded={true}
            aria-controls="zorai-context-panel"
            aria-label="Collapse context"
          >
            <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" aria-hidden="true"><path d="M6 6l12 12M18 6L6 18" /></svg>
          </button>
        </div>}
        <div className="zorai-context-body">{children}</div>
      </aside>
    </>
  );
}
