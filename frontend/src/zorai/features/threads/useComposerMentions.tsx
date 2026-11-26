import { useEffect, useMemo, useState, type KeyboardEvent, type RefObject } from "react";
import { getBridge } from "@/lib/bridge";
import type { ComposerAttachment } from "@/components/agent-chat-panel/chat-view/types";
import { getWorkspaceFiles } from "@/zorai/features/code/workspaceFileIndex";
import { languageForWorkspacePath } from "@/zorai/features/code/codeLanguages";
import { flattenLspSymbols } from "@/zorai/features/code/codeQuickOpenProviders";
import { ComposerMentionMenu } from "./ComposerMentionMenu";
import { mentionOptions, mentionQueryAt, removeMentionQuery, type MentionCommit, type MentionOption, type MentionQuery, type MentionSymbol } from "./composerMentions";
import { resolveMentionContent } from "./composerMentionContext";

export function useComposerMentions({ input, setInput, inputRef, root, anchorFile, attachedFiles, attachFile, addAttachment, onError, terminalSessionKey }: {
  input: string;
  setInput: (value: string) => void;
  inputRef: RefObject<HTMLTextAreaElement | null>;
  root: string | null;
  anchorFile: string | null;
  attachedFiles: string[];
  attachFile: (path: string) => void;
  addAttachment: (attachment: ComposerAttachment) => void;
  onError: (message: string) => void;
  terminalSessionKey: string | null;
}) {
  const [mention, setMention] = useState<MentionQuery | null>(null);
  const [activeIndex, setActiveIndex] = useState(0);
  const [files, setFiles] = useState<{ root: string; paths: string[] } | null>(null);
  const [commits, setCommits] = useState<{ root: string; list: MentionCommit[] } | null>(null);
  const [symbols, setSymbols] = useState<MentionSymbol[]>([]);
  const mode = mention?.query.includes(":") ? mention.query.slice(0, mention.query.indexOf(":")).toLowerCase() : null;
  const symbolQuery = mode === "symbol" ? mention?.query.slice("symbol:".length) ?? "" : "";

  useEffect(() => {
    const bridge = getBridge();
    if (mode !== "commit" || !root || commits?.root === root || !bridge?.workspaceGitHistory) return;
    let cancelled = false;
    void bridge.workspaceGitHistory(root, { limit: 100 }).then((list) => { if (!cancelled) setCommits({ root, list }); }).catch(() => undefined);
    return () => { cancelled = true; };
  }, [commits?.root, mode, root]);

  useEffect(() => {
    const bridge = getBridge();
    if (mode !== "symbol" || !root || !anchorFile || symbolQuery.length < 2 || !bridge?.workspaceLspRequest) {
      setSymbols([]);
      return;
    }
    let cancelled = false;
    const timer = window.setTimeout(() => {
      void bridge.workspaceLspRequest!(root, anchorFile, languageForWorkspacePath(anchorFile), "workspaceSymbol", { line: 0, character: 0 }, { query: symbolQuery })
        .then((response) => {
          if (cancelled) return;
          setSymbols(response?.result ? flattenLspSymbols(response.result, anchorFile, root).map((symbol) => ({ name: symbol.name, kind: symbol.kind, path: symbol.path, line: symbol.line })) : []);
        })
        .catch(() => { if (!cancelled) setSymbols([]); });
    }, 250);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [anchorFile, mode, root, symbolQuery]);

  useEffect(() => {
    const bridge = getBridge();
    if (!mention || !root || files?.root === root || !bridge?.workspaceListDirectory || (mode && mode !== "folder")) return;
    let cancelled = false;
    void getWorkspaceFiles(bridge, root).then((paths) => { if (!cancelled) setFiles({ root, paths }); }).catch(() => undefined);
    return () => { cancelled = true; };
  }, [files?.root, mention, mode, root]);

  const indexedFiles = useMemo(() => (files?.root === root ? files.paths : []), [files, root]);
  const options = useMemo(() => (mention ? mentionOptions(mention.query, indexedFiles, { commits: commits?.root === root ? commits.list : [], symbols }) : []), [commits, indexedFiles, mention, root, symbols]);

  const onInputChange = (value: string, caret: number) => {
    const next = mentionQueryAt(value, caret);
    setMention(next);
    if (next?.query !== mention?.query) setActiveIndex(0);
  };

  const pick = (option: MentionOption) => {
    if (!mention) return;
    if (option.kind === "prefix") {
      const typed = `@${option.id}`;
      const next = `${input.slice(0, mention.start)}${typed}${input.slice(mention.end)}`;
      const caret = mention.start + typed.length;
      setInput(next);
      setMention({ query: option.id, start: mention.start, end: caret });
      setActiveIndex(0);
      requestAnimationFrame(() => {
        inputRef.current?.focus();
        inputRef.current?.setSelectionRange(caret, caret);
      });
      return;
    }
    const { text, caret } = removeMentionQuery(input, mention);
    setInput(text);
    setMention(null);
    requestAnimationFrame(() => {
      inputRef.current?.focus();
      inputRef.current?.setSelectionRange(caret, caret);
    });
    if (option.kind === "file") {
      if (!attachedFiles.includes(option.id)) attachFile(option.id);
      return;
    }
    void resolveMentionContent(option, { root, terminalSessionKey, files: indexedFiles }).then((content) => addAttachment({
      id: `mention-${option.kind}-${Date.now()}`,
      name: option.label,
      size: content.length,
      kind: "text",
      mimeType: "text/plain",
      textContent: content,
    })).catch((error: unknown) => onError(error instanceof Error ? error.message : String(error)));
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>): boolean => {
    if (!mention) return false;
    if (event.key === "Escape") {
      event.preventDefault();
      setMention(null);
      return true;
    }
    if (options.length === 0) return false;
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const step = event.key === "ArrowDown" ? 1 : -1;
      setActiveIndex((current) => (current + step + options.length) % options.length);
      return true;
    }
    if ((event.key === "Enter" && !event.shiftKey) || event.key === "Tab") {
      event.preventDefault();
      pick(options[Math.min(activeIndex, options.length - 1)]);
      return true;
    }
    return false;
  };

  const menu = mention ? <ComposerMentionMenu options={options} activeIndex={activeIndex} onPick={pick} onHover={setActiveIndex} /> : null;
  return { onInputChange, handleKeyDown, menu, close: () => setMention(null) };
}
