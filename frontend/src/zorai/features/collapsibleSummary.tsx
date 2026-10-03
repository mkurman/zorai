import { useState, type MouseEvent } from "react";

const SUMMARY_LIMIT = 180;

export function CollapsibleSummary({ text, className }: { text: string; className?: string }) {
  const [expanded, setExpanded] = useState(false);
  const collapsible = text.length > SUMMARY_LIMIT;
  const visible = !collapsible || expanded ? text : `${text.slice(0, SUMMARY_LIMIT - 1)}…`;

  const toggle = (event: MouseEvent<HTMLButtonElement>) => {
    event.stopPropagation();
    setExpanded((current) => !current);
  };

  return (
    <p className={className}>
      {visible}
      {collapsible ? (
        <button
          type="button"
          className="zorai-run-card__more"
          aria-expanded={expanded}
          onClick={toggle}
        >
          {expanded ? "Show less" : "Show more"}
        </button>
      ) : null}
    </p>
  );
}
