import type { editor as MonacoEditorApi, IDisposable, IKeyboardEvent } from "monaco-editor";
import { create } from "zustand";
import { applyMotion, operatorRange, parseVimCommand, substituteLine, type VimCommand, type VimPos, type VimRange } from "./codeVimMotions";

type MonacoApi = typeof import("monaco-editor");
export type KeymapMode = "default" | "vim" | "emacs";
type VimState = "normal" | "insert" | "visual" | "visual-line";

export const useKeymapStatusStore = create<{ label: string | null; set: (label: string | null) => void }>((set) => ({ label: null, set: (label) => set({ label }) }));

export function attachKeymapMode(
  editor: MonacoEditorApi.IStandaloneCodeEditor,
  monaco: MonacoApi,
  getMode: () => KeymapMode,
  callbacks: () => { onSave?: () => void; onClose?: () => void },
): IDisposable & { refresh: () => void } {
  let state: VimState = "normal";
  let buffer = "";
  let register: { text: string; linewise: boolean } = { text: "", linewise: false };
  let anchor: VimPos | null = null;
  let lastSearch: { pattern: string; backward: boolean } | null = null;
  let lastChange = "";
  let emacsPrefix = false;
  let emacsMark = false;
  let killRing = "";
  const status = useKeymapStatusStore.getState().set;

  const model = () => editor.getModel();
  const lines = () => model()?.getLinesContent() ?? [""];
  const pos = (): VimPos => {
    const position = editor.getPosition();
    return { line: (position?.lineNumber ?? 1) - 1, col: (position?.column ?? 1) - 1 };
  };
  const setPos = (target: VimPos) => {
    editor.setPosition({ lineNumber: target.line + 1, column: target.col + 1 });
    editor.revealPositionInCenterIfOutsideViewport({ lineNumber: target.line + 1, column: target.col + 1 });
  };
  const updateStatus = () => {
    const mode = getMode();
    if (mode === "vim") status(`-- ${state === "visual-line" ? "VISUAL LINE" : state.toUpperCase()} --${buffer ? `  ${buffer}` : ""}`);
    else if (mode === "emacs") status(emacsPrefix ? "Emacs · C-x-" : emacsMark ? "Emacs · Mark set" : "Emacs");
    else status(null);
    editor.updateOptions({ cursorStyle: mode === "vim" && state !== "insert" ? "block" : "line" });
  };
  const setState = (next: VimState) => {
    state = next;
    if (next === "normal" || next === "insert") anchor = null;
    updateStatus();
  };
  const toMonaco = (range: VimRange) => {
    const current = model();
    if (!current) return null;
    if (!range.linewise) return new monaco.Range(range.start.line + 1, range.start.col + 1, range.end.line + 1, range.end.col + 1);
    const count = current.getLineCount();
    if (range.end.line + 1 < count) return new monaco.Range(range.start.line + 1, 1, range.end.line + 2, 1);
    if (range.start.line > 0) return new monaco.Range(range.start.line, current.getLineMaxColumn(range.start.line), range.end.line + 1, current.getLineMaxColumn(range.end.line + 1));
    return new monaco.Range(1, 1, range.end.line + 1, current.getLineMaxColumn(range.end.line + 1));
  };
  const edit = (range: InstanceType<MonacoApi["Range"]>, text: string) => {
    editor.pushUndoStop();
    editor.executeEdits("zorai.keymap", [{ range, text, forceMoveMarkers: true }]);
    editor.pushUndoStop();
  };
  const rangeText = (range: VimRange) => {
    const current = model();
    const monacoRange = toMonaco(range);
    if (!current || !monacoRange) return "";
    if (!range.linewise) return current.getValueInRange(monacoRange);
    return lines().slice(range.start.line, range.end.line + 1).join("\n") + "\n";
  };
  const applyOperator = (operator: "d" | "c" | "y" | ">" | "<", range: VimRange) => {
    const monacoRange = toMonaco(range);
    if (!monacoRange) return;
    if (operator === ">" || operator === "<") {
      editor.setSelection(new monaco.Range(range.start.line + 1, 1, range.end.line + 1, 1));
      editor.trigger("zorai.keymap", operator === ">" ? "editor.action.indentLines" : "editor.action.outdentLines", null);
      setPos({ line: range.start.line, col: 0 });
      return;
    }
    register = { text: rangeText(range), linewise: range.linewise };
    void navigator.clipboard?.writeText(register.text).catch(() => undefined);
    if (operator === "y") {
      setPos(range.start);
      return;
    }
    if (operator === "c" && range.linewise) {
      const indent = /^\s*/.exec(lines()[range.start.line])?.[0] ?? "";
      edit(new monaco.Range(range.start.line + 1, 1, range.end.line + 1, model()!.getLineMaxColumn(range.end.line + 1)), indent);
      setPos({ line: range.start.line, col: indent.length });
      setState("insert");
      return;
    }
    edit(monacoRange, "");
    setPos(range.linewise ? { line: Math.min(range.start.line, lines().length - 1), col: Math.max(0, lines()[Math.min(range.start.line, lines().length - 1)].search(/\S|$/)) } : range.start);
    if (operator === "c") setState("insert");
  };
  const paste = (before: boolean, count: number) => {
    const text = register.text.repeat(count);
    const at = pos();
    if (register.linewise) {
      const line = before ? at.line : at.line + 1;
      const current = model()!;
      if (line < current.getLineCount()) edit(new monaco.Range(line + 1, 1, line + 1, 1), text);
      else edit(new monaco.Range(line, current.getLineMaxColumn(line), line, current.getLineMaxColumn(line)), `\n${text.replace(/\n$/, "")}`);
      setPos({ line, col: Math.max(0, lines()[line]?.search(/\S|$/) ?? 0) });
    } else {
      const col = before ? at.col : Math.min(at.col + 1, lines()[at.line].length);
      edit(new monaco.Range(at.line + 1, col + 1, at.line + 1, col + 1), text);
      setPos({ line: at.line, col: col + Math.max(0, text.length - 1) });
    }
  };
  const search = (direction: 1 | -1) => {
    const current = model();
    if (!current || !lastSearch) return;
    const backward = lastSearch.backward ? direction === 1 : direction === -1;
    const position = editor.getPosition()!;
    const from = backward ? position : new monaco.Position(position.lineNumber, position.column + 1);
    const match = backward
      ? current.findPreviousMatch(lastSearch.pattern, from, true, false, null, false)
      : current.findNextMatch(lastSearch.pattern, from, true, false, null, false);
    if (match) setPos({ line: match.range.startLineNumber - 1, col: match.range.startColumn - 1 });
  };
  const runAction = (command: Extract<VimCommand, { kind: "action" }>) => {
    const at = pos();
    const lineText = lines()[at.line];
    const { count, action } = command;
    if (action === "i") setState("insert");
    else if (action === "a") { setPos({ line: at.line, col: Math.min(at.col + 1, lineText.length) }); setState("insert"); }
    else if (action === "I") { setPos({ line: at.line, col: Math.max(0, lineText.search(/\S|$/)) }); setState("insert"); }
    else if (action === "A") { setPos({ line: at.line, col: lineText.length }); setState("insert"); }
    else if (action === "o" || action === "O") {
      editor.setPosition({ lineNumber: at.line + 1, column: action === "o" ? lineText.length + 1 : 1 });
      editor.trigger("zorai.keymap", action === "o" ? "editor.action.insertLineAfter" : "editor.action.insertLineBefore", null);
      setState("insert");
    } else if (action === "v" || action === "V") {
      anchor = at;
      setState(action === "v" ? "visual" : "visual-line");
      editor.setSelection(action === "V" ? new monaco.Range(at.line + 1, 1, at.line + 1, lineText.length + 1) : new monaco.Range(at.line + 1, at.col + 1, at.line + 1, at.col + 2));
    } else if (action === "x" || action === "X") {
      if (lineText.length === 0) return;
      const start = action === "x" ? at.col : Math.max(0, at.col - count);
      const end = action === "x" ? Math.min(lineText.length, at.col + count) : at.col;
      register = { text: lineText.slice(start, end), linewise: false };
      edit(new monaco.Range(at.line + 1, start + 1, at.line + 1, end + 1), "");
      setPos({ line: at.line, col: Math.min(start, Math.max(0, lineText.length - (end - start) - 1)) });
    } else if (action === "p" || action === "P") paste(action === "P", count);
    else if (action === "u") for (let step = 0; step < count; step += 1) editor.trigger("zorai.keymap", "undo", null);
    else if (action === "J") {
      const last = Math.min(lines().length - 1, at.line + Math.max(1, count - 1));
      editor.setSelection(new monaco.Range(at.line + 1, 1, last + 1, 1));
      editor.trigger("zorai.keymap", "editor.action.joinLines", null);
    } else if (action.startsWith("r") && action.length === 2) {
      if (at.col + count <= lineText.length) edit(new monaco.Range(at.line + 1, at.col + 1, at.line + 1, at.col + count + 1), action[1].repeat(count));
      setPos(at);
    } else if (action === "~") {
      const end = Math.min(lineText.length, at.col + count);
      const toggled = [...lineText.slice(at.col, end)].map((char) => (char === char.toUpperCase() ? char.toLowerCase() : char.toUpperCase())).join("");
      edit(new monaco.Range(at.line + 1, at.col + 1, at.line + 1, end + 1), toggled);
      setPos({ line: at.line, col: Math.min(end, Math.max(0, lineText.length - 1)) });
    } else if (action === "n" || action === "N") search(action === "n" ? 1 : -1);
    else if (action === "s") execute(`${count}cl`);
    else if (action === "S") execute(`${count}cc`);
    else if (action === "C") execute("c$");
    else if (action === "D") execute("d$");
    else if (action === "Y") execute(`${count}yy`);
    else if (action === "zz") editor.revealLineInCenter(at.line + 1);
    else if (action === "." && lastChange) execute(lastChange, false);
  };
  const visualRange = (): VimRange | null => {
    if (!anchor) return null;
    const at = pos();
    const [from, to] = anchor.line < at.line || (anchor.line === at.line && anchor.col <= at.col) ? [anchor, at] : [at, anchor];
    return state === "visual-line"
      ? { start: { line: from.line, col: 0 }, end: { line: to.line, col: lines()[to.line].length }, linewise: true }
      : { start: from, end: { line: to.line, col: to.col + 1 }, linewise: false };
  };
  const moveTo = (command: Extract<VimCommand, { kind: "motion" }>) => {
    const at = pos();
    let target = applyMotion(lines(), at, command.motion, command.count);
    if (command.motion === "j" || command.motion === "k") target = { line: target.line, col: Math.min(at.col, Math.max(0, lines()[target.line].length - 1)) };
    if ((state === "visual" || state === "visual-line") && anchor) {
      const lineCount = lines();
      const [from, to] = anchor.line < target.line || (anchor.line === target.line && anchor.col <= target.col) ? [anchor, target] : [target, anchor];
      editor.setSelection(state === "visual-line"
        ? new monaco.Range(from.line + 1, 1, to.line + 1, lineCount[to.line].length + 1)
        : new monaco.Range(from.line + 1, from.col + 1, to.line + 1, to.col + 2));
      editor.setPosition({ lineNumber: target.line + 1, column: target.col + 1 });
      return;
    }
    setPos(target);
  };
  const execute = (input: string, remember = true): boolean => {
    if (state === "visual" || state === "visual-line") {
      const range = visualRange();
      const operator = input.replace(/^\d+/, "")[0];
      if (range && ["d", "c", "y", ">", "<", "x"].includes(operator)) {
        buffer = "";
        setState("normal");
        applyOperator(operator === "x" ? "d" : operator as "d" | "c" | "y" | ">" | "<", range);
        return true;
      }
    }
    const command = parseVimCommand(input);
    if (command === "pending") return false;
    buffer = "";
    if (command === "invalid") return true;
    if (command.kind === "motion") moveTo(command);
    else if (command.kind === "operator") {
      const range = operatorRange(lines(), pos(), command);
      if (range) applyOperator(command.operator, range);
      if (remember && command.operator !== "y") lastChange = input;
    } else {
      runAction(command);
      if (remember && ["x", "X", "p", "P", "J", "~", "s", "S", "C", "D"].includes(command.action)) lastChange = input;
    }
    return true;
  };
  const runCommandLine = (text: string) => {
    const trimmed = text.trim();
    if (trimmed.startsWith("/") || trimmed.startsWith("?")) {
      lastSearch = { pattern: trimmed.slice(1), backward: trimmed.startsWith("?") };
      search(1);
      return;
    }
    const command = trimmed.slice(1).trim();
    if (/^\d+$/.test(command)) setPos({ line: Math.min(lines().length - 1, Math.max(0, Number(command) - 1)), col: 0 });
    else if (command === "w") callbacks().onSave?.();
    else if (command === "q" || command === "q!") callbacks().onClose?.();
    else if (command === "wq" || command === "x") { callbacks().onSave?.(); callbacks().onClose?.(); }
    else if (command.startsWith("%s/") || command.startsWith("s/")) {
      const all = command.startsWith("%");
      const current = model()!;
      const first = all ? 0 : pos().line;
      const last = all ? current.getLineCount() - 1 : pos().line;
      const replaced = lines().slice(first, last + 1).map((line) => substituteLine(line, all ? command.slice(1) : command) ?? line);
      edit(new monaco.Range(first + 1, 1, last + 1, current.getLineMaxColumn(last + 1)), replaced.join("\n"));
    }
  };
  const commandWidget = (() => {
    const node = document.createElement("div");
    node.className = "zorai-vim-command";
    const input = document.createElement("input");
    input.spellcheck = false;
    node.append(input);
    node.style.display = "none";
    const widget = { getId: () => `zorai.vim.command.${editor.getId()}`, getDomNode: () => node, getPosition: () => ({ preference: monaco.editor.OverlayWidgetPositionPreference.BOTTOM_RIGHT_CORNER }) };
    editor.addOverlayWidget(widget);
    const close = () => { node.style.display = "none"; editor.focus(); };
    input.addEventListener("keydown", (event) => {
      event.stopPropagation();
      if (event.key === "Escape") close();
      else if (event.key === "Enter") { const value = input.value; close(); runCommandLine(value); }
    });
    return { open: (prefix: string) => { node.style.display = "block"; input.value = prefix; input.focus(); }, dispose: () => editor.removeOverlayWidget(widget) };
  })();

  const handleEmacs = (event: IKeyboardEvent) => {
    const key = event.browserEvent.key.toLowerCase();
    const ctrl = event.ctrlKey && !event.altKey;
    const alt = event.altKey && !event.ctrlKey;
    const run = (id: string) => editor.trigger("zorai.keymap", id, null);
    const select = emacsMark;
    let handled = true;
    if (emacsPrefix) {
      emacsPrefix = false;
      if (ctrl && key === "s") callbacks().onSave?.();
      else if (ctrl && key === "c") callbacks().onClose?.();
      else handled = key !== "control";
      if (key === "control") emacsPrefix = true;
    } else if (ctrl && key === "x") emacsPrefix = true;
    else if (ctrl && key === "a") run(select ? "cursorHomeSelect" : "cursorHome");
    else if (ctrl && key === "e") run(select ? "cursorEndSelect" : "cursorEnd");
    else if (ctrl && key === "f") run(select ? "cursorRightSelect" : "cursorRight");
    else if (ctrl && key === "b") run(select ? "cursorLeftSelect" : "cursorLeft");
    else if (ctrl && key === "n") run(select ? "cursorDownSelect" : "cursorDown");
    else if (ctrl && key === "p") run(select ? "cursorUpSelect" : "cursorUp");
    else if (alt && key === "f") run(select ? "cursorWordEndRightSelect" : "cursorWordEndRight");
    else if (alt && key === "b") run(select ? "cursorWordStartLeftSelect" : "cursorWordStartLeft");
    else if (ctrl && key === "d") run("deleteRight");
    else if (ctrl && key === "k") {
      const at = pos();
      const lineText = lines()[at.line];
      const range = at.col >= lineText.length ? new monaco.Range(at.line + 1, at.col + 1, at.line + 2, 1) : new monaco.Range(at.line + 1, at.col + 1, at.line + 1, lineText.length + 1);
      killRing = model()!.getValueInRange(range);
      edit(range, "");
    } else if (ctrl && key === "y") {
      const at = editor.getPosition()!;
      edit(new monaco.Range(at.lineNumber, at.column, at.lineNumber, at.column), killRing);
    } else if (ctrl && (key === " " || key === "@")) emacsMark = !emacsMark;
    else if ((ctrl && key === "w") || (alt && key === "w")) {
      const selection = editor.getSelection();
      if (selection && !selection.isEmpty()) {
        killRing = model()!.getValueInRange(selection);
        if (ctrl) edit(selection, "");
      }
      emacsMark = false;
    } else if (ctrl && key === "g") {
      emacsMark = false;
      const at = editor.getPosition()!;
      editor.setSelection(new monaco.Range(at.lineNumber, at.column, at.lineNumber, at.column));
    } else if (ctrl && key === "/") run("undo");
    else if (ctrl && key === "s") run("actions.find");
    else handled = false;
    if (!handled) return;
    event.preventDefault();
    event.stopPropagation();
    updateStatus();
  };

  const listener = editor.onKeyDown((event) => {
    const mode = getMode();
    if (mode === "emacs") return handleEmacs(event);
    if (mode !== "vim") return;
    const key = event.browserEvent.key;
    if (state === "insert") {
      if (key === "Escape" || (event.ctrlKey && key === "[")) {
        event.preventDefault();
        event.stopPropagation();
        setState("normal");
        const at = pos();
        setPos({ line: at.line, col: Math.max(0, at.col - 1) });
      }
      return;
    }
    if (["Shift", "Control", "Alt", "Meta"].includes(key)) return;
    if (event.ctrlKey || event.metaKey) {
      if (event.ctrlKey && key === "r") { editor.trigger("zorai.keymap", "redo", null); event.preventDefault(); event.stopPropagation(); }
      else if (event.ctrlKey && (key === "d" || key === "u")) { editor.trigger("zorai.keymap", key === "d" ? "scrollPageDown" : "scrollPageUp", null); event.preventDefault(); event.stopPropagation(); }
      return;
    }
    if (key.length > 1 && !["Escape", "Backspace", "Enter"].includes(key)) return;
    event.preventDefault();
    event.stopPropagation();
    if (key === "Escape") {
      buffer = "";
      if (state !== "normal") {
        const at = editor.getPosition()!;
        editor.setSelection(new monaco.Range(at.lineNumber, at.column, at.lineNumber, at.column));
        setState("normal");
      }
      updateStatus();
      return;
    }
    if (buffer === "" && (key === ":" || key === "/" || key === "?")) {
      commandWidget.open(key);
      return;
    }
    const translated = key === "Backspace" ? "h" : key === "Enter" ? "j" : key;
    if (buffer === "" && translated === "0") {
      execute("0");
    } else {
      buffer += translated;
      execute(buffer);
    }
    updateStatus();
  });

  const refresh = () => {
    if (getMode() !== "vim") state = "normal";
    buffer = "";
    emacsPrefix = false;
    emacsMark = false;
    updateStatus();
  };
  const focus = editor.onDidFocusEditorText(updateStatus);
  refresh();
  return {
    refresh,
    dispose() {
      listener.dispose();
      focus.dispose();
      commandWidget.dispose();
      status(null);
    },
  };
}
