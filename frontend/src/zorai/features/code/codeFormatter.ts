const parserMap: Record<string, string> = {
  javascript: "babel", javascriptreact: "babel", typescript: "typescript", typescriptreact: "typescript",
  json: "json", jsonc: "json", css: "css", scss: "scss", less: "less", html: "html",
  markdown: "markdown", yaml: "yaml",
};

export function prettierParserForLanguage(language: string): string | null {
  return parserMap[language.toLowerCase()] ?? null;
}

export async function formatCodeText(content: string, language: string, options: Record<string, string | number | boolean> = {}): Promise<string> {
  const parser = prettierParserForLanguage(language);
  if (!parser) throw new Error("No formatter available for this language.");
  const [{ format }, babel, estree, typescript, postcss, html, markdown, yaml] = await Promise.all([
    import("prettier/standalone"), import("prettier/plugins/babel"), import("prettier/plugins/estree"),
    import("prettier/plugins/typescript"), import("prettier/plugins/postcss"), import("prettier/plugins/html"),
    import("prettier/plugins/markdown"), import("prettier/plugins/yaml"),
  ]);
  return format(content, { ...options, parser, plugins: [babel.default, estree.default, typescript.default, postcss.default, html.default, markdown.default, yaml.default] });
}
