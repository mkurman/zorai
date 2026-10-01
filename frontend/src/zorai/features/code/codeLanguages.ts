const named: Record<string, string> = {
  dockerfile: "dockerfile", containerfile: "dockerfile", makefile: "shell", gnumakefile: "shell",
  rakefile: "ruby", gemfile: "ruby", guardfile: "ruby", procfile: "shell",
  ".bashrc": "shell", ".bash_profile": "shell", ".zshrc": "shell", ".profile": "shell",
  ".editorconfig": "ini", ".npmrc": "ini", ".prettierrc": "json", ".eslintrc": "json",
  "tsconfig.json": "json", "jsconfig.json": "json", "deno.json": "json", "deno.jsonc": "json",
};
const extensions: Record<string, string> = {
  c: "c", h: "c", cc: "cpp", cpp: "cpp", cxx: "cpp", hpp: "cpp", cs: "csharp", css: "css", scss: "scss", sass: "scss", less: "less",
  dart: "dart", ex: "elixir", exs: "elixir", fs: "fsharp", fsx: "fsharp", go: "go", graphql: "graphql", gql: "graphql", groovy: "java",
  html: "html", htm: "html", java: "java", js: "javascript", mjs: "javascript", cjs: "javascript", jsx: "javascript",
  json: "json", jsonc: "json", kt: "kotlin", kts: "kotlin", lua: "lua", md: "markdown", mdx: "mdx", mm: "objective-c",
  php: "php", pl: "perl", pm: "perl", proto: "proto", py: "python", r: "r", rb: "ruby", rs: "rust", sh: "shell", bash: "shell", zsh: "shell",
  sql: "sql", swift: "swift", svelte: "html", tf: "hcl", tfvars: "hcl", toml: "ini", ts: "typescript", mts: "typescript", cts: "typescript",
  tsx: "typescript", vue: "html", xml: "xml", yaml: "yaml", yml: "yaml", zig: "cpp",
};

export function monacoModelPath(scope: string, path: string): string {
  const normalized = path.trim().replace(/\\/g, "/").replace(/^\/+/, "");
  return `${scope}:///${normalized || "file"}`;
}

export function languageForWorkspacePath(path: string): string {
  const name = path.split(/[\\/]/).pop()?.toLowerCase() ?? "";
  if (named[name]) return named[name];
  if (/^(?:vite|vitest|eslint|prettier|postcss|tailwind)\.config\.(?:js|cjs|mjs)$/.test(name)) return "javascript";
  if (/^(?:vite|vitest|eslint|prettier|postcss|tailwind)\.config\.(?:ts|cts|mts)$/.test(name)) return "typescript";
  const extension = name.includes(".") ? name.split(".").pop() ?? "" : "";
  return extensions[extension] ?? "plaintext";
}
