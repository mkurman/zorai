export type CodeEditorPreloadState = "idle" | "loading" | "ready" | "failed";

export function createCodeEditorPreloader(load: () => Promise<void>) {
  let current: Promise<void> | null = null;
  let status: CodeEditorPreloadState = "idle";
  return {
    preload() {
      if (status === "ready") return current ?? Promise.resolve();
      if (status === "loading" && current) return current;
      status = "loading";
      current = load().then(() => {
        status = "ready";
      }).catch((error) => {
        status = "failed";
        current = null;
        throw error;
      });
      return current;
    },
    state: () => status,
  };
}

const editorPreloader = createCodeEditorPreloader(async () => {
  const [, , , environment] = await Promise.all([
    import("@/components/WorkspaceCodeEditor"),
    import("@monaco-editor/react"),
    import("monaco-editor/esm/vs/editor/editor.api"),
    import("@/lib/monacoEnvironment"),
  ]);
  environment.warmMonacoLanguages();
});

export function preloadCodeEditor(): Promise<void> {
  return editorPreloader.preload();
}

export function codeEditorPreloadState(): CodeEditorPreloadState {
  return editorPreloader.state();
}
