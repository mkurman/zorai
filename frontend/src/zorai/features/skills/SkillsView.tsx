import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { getBridge } from "@/lib/bridge";
import { useAgentStore } from "@/lib/agentStore";
import { MonacoReadOnlyView, WorkspaceCodeEditor } from "@/components/WorkspaceCodeEditor";
import { CodeDocumentPreview, codePreviewKind } from "../code/CodeDocumentPreview";
import { CodeFileIcon, CodeFolderChevron } from "../code/CodeFileIcon";
import { useCodeEditorSettingsStore } from "../code/codeEditorSettingsStore";
import {
  agentsPreferringSkill,
  coveringSkillKey,
  invalidateSkillDocuments,
  isBlankSkillDraft,
  isSkillPathCovered,
  preferredSkillAgents,
  resolveSkillsRoot,
  setPreferredSkill,
  skillKeyForPath,
  skillNameForPath,
  toggleSkillKey,
  useSkillLibrary,
} from "./skillLibrary";
import { isValidSkillName, joinSkillPath, parentDir, useSkillsViewStore, type OpenSkillDocument } from "./skillsViewStore";

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function useSkillsRoot(): string | null {
  const [root, setRoot] = useState<string | null>(null);
  useEffect(() => {
    let cancelled = false;
    void resolveSkillsRoot().then(async (resolved) => {
      if (!resolved) return;
      await getBridge()?.workspaceCreateDirectory?.(resolved, "").catch(() => undefined);
      if (!cancelled) setRoot(resolved);
    });
    return () => {
      cancelled = true;
    };
  }, []);
  return root;
}

function afterTreeChange() {
  invalidateSkillDocuments();
  useSkillsViewStore.getState().refresh();
}

function NameDialog({ title, label, placeholder, initial = "", submitLabel, onSubmit, onClose }: {
  title: string;
  label: string;
  placeholder: string;
  initial?: string;
  submitLabel: string;
  onSubmit: (value: string) => Promise<void>;
  onClose: () => void;
}) {
  const [value, setValue] = useState(initial);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement | null>(null);
  useEffect(() => {
    inputRef.current?.focus();
    inputRef.current?.select();
  }, []);
  return (
    <div className="zorai-code-dialog-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget && !busy) onClose(); }}>
      <form
        className="zorai-code-close-dialog zorai-workspace-path-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="zorai-skill-name-dialog-title"
        onSubmit={(event) => {
          event.preventDefault();
          setBusy(true);
          setError(null);
          void onSubmit(value).then(onClose, (failure) => {
            setError(errorMessage(failure));
            setBusy(false);
          });
        }}
      >
        <h2 id="zorai-skill-name-dialog-title">{title}</h2>
        <label>
          <span>{label}</span>
          <input ref={inputRef} value={value} disabled={busy} placeholder={placeholder} onChange={(event) => setValue(event.target.value)} onKeyDown={(event) => { if (event.key === "Escape" && !busy) onClose(); }} />
        </label>
        {error ? <p role="alert">{error}</p> : null}
        <footer>
          <button type="button" disabled={busy} onClick={onClose}>Cancel</button>
          <button type="submit" className="is-primary" disabled={busy || !value.trim()}>{busy ? "Working…" : submitLabel}</button>
        </footer>
      </form>
    </div>
  );
}

function SkillBadges({ path }: { path: string }) {
  const { library } = useSkillLibrary();
  const locked = isSkillPathCovered(library.locked, path);
  const disabled = isSkillPathCovered(library.disabled, path);
  const name = skillNameForPath(path);
  const preferredBy = name ? agentsPreferringSkill(library, name).length : 0;
  return (
    <>
      {locked ? <span className="zorai-skill-badge zorai-skill-badge--locked" title="Locked">locked</span> : null}
      {disabled ? <span className="zorai-skill-badge" title="Disabled">off</span> : null}
      {preferredBy > 0 ? <span className="zorai-skill-badge zorai-skill-badge--preferred" title={`Preferred by ${preferredBy} agent(s)`}>★</span> : null}
    </>
  );
}

