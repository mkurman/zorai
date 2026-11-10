import { useMemo, useState } from "react";

export type GitDialogOption = { value: string; label: string };

export type GitDialogField =
  | { key: string; kind: "text"; label: string; placeholder?: string; optional?: boolean; initial?: string; multiline?: boolean }
  | { key: string; kind: "pick"; label: string; options: GitDialogOption[]; optional?: boolean; initial?: string; allowCustom?: boolean }
  | { key: string; kind: "checkbox"; label: string; initial?: boolean };

export type GitDialogValues = Record<string, string | boolean>;

export type GitDialogSpec = {
  title: string;
  description?: string;
  fields: GitDialogField[];
  submitLabel: string;
  danger?: boolean;
  onSubmit: (values: GitDialogValues) => Promise<void> | void;
};

export function initialGitDialogValues(fields: GitDialogField[]): GitDialogValues {
  return Object.fromEntries(fields.map((field) => [field.key, field.kind === "checkbox" ? Boolean(field.initial) : field.initial ?? ""]));
}

export function gitDialogReady(fields: GitDialogField[], values: GitDialogValues): boolean {
  return fields.every((field) => field.kind === "checkbox" || field.optional || String(values[field.key] ?? "").trim().length > 0);
}

export function GitActionDialog({ spec, busy, error, onClose }: {
  spec: GitDialogSpec;
  busy: boolean;
  error: string | null;
  onClose: () => void;
}) {
  const [values, setValues] = useState<GitDialogValues>(() => initialGitDialogValues(spec.fields));
  const ready = gitDialogReady(spec.fields, values);
  const set = (key: string, value: string | boolean) => setValues((current) => ({ ...current, [key]: value }));

  return (
    <div className="zorai-code-dialog-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget && !busy) onClose(); }}>
      <form
        className="zorai-code-close-dialog zorai-workspace-path-dialog zorai-git-dialog"
        role="dialog"
        aria-modal="true"
        aria-label={spec.title}
        onKeyDown={(event) => { if (event.key === "Escape" && !busy) onClose(); }}
        onSubmit={(event) => {
          event.preventDefault();
          if (ready && !busy) void spec.onSubmit(values);
        }}
      >
        <h2>{spec.title}</h2>
        {spec.description ? <p className="zorai-git-dialog__description">{spec.description}</p> : null}
        {spec.fields.map((field, index) => {
          if (field.kind === "checkbox") {
            return (
              <label key={field.key} className="zorai-git-dialog__check">
                <input type="checkbox" checked={Boolean(values[field.key])} disabled={busy} onChange={(event) => set(field.key, event.target.checked)} />
                <span>{field.label}</span>
              </label>
            );
          }
          if (field.kind === "pick") {
            return <GitDialogPicker key={field.key} field={field} value={String(values[field.key] ?? "")} busy={busy} autoFocus={index === 0} onChange={(value) => set(field.key, value)} />;
          }
          return (
            <label key={field.key}>
              <span>{field.label}{field.optional ? " (optional)" : ""}</span>
              {field.multiline ? (
                <textarea value={String(values[field.key] ?? "")} disabled={busy} autoFocus={index === 0} rows={3} placeholder={field.placeholder} onChange={(event) => set(field.key, event.target.value)} />
              ) : (
                <input value={String(values[field.key] ?? "")} disabled={busy} autoFocus={index === 0} placeholder={field.placeholder} onChange={(event) => set(field.key, event.target.value)} />
              )}
            </label>
          );
        })}
        {error ? <p role="alert">{error}</p> : null}
        <footer>
          <button type="button" disabled={busy} onClick={onClose}>Cancel</button>
          <button type="submit" className={spec.danger ? "is-primary is-danger" : "is-primary"} disabled={busy || !ready} autoFocus={spec.fields.length === 0}>{busy ? "Working…" : spec.submitLabel}</button>
        </footer>
      </form>
    </div>
  );
}

function GitDialogPicker({ field, value, busy, autoFocus, onChange }: {
  field: Extract<GitDialogField, { kind: "pick" }>;
  value: string;
  busy: boolean;
  autoFocus: boolean;
  onChange: (value: string) => void;
}) {
  const [query, setQuery] = useState("");
  const visible = useMemo(() => {
    const needle = query.trim().toLowerCase();
    return needle ? field.options.filter((option) => option.label.toLowerCase().includes(needle)) : field.options;
  }, [field.options, query]);
  const custom = field.allowCustom && query.trim() && !field.options.some((option) => option.value === query.trim()) ? query.trim() : null;

  return (
    <div className="zorai-git-dialog__picker">
      <span>{field.label}{field.optional ? " (optional)" : ""}</span>
      <input
        value={query}
        disabled={busy}
        autoFocus={autoFocus}
        placeholder={field.allowCustom ? "Search, or type a ref" : "Search"}
        aria-label={`Search ${field.label}`}
        onChange={(event) => setQuery(event.target.value)}
        onKeyDown={(event) => {
          if (event.key !== "Enter") return;
          const first = custom ?? visible[0]?.value;
          if (first && first !== value) {
            event.preventDefault();
            onChange(first);
          }
        }}
      />
      <div className="zorai-git-dialog__options" role="listbox" aria-label={field.label}>
        {custom ? (
          <button type="button" role="option" aria-selected={value === custom} className={value === custom ? "is-selected" : undefined} disabled={busy} onClick={() => onChange(custom)}>
            Use “{custom}”
          </button>
        ) : null}
        {visible.length === 0 && !custom ? <div className="zorai-git-dialog__empty">{field.options.length === 0 ? "Nothing to choose from." : "No matches."}</div> : null}
        {visible.map((option) => (
          <button
            type="button"
            role="option"
            key={option.value}
            aria-selected={value === option.value}
            className={value === option.value ? "is-selected" : undefined}
            disabled={busy}
            onClick={() => onChange(option.value)}
          >
            {option.label}
          </button>
        ))}
      </div>
    </div>
  );
}

export function GitOutputDialog({ entries, onClose }: {
  entries: Array<{ cwd: string; command: string; ok: boolean; output: string; at: number }>;
  onClose: () => void;
}) {
  return (
    <div className="zorai-code-dialog-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget) onClose(); }}>
      <div className="zorai-code-close-dialog zorai-git-output" role="dialog" aria-modal="true" aria-label="Git output" onKeyDown={(event) => { if (event.key === "Escape") onClose(); }}>
        <h2>Git Output</h2>
        <div className="zorai-git-output__list">
          {entries.length === 0 ? <p className="zorai-git-dialog__description">No git commands have run yet in this session.</p> : null}
          {[...entries].reverse().map((entry, index) => (
            <article key={`${entry.at}:${index}`} className={entry.ok ? undefined : "is-error"}>
              <header>
                <code>{entry.command}</code>
                <time>{new Date(entry.at).toLocaleTimeString()}</time>
              </header>
              <small>{entry.cwd}</small>
              {entry.output ? <pre>{entry.output}</pre> : null}
            </article>
          ))}
        </div>
        <footer>
          <button type="button" className="is-primary" autoFocus onClick={onClose}>Close</button>
        </footer>
      </div>
    </div>
  );
}
