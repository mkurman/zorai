import { lazy, Suspense, useMemo, useRef } from "react";
import { useCodeEditorSettingsStore } from "@/zorai/features/code/codeEditorSettingsStore";

const WorkspaceCodeEditor = lazy(() => import("@/components/WorkspaceCodeEditor").then((module) => ({ default: module.WorkspaceCodeEditor })));

export function PromptEditor({ value, onChange, path, label }: {
  value: string;
  onChange: (value: string) => void;
  path: string;
  label: string;
}) {
  const textareaRef = useRef<HTMLTextAreaElement | null>(null);
  const codeSettings = useCodeEditorSettingsStore((state) => state.settings);
  const settings = useMemo(() => ({ ...codeSettings, wordWrap: "viewport" as const, minimap: false, stickyScroll: false, glyphMargin: false }), [codeSettings]);
  return (
    <div className="zorai-prompt-editor" aria-label={label}>
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
    </div>
  );
}