function SkillTreeNode({ root, entry, depth, expanded, childrenByPath, onToggle }: {
  root: string;
  entry: ZoraiWorkspaceEntry;
  depth: number;
  expanded: Set<string>;
  childrenByPath: Map<string, ZoraiWorkspaceEntry[]>;
  onToggle: (path: string) => void;
}) {
  const selectedDir = useSkillsViewStore((state) => state.selectedDir);
  const openPath = useSkillsViewStore((state) => state.open?.path ?? null);
  const { library } = useSkillLibrary();
  const isOpen = expanded.has(entry.path);
  const disabled = isSkillPathCovered(library.disabled, entry.path);
  const active = entry.isDirectory ? selectedDir === entry.path : openPath === entry.path;
  const open = (mode: "preview" | "editor") => {
    const store = useSkillsViewStore.getState();
    store.selectDir(parentDir(entry.path));
    store.openDocument(entry.path, mode);
  };
  return (
    <div>
      <div
        className={["zorai-skill-tree-row", active ? "zorai-skill-tree-row--active" : "", disabled ? "zorai-skill-tree-row--disabled" : ""].filter(Boolean).join(" ")}
        style={{ paddingLeft: 8 + depth * 14 }}
      >
        <button
          type="button"
          role="treeitem"
          aria-expanded={entry.isDirectory ? isOpen : undefined}
          aria-selected={active}
          className="zorai-skill-tree-row__main"
          onClick={() => {
            if (entry.isDirectory) {
              useSkillsViewStore.getState().selectDir(entry.path);
              onToggle(entry.path);
            } else {
              open("preview");
            }
          }}
        >
          {entry.isDirectory ? <span className="zorai-workspace-chevron"><CodeFolderChevron expanded={isOpen} /></span> : <CodeFileIcon path={entry.path} />}
          <span className="zorai-skill-tree-row__name">{entry.name}</span>
          <SkillBadges path={entry.path} />
        </button>
        {entry.isDirectory ? null : (
          <span className="zorai-skill-tree-row__actions">
            <button type="button" onClick={() => open("preview")}>show</button>
            <button type="button" onClick={() => open("editor")}>edit</button>
          </span>
        )}
      </div>
      {entry.isDirectory && isOpen
        ? (childrenByPath.get(entry.path) ?? []).map((child) => (
            <SkillTreeNode key={child.path} root={root} entry={child} depth={depth + 1} expanded={expanded} childrenByPath={childrenByPath} onToggle={onToggle} />
          ))
        : null}
    </div>
  );
}

