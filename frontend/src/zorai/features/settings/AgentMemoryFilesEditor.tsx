import { useEffect, useState } from "react";
import { ViewModeToggle } from "@/components/data-viewers/ViewModeToggle";
import { getBridge } from "@/lib/bridge";
import { pushToast } from "@/lib/toastStore";
import { PromptEditor } from "./PromptEditor";
import { AGENT_MEMORY_FILE_KEYS, memoryCharCount, parseAgentMemoryFiles, type AgentMemoryFileKey, type AgentMemoryFiles } from "./agentMemoryFiles";

const FILE_TABS: ReadonlyArray<{ id: AgentMemoryFileKey; label: string }> = [
  { id: "soul", label: "SOUL.md" },
  { id: "memory", label: "MEMORY.md" },
  { id: "user", label: "USER.md" },
];

const FILE_HINTS: Record<AgentMemoryFileKey, string> = {
  soul: "Identity and voice. Only this agent reads it.",
  memory: "Durable facts and strategy. Only this agent reads it.",
  user: "Operator profile shared by every agent. Saving replaces the profile text; lines are kept as bullets.",
};

const EMPTY_DRAFTS: Record<AgentMemoryFileKey, string> = { soul: "", memory: "", user: "" };

export function AgentMemoryFilesEditor({ agentId, agentName }: { agentId: string; agentName: string }) {
  const [active, setActive] = useState<AgentMemoryFileKey>("soul");
  const [files, setFiles] = useState<AgentMemoryFiles | null>(null);
  const [drafts, setDrafts] = useState<Record<AgentMemoryFileKey, string>>(EMPTY_DRAFTS);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  const applyResult = (raw: unknown) => {
    const parsed = parseAgentMemoryFiles(raw);
    if (!parsed.files) return parsed.error;
    setFiles(parsed.files);
    setDrafts({ soul: parsed.files.soul.content, memory: parsed.files.memory.content, user: parsed.files.user.content });
    setError(null);
    return null;
  };

  useEffect(() => {
    let alive = true;
    setFiles(null);
    setError(null);
    const bridge = getBridge();
    if (!bridge?.agentGetMemoryFiles) {
      setError("Memory files need the desktop app.");
      return;
    }
    void bridge.agentGetMemoryFiles(agentId).then((raw) => {
      if (!alive) return;
      const failure = applyResult(raw);
      if (failure) setError(failure);
    });
    return () => { alive = false; };
  }, [agentId]);

  if (error && !files) return <p className="zorai-empty-state">{error}</p>;
  if (!files) return <p className="zorai-empty-state">Loading memory files…</p>;

  const file = files[active];
  const draft = drafts[active];
  const dirty = draft !== file.content;
  const count = memoryCharCount(draft);
  const over = file.limitChars > 0 && count > file.limitChars;
  const dirtyKeys = AGENT_MEMORY_FILE_KEYS.filter((key) => drafts[key] !== files[key].content);

  const save = async () => {
    const bridge = getBridge();
    if (!bridge?.agentSaveMemoryFile || saving) return;
    setSaving(true);
    try {
      const raw = await bridge.agentSaveMemoryFile(agentId, active, draft);
      const parsed = parseAgentMemoryFiles(raw);
      if (!parsed.files) {
        pushToast(parsed.error ?? "Could not save memory file.", "error");
        return;
      }
      setFiles(parsed.files);
      setDrafts((current) => ({ ...current, [active]: parsed.files![active].content }));
      pushToast(`${agentName}: ${FILE_TABS.find((tab) => tab.id === active)?.label} saved.`, "info");
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="zorai-memory-files">
      <div className="zorai-memory-files__bar">
        <ViewModeToggle
          modes={FILE_TABS.map((tab) => ({ id: tab.id, label: dirtyKeys.includes(tab.id) ? `${tab.label} •` : tab.label }))}
          value={active}
          onChange={setActive}
          label={`${agentName} memory file`}
        />
        <span className={over ? "zorai-memory-files__count zorai-memory-files__count--over" : "zorai-memory-files__count"}>
          {count} / {file.limitChars} chars
        </span>
      </div>
      <p className="zorai-memory-files__hint">{FILE_HINTS[active]} <code title={file.path}>{file.path}</code></p>
      <PromptEditor
        label={`${agentName} ${active} file`}
        path={`memory/${agentId}/${active}.md`}
        value={draft}
        onChange={(value) => setDrafts((current) => ({ ...current, [active]: value }))}
      />
      <div className="zorai-memory-files__actions">
        <button type="button" className="zorai-ghost-button" disabled={!dirty || saving} onClick={() => setDrafts((current) => ({ ...current, [active]: file.content }))}>
          Revert
        </button>
        <button type="button" className="zorai-primary-button" disabled={!dirty || saving || over || !draft.trim()} onClick={() => void save()}>
          {saving ? "Saving…" : "Save"}
        </button>
      </div>
    </div>
  );
}
