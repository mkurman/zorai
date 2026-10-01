import type { ReactNode } from "react";

type ZoraiContextPanelProps = {
  title: string;
  subtitle?: string;
  collapsedLabel?: string;
  open: boolean;
  onToggle: () => void;
  children: ReactNode;
};

export function ZoraiContextPanel({
  title,
  subtitle,
  collapsedLabel = "Context",
  open,
  onToggle,
  children,
}: ZoraiContextPanelProps) {
  if (!open) {
    return (
      <button
        type="button"
        className="zorai-context-tab"
        onClick={onToggle}
        title={collapsedLabel}
        aria-expanded={false}
        aria-controls="zorai-context-panel"
        aria-label={collapsedLabel}
      >
        {collapsedLabel}
      </button>
    );
  }

  return (
    <aside id="zorai-context-panel" className="zorai-context-panel" aria-label={title}>
      <div className="zorai-context-header">
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
          x
        </button>
      </div>
      <div className="zorai-context-body">{children}</div>
    </aside>
  );
}