export function SkillsRail() {
  const root = useSkillsRoot();
  const selectedDir = useSkillsViewStore((state) => state.selectedDir);
  const refreshToken = useSkillsViewStore((state) => state.refreshToken);
  const { library } = useSkillLibrary();
  const [expanded, setExpanded] = useState<Set<string>>(() => new Set([""]));
  const [childrenByPath, setChildrenByPath] = useState<Map<string, ZoraiWorkspaceEntry[]>>(() => new Map());
  const [dialog, setDialog] = useState<"skill" | "folder" | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const targetLocked = selectedDir !== "" && isSkillPathCovered(library.locked, selectedDir);

  const load = useCallback(async (dir: string) => {
    if (!root) return;
    try {
      const entries = (await getBridge()?.workspaceListDirectory?.(root, dir, { includeIgnored: true })) ?? [];
      setChildrenByPath((current) => new Map(current).set(dir, entries.filter((entry) => !entry.name.startsWith("."))));
    } catch (error) {
      setNotice(errorMessage(error));
    }
  }, [root]);

  useEffect(() => {
    for (const dir of expanded) void load(dir);
  }, [load, refreshToken]);

  const toggle = (path: string) => {
    setExpanded((current) => {
      const next = new Set(current);
      if (next.has(path)) next.delete(path);
      else {
        next.add(path);
        void load(path);
      }
      return next;
    });
  };

  const importFrom = async (kind: "file" | "folder") => {
    const bridge = getBridge();
    if (!root || !bridge) return;
    setNotice(null);
    const source = kind === "file"
      ? (await bridge.workspaceSelectFile?.())?.path ?? null
      : (await bridge.workspaceSelectFolder?.())?.root?.root ?? null;
    if (!source) return;
    const name = source.split(/[\\/]/).filter(Boolean).pop() ?? "";
    const relative = joinSkillPath(selectedDir, name);
    const fileExists = await bridge.workspaceStatFile?.(root, relative).then(() => true, () => false);
    const dirExists = await bridge.workspaceListDirectory?.(root, relative).then(() => true, () => false);
    if (fileExists || dirExists) {
      setNotice(`${relative} already exists.`);
      return;
    }
    if (isSkillPathCovered(library.locked, relative)) {
      setNotice(`${relative} is locked.`);
      return;
    }
    await bridge.copyFsPath?.(source, `${root}/${relative}`);
    setExpanded((current) => new Set(current).add(selectedDir));
    afterTreeChange();
    setNotice(`Imported ${relative}.`);
  };

  if (!root) return <p className="zorai-empty-state">Loading skills…</p>;
  const rootEntry: ZoraiWorkspaceEntry = { name: "skills", path: "", isDirectory: true, isSymbolicLink: false, sizeBytes: null, modifiedAt: null };

  return (
    <div className="zorai-skills-rail">
      <div className="zorai-skills-rail__toolbar">
        <button type="button" className="zorai-ghost-button" disabled={targetLocked} onClick={() => setDialog("skill")}>New skill</button>
        <button type="button" className="zorai-ghost-button" disabled={targetLocked} onClick={() => setDialog("folder")}>New folder</button>
        <button type="button" className="zorai-ghost-button" disabled={targetLocked} onClick={() => void importFrom("file")}>Import file</button>
        <button type="button" className="zorai-ghost-button" disabled={targetLocked} onClick={() => void importFrom("folder")}>Import folder</button>
      </div>
      <div className="zorai-skills-rail__target" title="New skills, folders and imports go here">
        into <code>skills/{selectedDir}</code>{targetLocked ? " (locked)" : ""}
      </div>
      {notice ? <p className="zorai-skills-rail__notice" role="status">{notice}</p> : null}
      <div className="zorai-skill-tree" role="tree" aria-label="Skills">
        <SkillTreeNode root={root} entry={rootEntry} depth={0} expanded={expanded} childrenByPath={childrenByPath} onToggle={toggle} />
      </div>
      {dialog === "folder" ? (
        <NameDialog
          title="Create folder"
          label={`Folder name in skills/${selectedDir}`}
          placeholder="research"
          submitLabel="Create folder"
          onClose={() => setDialog(null)}
          onSubmit={async (name) => {
            if (!isValidSkillName(name)) throw new Error("Use a single folder name without slashes.");
            await getBridge()?.workspaceCreateDirectory?.(root, joinSkillPath(selectedDir, name));
            setExpanded((current) => new Set(current).add(selectedDir));
            afterTreeChange();
          }}
        />
      ) : null}
      {dialog === "skill" ? (
        <NameDialog
          title="New skill"
          label={`Skill name in skills/${selectedDir}`}
          placeholder="my-skill"
          submitLabel="Create skill"
          onClose={() => setDialog(null)}
          onSubmit={async (name) => {
            if (!isValidSkillName(name)) throw new Error("Use a single skill name without slashes.");
            const skillDir = joinSkillPath(selectedDir, name);
            const exists = await getBridge()?.workspaceListDirectory?.(root, skillDir).then(() => true, () => false);
            if (exists) throw new Error(`skills/${skillDir} already exists.`);
            useSkillsViewStore.getState().openDraft(`${skillDir}/SKILL.md`);
          }}
        />
      ) : null}
    </div>
  );
}

