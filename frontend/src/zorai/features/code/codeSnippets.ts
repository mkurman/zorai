import { getBridge } from "@/lib/bridge";

type MonacoApi = typeof import("monaco-editor");
export type CodeSnippet = { name: string; prefix: string; body: string; description: string; scope: string[]; source: string };

const LANGUAGE_ALIASES: Record<string, string[]> = {
  typescriptreact: ["typescript"],
  javascriptreact: ["javascript"],
  shellscript: ["shell"],
};

let snippets: CodeSnippet[] = [];
let loadedRoot: string | null | undefined;
let loadedAt = 0;
let registered = false;

export function snippetsForLanguage(all: CodeSnippet[], language: string): CodeSnippet[] {
  const names = new Set([language, ...(LANGUAGE_ALIASES[language] ?? []), ...Object.entries(LANGUAGE_ALIASES).filter(([, aliases]) => aliases.includes(language)).map(([name]) => name)]);
  return all.filter((snippet) => snippet.scope.length === 0 || snippet.scope.some((scope) => names.has(scope)));
}

export async function loadWorkspaceSnippets(root: string | null) {
  if (loadedRoot === root && Date.now() - loadedAt < 10_000) return;
  loadedRoot = root;
  loadedAt = Date.now();
  snippets = (await getBridge()?.snippetsLoad?.(root).catch(() => [])) ?? [];
}

export function registerSnippetCompletions(monaco: MonacoApi) {
  if (registered) return;
  registered = true;
  monaco.languages.registerCompletionItemProvider("*", {
    provideCompletionItems: (model, position) => {
      const word = model.getWordUntilPosition(position);
      const range = new monaco.Range(position.lineNumber, word.startColumn, position.lineNumber, word.endColumn);
      return {
        suggestions: snippetsForLanguage(snippets, model.getLanguageId()).map((snippet) => ({
          label: { label: snippet.prefix, description: snippet.name },
          kind: monaco.languages.CompletionItemKind.Snippet,
          detail: `${snippet.description} (${snippet.source})`,
          documentation: { value: `\`\`\`\n${snippet.body}\n\`\`\`` },
          insertText: snippet.body,
          insertTextRules: monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet,
          range,
          sortText: `~${snippet.prefix}`,
        })),
      };
    },
  });
}
