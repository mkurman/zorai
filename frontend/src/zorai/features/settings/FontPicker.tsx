import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { filterFontOptions } from "./terminalFontOptions";
import { usePresence } from "@/hooks/usePresence";

export function FontPicker({
  value,
  options,
  onChange,
  defaultLabel,
  previewFont,
}: {
  value: string;
  options: readonly string[];
  onChange: (font: string) => void;
  defaultLabel?: string;
  previewFont?: string;
}) {
  const [open, setOpen] = useState(false);
  const presence = usePresence(open);
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const rootRef = useRef<HTMLDivElement | null>(null);
  const listRef = useRef<HTMLDivElement | null>(null);

  const entries = useMemo(() => {
    const fonts = filterFontOptions(options, query).map((font) => ({ value: font, label: font }));
    const showDefault = defaultLabel !== undefined && (!query.trim() || defaultLabel.toLowerCase().includes(query.trim().toLowerCase()));
    return showDefault ? [{ value: "", label: defaultLabel }, ...fonts] : fonts;
  }, [defaultLabel, options, query]);

  useEffect(() => {
    if (!open) return;
    const onPointerDown = (event: PointerEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) setOpen(false);
    };
    document.addEventListener("pointerdown", onPointerDown);
    return () => document.removeEventListener("pointerdown", onPointerDown);
  }, [open]);

  useEffect(() => {
    setActive(0);
  }, [query]);

  useEffect(() => {
    listRef.current?.querySelector<HTMLElement>(`[data-index="${active}"]`)?.scrollIntoView({ block: "nearest" });
  }, [active, open]);

  const pick = (font: string) => {
    onChange(font);
    setOpen(false);
  };

  const openPicker = () => {
    setQuery("");
    const index = entries.findIndex((entry) => entry.value === value);
    setActive(Math.max(0, index));
    setOpen(true);
  };

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      setActive((index) => Math.min(entries.length - 1, index + 1));
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      setActive((index) => Math.max(0, index - 1));
    } else if (event.key === "Enter") {
      event.preventDefault();
      const entry = entries[active];
      if (entry) pick(entry.value);
    } else if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      setOpen(false);
    }
  };

  const currentLabel = value ? value : defaultLabel ?? "";

  return (
    <div className="zorai-font-picker" ref={rootRef}>
      {open ? (
        <input
          className="zorai-input"
          autoFocus
          value={query}
          placeholder="Search fonts…"
          onChange={(event) => setQuery(event.target.value)}
          onKeyDown={onKeyDown}
        />
      ) : (
        <button
          type="button"
          className="zorai-input zorai-font-picker__trigger"
          style={{ fontFamily: previewFont ?? value }}
          onClick={openPicker}
        >
          {currentLabel}
        </button>
      )}
      {presence.mounted ? (
        <div data-closing={presence.closing || undefined} className="zorai-font-picker__list" ref={listRef} role="listbox">
          {entries.length === 0 ? <div className="zorai-font-picker__empty">No matching fonts</div> : null}
          {entries.map((entry, index) => (
            <button
              key={entry.value || "__default"}
              type="button"
              role="option"
              aria-selected={entry.value === value}
              data-index={index}
              className={`zorai-font-picker__option${index === active ? " is-active" : ""}${entry.value === value ? " is-selected" : ""}`}
              style={entry.value ? { fontFamily: entry.value } : undefined}
              onMouseEnter={() => setActive(index)}
              onClick={() => pick(entry.value)}
            >
              {entry.label}
            </button>
          ))}
        </div>
      ) : null}
    </div>
  );
}