function PreferredPicker({ skill, onClose }: { skill: string; onClose: () => void }) {
  const { library, update } = useSkillLibrary();
  const subAgents = useAgentStore((state) => state.subAgents);
  const agents = useMemo(() => preferredSkillAgents(subAgents), [subAgents]);
  const preferredBy = agentsPreferringSkill(library, skill);
  const ref = useRef<HTMLDivElement | null>(null);
  useEffect(() => {
    const onPointerDown = (event: PointerEvent) => {
      if (!ref.current?.contains(event.target as Node)) onClose();
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    document.addEventListener("pointerdown", onPointerDown);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [onClose]);
  return (
    <div ref={ref} className="zorai-skill-preferred-picker" role="dialog" aria-label={`Agents preferring ${skill}`}>
      <div className="zorai-section-label">Preferred by</div>
      {agents.map((agent) => {
        const checked = preferredBy.includes(agent.id);
        return (
          <label key={agent.id} className="zorai-skill-preferred-picker__row">
            <input
              type="checkbox"
              checked={checked}
              onChange={() => {
                const latest = useAgentStore.getState().agentSettings.skill_library;
                void update(setPreferredSkill(latest, agent.id, skill, !checked));
              }}
            />
            <span>{agent.name}</span>
          </label>
        );
      })}
    </div>
  );
}

function LibraryToggles({ path, isSkillDoc }: { path: string; isSkillDoc: boolean }) {
  const { library, update } = useSkillLibrary();
  const key = isSkillDoc ? skillKeyForPath(path) : path;
  if (!key) return null;
  const lockedBy = coveringSkillKey(library.locked, key);
  const disabledBy = coveringSkillKey(library.disabled, key);
  const lockInherited = lockedBy !== null && lockedBy !== key;
  const disableInherited = disabledBy !== null && disabledBy !== key;
  return (
    <>
      <button
        type="button"
        className={["zorai-ghost-button", lockedBy ? "zorai-skill-toggle--on" : ""].filter(Boolean).join(" ")}
        disabled={lockInherited}
        title={lockInherited ? `Locked by skills/${lockedBy}` : lockedBy ? "Allow edits and sync overwrites again" : "Protect from edits and sync overwrites"}
        onClick={() => void update({ ...library, locked: toggleSkillKey(library.locked, key) })}
      >
        {lockedBy ? "Unlock" : "Lock"}
      </button>
      <button
        type="button"
        className={["zorai-ghost-button", disabledBy ? "zorai-skill-toggle--on" : ""].filter(Boolean).join(" ")}
        disabled={disableInherited}
        title={disableInherited ? `Disabled by skills/${disabledBy}` : disabledBy ? "Show to agents again" : "Hide from skill lists and preferred skills"}
        onClick={() => {
          invalidateSkillDocuments();
          void update({ ...library, disabled: toggleSkillKey(library.disabled, key) });
        }}
      >
        {disabledBy ? "Enable" : "Disable"}
      </button>
    </>
  );
}

function FolderPane({ root, dir }: { root: string; dir: string }) {
  const { library } = useSkillLibrary();
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const locked = dir !== "" && isSkillPathCovered(library.locked, dir);
  return (
    <section className="zorai-skill-pane zorai-skill-pane--folder">
      <header className="zorai-skill-pane__header">
        <div className="zorai-skill-pane__title">
          <div className="zorai-section-label">Folder</div>
          <h2>skills/{dir}</h2>
        </div>
        {dir ? (
          <div className="zorai-skill-pane__actions">
            <LibraryToggles path={dir} isSkillDoc={false} />
            {confirmDelete ? (
              <>
                <span className="zorai-skill-pane__confirm">Delete folder and everything in it?</span>
                <button
                  type="button"
                  className="zorai-ghost-button zorai-skill-danger"
                  onClick={() => {
                    void getBridge()?.workspaceDeletePath?.(root, dir, { recursive: true }).then(() => {
                      useSkillsViewStore.getState().selectDir(parentDir(dir));
                      afterTreeChange();
                    }, (failure) => setError(errorMessage(failure)));
                  }}
                >
                  Delete
                </button>
                <button type="button" className="zorai-ghost-button" onClick={() => setConfirmDelete(false)}>Cancel</button>
              </>
            ) : (
              <button type="button" className="zorai-ghost-button" disabled={locked} onClick={() => setConfirmDelete(true)}>Delete folder</button>
            )}
          </div>
        ) : null}
      </header>
      {error ? <p role="alert" className="zorai-skill-pane__error">{error}</p> : null}
      <div className="zorai-skill-pane__empty">
        <p>Pick a file in the tree to preview it, or hover a file and choose <strong>edit</strong>.</p>
        <p>New skills, folders and imports are created in <code>skills/{dir}</code>.</p>
        <p><strong>Lock</strong> stops edits and keeps built-in sync, plugins and skill evolution from overwriting. <strong>Disable</strong> hides skills from agents' skill lists and preferred skills. Both apply to everything inside a folder.</p>
      </div>
    </section>
  );
}

function DocumentPane({ root, open }: { root: string; open: OpenSkillDocument }) {
  const { library } = useSkillLibrary();
  const settings = useCodeEditorSettingsStore((state) => state.settings);
  const [content, setContent] = useState("");
  const [original, setOriginal] = useState("");
  const [language, setLanguage] = useState("markdown");
  const [loading, setLoading] = useState(!open.draft);
  const [error, setError] = useState<string | null>(null);
  const [dialog, setDialog] = useState<"rename" | null>(null);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [pickerOpen, setPickerOpen] = useState(false);
  const textareaRef = useRef<HTMLTextAreaElement | null>(null);
  const hashRef = useRef<string | null>(null);
  const contentRef = useRef("");
  const originalRef = useRef("");
  const draftRef = useRef(open.draft);
  const skipCloseSaveRef = useRef(false);
  const locked = isSkillPathCovered(library.locked, open.path);
  const lockedRef = useRef(locked);
  lockedRef.current = locked;
  draftRef.current = open.draft;
  const skillName = skillNameForPath(open.path);
  const isSkillDoc = skillName !== null;
  const dirty = content !== original || open.draft;
  const mode = locked ? "preview" : open.mode;

  useEffect(() => {
    if (open.draft) return;
    let cancelled = false;
    void getBridge()?.workspaceReadFile?.(root, open.path).then((file) => {
      if (cancelled) return;
      hashRef.current = file.hash;
      contentRef.current = file.content;
      originalRef.current = file.content;
      setContent(file.content);
      setOriginal(file.content);
      setLanguage(file.language || "plaintext");
      setLoading(false);
    }, (failure) => {
      if (cancelled) return;
      setError(errorMessage(failure));
      setLoading(false);
    });
    return () => {
      cancelled = true;
    };
  }, [root, open.path, open.draft]);

  const writeDocument = useCallback(async (text: string, draft: boolean) => {
    const bridge = getBridge();
    if (!bridge) throw new Error("Bridge unavailable.");
    if (isSkillPathCovered(useAgentStore.getState().agentSettings.skill_library.locked, open.path)) {
      throw new Error("This skill is locked.");
    }
    if (draft) {
      await bridge.workspaceCreateDirectory?.(root, parentDir(open.path));
      const file = await bridge.workspaceWriteFile?.(root, open.path, text, null);
      hashRef.current = file?.hash ?? null;
      afterTreeChange();
      return;
    }
    const file = await bridge.workspaceWriteFile?.(root, open.path, text, hashRef.current);
    hashRef.current = file?.hash ?? hashRef.current;
  }, [root, open.path]);

  const save = useCallback(async () => {
    setError(null);
    const text = contentRef.current;
    if (open.draft && isBlankSkillDraft(text)) {
      setError("Write the skill content before saving.");
      return;
    }
    try {
      await writeDocument(text, open.draft);
      originalRef.current = text;
      setOriginal(text);
      if (open.draft) useSkillsViewStore.getState().markDraftSaved(open.path);
    } catch (failure) {
      setError(errorMessage(failure));
    }
  }, [open.draft, open.path, writeDocument]);

  useEffect(() => () => {
    if (skipCloseSaveRef.current || lockedRef.current) return;
    const text = contentRef.current;
    if (draftRef.current) {
      if (!isBlankSkillDraft(text)) void writeDocument(text, true).catch(() => undefined);
      return;
    }
    if (text !== originalRef.current) void writeDocument(text, false).catch(() => undefined);
  }, [writeDocument]);

  const deleteTarget = isSkillDoc ? skillKeyForPath(open.path) : open.path;

  return (
    <section className="zorai-skill-pane">
      <header className="zorai-skill-pane__header">
        <div className="zorai-skill-pane__title">
          <div className="zorai-section-label">{open.draft ? "New skill" : isSkillDoc ? "Skill" : "File"}</div>
          <h2 title={`skills/${open.path}`}>{skillName ?? open.path.split("/").pop()}</h2>
          <code>skills/{open.path}</code>
        </div>
        <div className="zorai-skill-pane__actions">
          <div className="zorai-skill-pane__tabs" role="tablist">
            <button type="button" role="tab" aria-selected={mode === "preview"} onClick={() => useSkillsViewStore.getState().setMode("preview")}>Preview</button>
            <button
              type="button"
              role="tab"
              aria-selected={mode === "editor"}
              disabled={locked}
              title={locked ? "Unlock to edit" : undefined}
              onClick={() => useSkillsViewStore.getState().setMode("editor")}
            >
              Editor
            </button>
          </div>
          {open.draft ? null : <LibraryToggles path={open.path} isSkillDoc={isSkillDoc} />}
          {skillName && !open.draft ? (
            <div className="zorai-skill-pane__picker-anchor">
              <button type="button" className="zorai-ghost-button" aria-expanded={pickerOpen} onClick={() => setPickerOpen((value) => !value)}>
                Preferred{agentsPreferringSkill(library, skillName).length > 0 ? ` · ${agentsPreferringSkill(library, skillName).length}` : ""}
              </button>
              {pickerOpen ? <PreferredPicker skill={skillName} onClose={() => setPickerOpen(false)} /> : null}
            </div>
          ) : null}
          {open.draft ? null : (
            <button type="button" className="zorai-ghost-button" disabled={locked} onClick={() => setDialog("rename")}>Rename</button>
          )}
          {confirmDelete ? (
            <>
              <span className="zorai-skill-pane__confirm">{open.draft ? "Discard this draft?" : `Delete skills/${deleteTarget}?`}</span>
              <button
                type="button"
                className="zorai-ghost-button zorai-skill-danger"
                onClick={() => {
                  skipCloseSaveRef.current = true;
                  if (open.draft) {
                    useSkillsViewStore.getState().close();
                    return;
                  }
                  void getBridge()?.workspaceDeletePath?.(root, deleteTarget, { recursive: true }).then(() => {
                    useSkillsViewStore.getState().close();
                    afterTreeChange();
                  }, (failure) => {
                    skipCloseSaveRef.current = false;
                    setError(errorMessage(failure));
                  });
                }}
              >
                {open.draft ? "Discard" : "Delete"}
              </button>
              <button type="button" className="zorai-ghost-button" onClick={() => setConfirmDelete(false)}>Cancel</button>
            </>
          ) : (
            <button type="button" className="zorai-ghost-button" disabled={locked} onClick={() => setConfirmDelete(true)}>{open.draft ? "Discard" : "Delete"}</button>
          )}
          {mode === "editor" ? (
            <button type="button" className="zorai-ghost-button zorai-skill-save" disabled={!dirty} onClick={() => void save()}>Save</button>
          ) : null}
          <button type="button" className="zorai-icon-button" aria-label="Close" title="Close (unsaved edits are saved)" onClick={() => useSkillsViewStore.getState().close()}>×</button>
        </div>
      </header>
      {locked ? <p className="zorai-skill-pane__banner">Locked — read only, and protected from sync overwrites.</p> : null}
      {error ? <p role="alert" className="zorai-skill-pane__error">{error}</p> : null}
      <div className="zorai-skill-pane__body">
        {loading ? (
          <p className="zorai-empty-state">Loading…</p>
        ) : mode === "editor" ? (
          <WorkspaceCodeEditor
            value={content}
            path={`${root}/${open.path}`}
            language={language}
            settings={settings}
            textareaRef={textareaRef}
            onChange={(next) => {
              contentRef.current = next;
              setContent(next);
            }}
            onSelect={() => undefined}
            onSave={() => void save()}
          />
        ) : codePreviewKind(open.path) ? (
          <div className="zorai-skill-pane__preview">
            {content.trim() ? <CodeDocumentPreview path={open.path} content={content} /> : <p className="zorai-empty-state">Nothing to preview yet.</p>}
          </div>
        ) : (
          <MonacoReadOnlyView value={content} path={`${root}/${open.path}`} language={language} />
        )}
      </div>
      {dialog === "rename" ? (
        <NameDialog
          title="Rename"
          label="New path inside skills/"
          placeholder={open.path}
          initial={open.path}
          submitLabel="Rename"
          onClose={() => setDialog(null)}
          onSubmit={async (next) => {
            const target = next.trim().replace(/^\/+/, "");
            if (!target || target.split("/").includes("..")) throw new Error("Pick a path inside skills/.");
            if (isSkillPathCovered(library.locked, target)) throw new Error(`skills/${target} is locked.`);
            await save();
            await getBridge()?.workspaceRenamePath?.(root, open.path, target);
            skipCloseSaveRef.current = true;
            useSkillsViewStore.getState().openDocument(target, open.mode);
            afterTreeChange();
          }}
        />
      ) : null}
    </section>
  );
}

export function SkillsView() {
  const root = useSkillsRoot();
  const open = useSkillsViewStore((state) => state.open);
  const selectedDir = useSkillsViewStore((state) => state.selectedDir);
  if (!root) return <p className="zorai-empty-state">Loading skills…</p>;
  if (!open) return <FolderPane root={root} dir={selectedDir} />;
  return <DocumentPane key={open.path} root={root} open={open} />;
}
