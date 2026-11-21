const ICON_PATHS = {
  copy: "M9 9h10v10H9zM5 15V5h10",
  image: "M4 5h16v14H4zM4 15l4-4 4 4 3-3 5 5M15 9.5h.01",
  expand: "M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5",
} as const;

export function TableIconButton({ icon, label, onClick }: { icon: keyof typeof ICON_PATHS; label: string; onClick: () => void }) {
  return (
    <button type="button" className="zorai-data-table__action zorai-data-table__action--icon" title={label} aria-label={label} onClick={onClick}>
      <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false"><path d={ICON_PATHS[icon]} /></svg>
    </button>
  );
}
