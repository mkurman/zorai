import { lazy, Suspense, useMemo, useRef, useState } from "react";
import { MarkdownContent } from "@/components/agent-chat-panel/chat-view/markdown";
import { ViewModeToggle } from "@/components/data-viewers/ViewModeToggle";
import { useCodeEditorSettingsStore } from "@/zorai/features/code/codeEditorSettingsStore";

const WorkspaceCodeEditor = lazy(() => import("@/components/WorkspaceCodeEditor").then((module) => ({ default: module.WorkspaceCodeEditor })));

type PromptEditorMode = "edit" | "preview";

const PROMPT_EDITOR_MODES: ReadonlyArray<{ id: PromptEditorMode; label: string }> = [
  { id: "edit", label: "Edit" },
  { id: "preview", label: "Preview" },
];

export function PromptEditor({ value, onChange, path, label }: {
  value: string;
  onChange: (value: string) => void;
  path: string;
  label: string;
}) {
  const textareaRef = useRef<HTMLTextAreaElement | null>(null);
  const codeSettings = useCodeEditorSettingsStore((state) => state.settings);
  const settings = useMemo(() => ({ ...codeSettings, wordWrap: "viewport" as const, minimap: false, stickyScroll: false, glyphMargin: false }), [codeSettings]);
  const [mode, setMode] = useState<PromptEditorMode>("edit");
  return (
    <div className="zorai-prompt-field">
      <ViewModeToggle modes={PROMPT_EDITOR_MODES} value={mode} onChange={setMode} label={`${label} view`} />
      <div className="zorai-prompt-editor" aria-label={label}>
        {mode === "preview" ? (
          <div className="zorai-prompt-editor__preview">
            {value.trim() ? <MarkdownContent content={value} /> : <p className="zorai-empty-state">Nothing to preview yet.</p>}
          </div>
        ) : (
          <Suspense fallback={<textarea className="zorai-prompt-editor__fallback" aria-label={label} value={value} onChange={(event) => onChange(event.target.value)} />}>
            <WorkspaceCodeEditor
              value={value}
              path={path}
              language="markdown"
              settings={settings}
              textareaRef={textareaRef}
              onChange={onChange}
              onSelect={() => undefined}
              onSave={() => undefined}
            />
          </Suspense>
        )}
      </div>
    </div>
  );
}
