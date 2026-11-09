import { useMemo, useRef, useState } from "react";
import { preferableSkills, searchPreferableSkills, setPreferredSkill, useSkillDocuments, useSkillLibrary } from "./skillLibrary";

const PREFERRED_SKILLS_COLLAPSED_COUNT = 5;
const SUGGESTIONS_MAX_HEIGHT = 308;

function shouldOpenUpward(input: HTMLElement): boolean {
  let top = 0;
  let bottom = window.innerHeight;
  for (let element = input.parentElement; element; element = element.parentElement) {
    const overflow = getComputedStyle(element).overflowY;
    if (overflow === "auto" || overflow === "scroll") {
      const bounds = element.getBoundingClientRect();
      top = Math.max(top, bounds.top);
      bottom = Math.min(bottom, bounds.bottom);
      break;
    }
  }
  const rect = input.getBoundingClientRect();
  const below = bottom - rect.bottom;
  return below < SUGGESTIONS_MAX_HEIGHT && rect.top - top > below;
}

export function PreferredSkillsEditor({ agentId, agentName }: { agentId: string; agentName: string }) {
  const { library, update } = useSkillLibrary();
  const documents = useSkillDocuments();
  const [query, setQuery] = useState("");
  const [expanded, setExpanded] = useState(false);
  const [open, setOpen] = useState(false);
  const [highlighted, setHighlighted] = useState(0);
  const [upward, setUpward] = useState(false);
  const inputRef = useRef<HTMLInputElement | null>(null);
  const available = useMemo(() => preferableSkills(documents, library.disabled), [documents, library.disabled]);
  const preferred = (library.preferred[agentId] ?? []).filter((skill) => available.some((entry) => entry.name.toLowerCase() === skill.toLowerCase()));
  const visible = expanded ? preferred : preferred.slice(0, PREFERRED_SKILLS_COLLAPSED_COUNT);
  const suggestions = useMemo(() => searchPreferableSkills(available, query, preferred), [available, query, preferred]);
  const listId = `zorai-preferred-skills-${agentId}`;
  const openSuggestions = () => {
    if (inputRef.current) setUpward(shouldOpenUpward(inputRef.current));
    setOpen(true);
  };
  const add = (name: string) => {
    setQuery("");
    setHighlighted(0);
    void update(setPreferredSkill(library, agentId, name, true));
    inputRef.current?.focus();
  };

  return (
    <div className="zorai-preferred-skills">
      <p className="zorai-preferred-skills__hint">
        {agentName} sees these names in a "Preferred skills" prompt section and reads them with <code>read_skill</code> when relevant. Their content is never injected automatically. Agent-generated skills are not listed.
      </p>
      <div className="zorai-preferred-skills__combo">
        <input
          ref={inputRef}
          className="zorai-input"
          role="combobox"
          aria-expanded={open && suggestions.length > 0}
          aria-controls={listId}
          aria-autocomplete="list"
          aria-activedescendant={open && suggestions[highlighted] ? `${listId}-${highlighted}` : undefined}
          value={query}
          placeholder="Search skills to add…"
          onFocus={openSuggestions}
          onBlur={() => setOpen(false)}
          onChange={(event) => {
            setQuery(event.target.value);
            setHighlighted(0);
            openSuggestions();
          }}
          onKeyDown={(event) => {
            if (event.key === "ArrowDown" || event.key === "ArrowUp") {
              event.preventDefault();
              if (!open) openSuggestions();
              if (suggestions.length === 0) return;
              const delta = event.key === "ArrowDown" ? 1 : -1;
              setHighlighted((current) => (current + delta + suggestions.length) % suggestions.length);
            } else if (event.key === "Enter") {
              event.preventDefault();
              const pick = suggestions[highlighted];
              if (open && pick) add(pick.name);
            } else if (event.key === "Escape") {
              setOpen(false);
            }
          }}
        />
        {open && suggestions.length > 0 ? (
          <ul id={listId} className={upward ? "zorai-preferred-skills__suggestions zorai-preferred-skills__suggestions--up" : "zorai-preferred-skills__suggestions"} role="listbox" aria-label="Matching skills">
            {suggestions.map((skill, index) => (
              <li
                key={skill.path}
                id={`${listId}-${index}`}
                role="option"
                aria-selected={index === highlighted}
                className={index === highlighted ? "is-highlighted" : undefined}
                onMouseDown={(event) => {
                  event.preventDefault();
                  add(skill.name);
                }}
                onMouseEnter={() => setHighlighted(index)}
              >
                <span className="zorai-preferred-skills__suggestion-name">{skill.name}</span>
                <span className="zorai-preferred-skills__suggestion-path">{skill.path.replace(/\/?SKILL\.md$/i, "") || skill.path}</span>
              </li>
            ))}
          </ul>
        ) : null}
        {open && query.trim() && suggestions.length === 0 ? (
          <div className={upward ? "zorai-preferred-skills__suggestions zorai-preferred-skills__suggestions--empty zorai-preferred-skills__suggestions--up" : "zorai-preferred-skills__suggestions zorai-preferred-skills__suggestions--empty"}>No matching skills.</div>
        ) : null}
      </div>
      {preferred.length > 0 ? (
        <ul className="zorai-preferred-skills__list">
          {visible.map((skill) => (
            <li key={skill} className="zorai-preferred-skills__chip">
              <span>{skill}</span>
              <button
                type="button"
                aria-label={`Remove ${skill} from ${agentName}'s preferred skills`}
                title="Remove"
                onClick={() => void update(setPreferredSkill(library, agentId, skill, false))}
              >
                ×
              </button>
            </li>
          ))}
          {preferred.length > PREFERRED_SKILLS_COLLAPSED_COUNT ? (
            <li>
              <button type="button" className="zorai-preferred-skills__more" aria-expanded={expanded} onClick={() => setExpanded((value) => !value)}>
                {expanded ? "Show less" : `Show ${preferred.length - PREFERRED_SKILLS_COLLAPSED_COUNT} more`}
              </button>
            </li>
          ) : null}
        </ul>
      ) : (
        <p className="zorai-empty-state">No preferred skills yet.</p>
      )}
    </div>
  );
}
