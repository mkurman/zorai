export function ViewModeToggle<T extends string>({
  modes,
  value,
  onChange,
  label,
}: {
  modes: ReadonlyArray<{ id: T; label: string }>;
  value: T;
  onChange: (mode: T) => void;
  label: string;
}) {
  return (
    <div className="zorai-view-toggle" role="group" aria-label={label}>
      {modes.map((mode) => (
        <button
          key={mode.id}
          type="button"
          className={`zorai-view-toggle__button${value === mode.id ? " zorai-view-toggle__button--active" : ""}`}
          aria-pressed={value === mode.id}
          onClick={() => onChange(mode.id)}
        >
          {mode.label}
        </button>
      ))}
    </div>
  );
}
