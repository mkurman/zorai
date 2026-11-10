import { useEffect, useRef, useState, type MutableRefObject } from "react";
import type { SearchAddon } from "@xterm/addon-search";

export function TerminalFindBar({ searchAddonRef, onClose }: {
  searchAddonRef: MutableRefObject<SearchAddon | null>;
  onClose: () => void;
}) {
  const [query, setQuery] = useState("");
  const [found, setFound] = useState<boolean | null>(null);
  const inputRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    inputRef.current?.focus();
    return () => searchAddonRef.current?.clearDecorations();
  }, [searchAddonRef]);

  const find = (direction: "next" | "previous", incremental = false) => {
    const addon = searchAddonRef.current;
    if (!addon || !query) {
      setFound(null);
      return;
    }
    setFound(direction === "next" ? addon.findNext(query, { incremental }) : addon.findPrevious(query));
  };

  return (
    <div className="zorai-terminal-find" role="search" style={{ position: "absolute", top: 8, right: 14, zIndex: 20 }} onPointerDown={(event) => event.stopPropagation()}>
      <input
        ref={inputRef}
        value={query}
        placeholder="Find"
        aria-label="Find in terminal"
        className={found === false ? "is-missing" : undefined}
        onChange={(event) => {
          setQuery(event.target.value);
          const addon = searchAddonRef.current;
          if (!addon || !event.target.value) {
            addon?.clearDecorations();
            setFound(null);
            return;
          }
          setFound(addon.findNext(event.target.value, { incremental: true }));
        }}
        onKeyDown={(event) => {
          event.stopPropagation();
          if (event.key === "Escape") onClose();
          if (event.key === "Enter") find(event.shiftKey ? "previous" : "next");
        }}
      />
      <button type="button" title="Previous match (Shift+Enter)" aria-label="Previous match" onClick={() => find("previous")}>↑</button>
      <button type="button" title="Next match (Enter)" aria-label="Next match" onClick={() => find("next")}>↓</button>
      <button type="button" title="Close (Escape)" aria-label="Close find" onClick={onClose}>×</button>
    </div>
  );
}
