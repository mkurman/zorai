import { useEffect, useRef } from "react";
import type { MentionOption } from "./composerMentions";

export function ComposerMentionMenu({ options, activeIndex, onPick, onHover }: {
  options: MentionOption[];
  activeIndex: number;
  onPick: (option: MentionOption) => void;
  onHover: (index: number) => void;
}) {
  const listRef = useRef<HTMLUListElement>(null);

  useEffect(() => {
    listRef.current?.querySelector<HTMLElement>(`[data-index="${activeIndex}"]`)?.scrollIntoView({ block: "nearest" });
  }, [activeIndex]);

  if (options.length === 0) return <div className="zorai-mention-menu zorai-mention-menu--empty">No matching files or context</div>;

  return (
    <ul ref={listRef} className="zorai-mention-menu" role="listbox" aria-label="Add context">
      {options.map((option, index) => (
        <li
          key={`${option.kind}:${option.id}`}
          data-index={index}
          role="option"
          aria-selected={index === activeIndex}
          className={index === activeIndex ? "is-active" : undefined}
          onMouseDown={(event) => {
            event.preventDefault();
            onPick(option);
          }}
          onMouseEnter={() => onHover(index)}
        >
          <span className={`zorai-mention-menu__kind zorai-mention-menu__kind--${option.kind}`} aria-hidden="true">{option.kind === "file" ? "📄" : "@"}</span>
          <span className="zorai-mention-menu__label">{option.label}</span>
          {option.detail ? <span className="zorai-mention-menu__detail">{option.detail}</span> : null}
        </li>
      ))}
    </ul>
  );
}
