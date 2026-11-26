import { describe, expect, it } from "vitest";
import { parseEditorconfig, parsePrettierConfig, resolvePrettierOptions } from "./codePrettierConfig";

describe("project Prettier config", () => {
  it("reads JSON and YAML .prettierrc so saved files match the project's style, not Prettier defaults", () => {
    expect(parsePrettierConfig('{ "singleQuote": true, "tabWidth": 4, "overrides": [], }')).toEqual({ singleQuote: true, tabWidth: 4 });
    expect(parsePrettierConfig("semi: false\nprintWidth: 100 # wide\ntrailingComma: 'all'")).toEqual({ semi: false, printWidth: 100, trailingComma: "all" });
    expect(parsePrettierConfig("not { json")).toBeNull();
  });

  it("applies only the .editorconfig sections that match the file", () => {
    const text = "root = true\n[*]\nindent_style = space\nindent_size = 2\n[*.{ts,tsx}]\nindent_size = 4\nend_of_line = crlf\n[*.md]\nindent_style = tab";
    expect(parseEditorconfig(text, "src/app.ts")).toEqual({ useTabs: false, tabWidth: 4, endOfLine: "crlf" });
    expect(parseEditorconfig(text, "README.md")).toEqual({ useTabs: true, tabWidth: 2 });
  });

  it("lets the nearest .prettierrc win over .editorconfig and outer configs", async () => {
    const files: Record<string, string> = {
      ".editorconfig": "[*]\nindent_size = 8",
      ".prettierrc": '{ "semi": false }',
      "packages/web/.prettierrc.json": '{ "singleQuote": true }',
      "packages/web/package.json": '{ "prettier": { "semi": true } }',
    };
    const read = async (path: string) => files[path] ?? null;
    expect(await resolvePrettierOptions("packages/web/src/index.ts", read)).toEqual({ tabWidth: 8, singleQuote: true });
    expect(await resolvePrettierOptions("tools/build.ts", read)).toEqual({ tabWidth: 8, semi: false });
  });
});
