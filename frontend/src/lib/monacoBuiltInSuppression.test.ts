import { describe, expect, it } from "vitest";
import { installBuiltInLanguageSuppression, suppressBuiltInLanguageService } from "./monacoBuiltInSuppression";

class HoverAdapter {
  provideHover() { return { contents: [{ value: "type X = 1" }] }; }
}

function fakeMonaco() {
  const registered: Array<{ selector: string; provider: { provideHover: () => unknown } }> = [];
  let tsDiagnostics: Record<string, unknown> = { noSemanticValidation: false };
  const monaco = {
    languages: {
      registerHoverProvider: (selector: string, provider: { provideHover: () => unknown }) => {
        registered.push({ selector, provider });
        return { dispose() {} };
      },
      typescript: {
        typescriptDefaults: { getDiagnosticsOptions: () => tsDiagnostics, setDiagnosticsOptions: (options: object) => { tsDiagnostics = { ...options }; } },
        javascriptDefaults: { getDiagnosticsOptions: () => ({}), setDiagnosticsOptions: () => {} },
      },
    } as Record<string, unknown>,
  };
  return { monaco, registered, diagnostics: () => tsDiagnostics };
}

describe("built-in Monaco language services under a language server", () => {
  it("silences the built-in TypeScript hover while the LSP runs, so hovers are not shown twice", () => {
    const { monaco, registered, diagnostics } = fakeMonaco();
    installBuiltInLanguageSuppression(monaco as never);
    const register = monaco.languages.registerHoverProvider as (selector: string, provider: object) => unknown;
    register("typescript", new HoverAdapter());
    register("typescript", { provideHover: () => ({ contents: [{ value: "from LSP" }] }) });
    const [builtIn, lsp] = registered;

    const release = suppressBuiltInLanguageService("typescript");
    expect(builtIn.provider.provideHover()).toBeUndefined();
    expect(lsp.provider.provideHover()).toEqual({ contents: [{ value: "from LSP" }] });
    expect(diagnostics()).toMatchObject({ noSemanticValidation: true, noSyntaxValidation: true });

    release();
    expect(builtIn.provider.provideHover()).toEqual({ contents: [{ value: "type X = 1" }] });
    expect(diagnostics()).toMatchObject({ noSemanticValidation: false });
  });

  it("leaves languages without a matching service untouched", () => {
    const { monaco, registered } = fakeMonaco();
    installBuiltInLanguageSuppression(monaco as never);
    (monaco.languages.registerHoverProvider as (selector: string, provider: object) => unknown)("rust", new HoverAdapter());
    const release = suppressBuiltInLanguageService("typescript");
    expect(registered[0].provider.provideHover()).toEqual({ contents: [{ value: "type X = 1" }] });
    release();
  });
});
