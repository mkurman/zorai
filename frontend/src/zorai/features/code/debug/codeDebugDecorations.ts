import type { editor as MonacoEditorApi, IDisposable } from "monaco-editor";
import { useDebugStore } from "./debugStore";

type MonacoApi = typeof import("monaco-editor");
type Target = { root: string; path: string } | null;

let hoverRegistered = false;
const INLINE_LOOKBACK = 60;

export function inlineValues(lines: string[], frameLine: number, variables: Array<{ name: string; value: string }>): Map<number, string> {
  const byLine = new Map<number, string[]>();
  for (const variable of variables) {
    if (!/^[A-Za-z_$][\w$]*$/.test(variable.name)) continue;
    const pattern = new RegExp(`(^|[^\\w$.])${variable.name.replace(/\$/g, "\\$")}(?![\\w$])`);
    for (let line = frameLine; line >= Math.max(1, frameLine - INLINE_LOOKBACK); line -= 1) {
      if (!pattern.test(lines[line - 1] ?? "")) continue;
      const value = variable.value.length > 40 ? `${variable.value.slice(0, 39)}…` : variable.value;
      byLine.set(line, [...(byLine.get(line) ?? []), `${variable.name} = ${value}`]);
      break;
    }
  }
  return new Map([...byLine].map(([line, entries]) => [line, entries.join(", ")]));
}

function registerDebugHover(monaco: MonacoApi) {
  if (hoverRegistered) return;
  hoverRegistered = true;
  monaco.languages.registerHoverProvider("*", {
    provideHover: async (model, position) => {
      if (useDebugStore.getState().status !== "stopped") return null;
      const word = model.getWordAtPosition(position);
      if (!word) return null;
      const line = model.getLineContent(position.lineNumber);
      const prefix = /[\w$.]*$/.exec(line.slice(0, word.startColumn - 1))?.[0] ?? "";
      const expression = `${prefix}${word.word}`;
      const value = await useDebugStore.getState().evaluate(expression, "hover");
      if (value === null || value === "") return null;
      return {
        range: new monaco.Range(position.lineNumber, word.startColumn - prefix.length, position.lineNumber, word.endColumn),
        contents: [{ value: `\`${expression}\` = \`${value.replace(/`/g, "'").slice(0, 2000)}\`` }],
      };
    },
  });
}

export function attachDebugDecorations(editor: MonacoEditorApi.IStandaloneCodeEditor, monaco: MonacoApi, getTarget: () => Target): IDisposable {
  registerDebugHover(monaco);
  const decorations = editor.createDecorationsCollection([]);
  const render = () => {
    const target = getTarget();
    const model = editor.getModel();
    if (!target || !model) {
      decorations.clear();
      return;
    }
    const state = useDebugStore.getState();
    const breakpoints = state.breakpoints[target.root]?.[target.path] ?? [];
    const frame = state.status === "stopped" ? state.frames.find((entry) => entry.id === state.frameId) : undefined;
    const lineCount = model.getLineCount();
    decorations.set([
      ...breakpoints.filter((breakpoint) => breakpoint.line <= lineCount).map((breakpoint) => ({
        range: new monaco.Range(breakpoint.line, 1, breakpoint.line, 1),
        options: {
          glyphMarginClassName: `zorai-breakpoint${breakpoint.enabled ? "" : " is-disabled"}${breakpoint.condition || breakpoint.hitCondition ? " is-conditional" : ""}${breakpoint.logMessage ? " is-log" : ""}`,
          glyphMarginHoverMessage: { value: breakpoint.condition ? `Breakpoint · when \`${breakpoint.condition}\`` : "Breakpoint" },
          stickiness: monaco.editor.TrackedRangeStickiness.NeverGrowsWhenTypingAtEdges,
        },
      })),
      ...(frame && frame.path === target.path ? [...inlineValues(model.getLinesContent(), Math.min(frame.line, lineCount), state.scopes[0] ? state.variables[state.scopes[0].variablesReference] ?? [] : [])].map(([line, text]) => ({
        range: new monaco.Range(line, model.getLineMaxColumn(line), line, model.getLineMaxColumn(line)),
        options: { after: { content: `  ${text}`, inlineClassName: "zorai-debug-inline-value" } },
      })) : []),
      ...(frame && frame.path === target.path && frame.line >= 1 && frame.line <= lineCount ? [{
        range: new monaco.Range(frame.line, 1, frame.line, 1),
        options: { isWholeLine: true, className: "zorai-debug-current-line", glyphMarginClassName: "zorai-debug-current-glyph" },
      }] : []),
    ]);
  };
  const unsubscribe = useDebugStore.subscribe((state, previous) => {
    if (state.breakpoints !== previous.breakpoints || state.frameId !== previous.frameId || state.frames !== previous.frames || state.status !== previous.status || state.variables !== previous.variables) render();
  });
  const listeners = [
    editor.onDidChangeModel(render),
    editor.onMouseDown((event) => {
      if (event.target.type !== monaco.editor.MouseTargetType.GUTTER_GLYPH_MARGIN) return;
      const target = getTarget();
      const line = event.target.position?.lineNumber;
      if (target && line) useDebugStore.getState().toggleBreakpoint(target.root, target.path, line);
    }),
    editor.addAction({
      id: "debug.toggleBreakpoint",
      label: "Toggle Breakpoint",
      keybindings: [monaco.KeyCode.F9],
      run: () => {
        const target = getTarget();
        const line = editor.getPosition()?.lineNumber;
        if (target && line) useDebugStore.getState().toggleBreakpoint(target.root, target.path, line);
      },
    }),
  ];
  render();
  return {
    dispose() {
      unsubscribe();
      listeners.forEach((listener) => listener.dispose());
      decorations.clear();
    },
  };
}
