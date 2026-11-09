import { useMemo, useState } from "react";
import { enabledSkillNames, setPreferredSkill, useSkillDocuments, useSkillLibrary } from "./skillLibrary";

export function PreferredSkillsEditor({ agentId, agentName }: { agentId: string; agentName: string }) {
  const { library, update } = useSkillLibrary();
  const documents = useSkillDocuments();
  const [query, setQuery] = useState("");
  const available = useMemo(() => enabledSkillNames(documents, library.disabled), [documents, library.disabled]);
  const preferred = (library.preferred[agentId] ?? []).filter((skill) => available.some((name) => name.toLowerCase() === skill.toLowerCase()));
  const listId = `zorai-preferred-skills-${agentId}`;
  const add = (raw: string) => {
    const match = available.find((name) => name.toLowerCase() === raw.trim().toLowerCase());
    if (!match) return;
    setQuery("");
    void update(setPreferredSkill(library, agentId, match, true));
  };

  return (
    <div className="zorai-preferred-skills">
      <p className="zorai-preferred-skills__hint">
        {agentName} sees these names in a "Preferred skills" prompt section and reads them with <code>read_skill</code> when relevant. Their content is never injected automatically.
      </p>
      {preferred.length > 0 ? (
        <ul className="zorai-preferred-skills__list">
          {preferred.map((skill) => (
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
        </ul>
      ) : (
        <p className="zorai-empty-state">No preferred skills yet.</p>
      )}
      <form
        className="zorai-preferred-skills__add"
        onSubmit={(event) => {
          event.preventDefault();
          add(query);
        }}
      >
        <input
          className="zorai-input"
          list={listId}
          value={query}
          placeholder="Add a skill by name…"
          onChange={(event) => setQuery(event.target.value)}
        />
        <datalist id={listId}>
          {available
            .filter((name) => !preferred.some((skill) => skill.toLowerCase() === name.toLowerCase()))
            .map((name) => <option key={name} value={name} />)}
        </datalist>
        <button type="submit" className="zorai-ghost-button" disabled={!available.some((name) => name.toLowerCase() === query.trim().toLowerCase())}>
          Add
        </button>
      </form>
    </div>
  );
}
