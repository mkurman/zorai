import type { ButtonHTMLAttributes } from "react";

type RefreshButtonProps = {
  label?: string;
  busy?: boolean;
} & Omit<ButtonHTMLAttributes<HTMLButtonElement>, "children">;

export function RefreshIcon() {
  return (
    <svg className="zorai-refresh-icon" viewBox="0 0 24 24" aria-hidden="true" focusable="false">
      <path d="M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8" />
      <path d="M21 3v5h-5" />
      <path d="M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16" />
      <path d="M8 16H3v5" />
    </svg>
  );
}

export function RefreshButton({
  label = "Refresh",
  busy = false,
  className,
  disabled,
  type = "button",
  title,
  "aria-label": ariaLabel,
  ...rest
}: RefreshButtonProps) {
  return (
    <button
      type={type}
      className={["zorai-icon-button", "zorai-refresh-button", className].filter(Boolean).join(" ")}
      disabled={disabled}
      aria-busy={busy || undefined}
      {...rest}
      aria-label={ariaLabel ?? title ?? label}
      title={title ?? label}
    >
      <RefreshIcon />
    </button>
  );
}
