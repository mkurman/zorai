import type { editor as MonacoEditorApi, IDisposable, languages } from "monaco-editor";
import { useSyncExternalStore } from "react";
import { getBridge } from "@/lib/bridge";

type MonacoApi = typeof import("monaco-editor");

const DEBOUNCE_MS = 300;
const CACHE_LIMIT = 50;
const PREFIX_SEND_CHARS = 8_000;
const SUFFIX_SEND_CHARS = 4_000;
const CACHE_PREFIX_CHARS = 300;
const CACHE_SUFFIX_CHARS = 120;

let enabled = true;
let registration: IDisposable | null = null;
let sequence = 0;
const cache = new Map<string, string>();
const listeners = new Set<() => void>();

export function setInlineCompletionsEnabled(value: boolean) {
  if (enabled === value) return;
  enabled = value;
  listeners.forEach((listener) => listener());
}

export function saveInlineCompletionsEnabled(value: boolean) {
  setInlineCompletionsEnabled(value);
  return getBridge()?.agentSetConfigItem?.("/code_completion/enabled", value);
}

export function useInlineCompletionsEnabled() {
  return useSyncExternalStore((listener) => {
    listeners.add(listener);
    return () => listeners.delete(listener);
  }, () => enabled, () => enabled);
}

export function inlineCompletionsEnabled() {
  return enabled;
}

export function completionCacheKey(path: string, prefix: string, suffix: string) {
  return `${path}\u0000${prefix.slice(-CACHE_PREFIX_CHARS)}\u0000${suffix.slice(0, CACHE_SUFFIX_CHARS)}`;
}

export function rememberCompletion(key: string, completion: string) {
  cache.delete(key);
  cache.set(key, completion);
  if (cache.size > CACHE_LIMIT) cache.delete(cache.keys().next().value as string);
}

export function cachedCompletion(key: string) {
  const value = cache.get(key);
  if (value === undefined) return undefined;
  rememberCompletion(key, value);
  return value;
}

export function shouldRequestCompletion(prefix: string, suffix: string): boolean {
  if (!prefix.trim()) return false;
  return !(/\w$/.test(prefix) && /^\w/.test(suffix));
}

function delay(ms: number, token: { isCancellationRequested: boolean }) {
  return new Promise<boolean>((resolve) => window.setTimeout(() => resolve(!token.isCancellationRequested), ms));
}

function modelPath(model: MonacoEditorApi.ITextModel) {
  return model.uri.path.replace(/^\/+/, "");
}

export function registerInlineCompletions(monaco: MonacoApi): IDisposable {
  if (registration) return registration;
  const provider: languages.InlineCompletionsProvider = {
    async provideInlineCompletions(model, position, _context, token) {
      const bridge = getBridge();
      if (!enabled || !bridge?.agentCodeComplete || model.uri.scheme === "inmemory") return { items: [] };
      const lastLine = model.getLineCount();
      const prefix = model.getValueInRange(new monaco.Range(1, 1, position.lineNumber, position.column));
      const suffix = model.getValueInRange(new monaco.Range(position.lineNumber, position.column, lastLine, model.getLineMaxColumn(lastLine)));
      if (!shouldRequestCompletion(prefix, suffix)) return { items: [] };
      const path = modelPath(model);
      const key = completionCacheKey(path, prefix, suffix);
      const range = new monaco.Range(position.lineNumber, position.column, position.lineNumber, position.column);
      const cached = cachedCompletion(key);
      if (cached !== undefined) return { items: cached ? [{ insertText: cached, range }] : [] };
      if (!(await delay(DEBOUNCE_MS, token))) return { items: [] };
      const requestId = `complete-${Date.now()}-${++sequence}`;
      const result = await bridge.agentCodeComplete({
        requestId,
        path,
        language: model.getLanguageId(),
        prefix: prefix.slice(-PREFIX_SEND_CHARS),
        suffix: suffix.slice(0, SUFFIX_SEND_CHARS),
      }).catch(() => null);
      if (!result || result.error) return { items: [] };
      rememberCompletion(key, result.completion);
      if (token.isCancellationRequested || !result.completion) return { items: [] };
      return { items: [{ insertText: result.completion, range }] };
    },
    freeInlineCompletions() {},
  };
  const disposable = monaco.languages.registerInlineCompletionsProvider({ pattern: "**" }, provider);
  registration = {
    dispose() {
      disposable.dispose();
      registration = null;
    },
  };
  return registration;
}
