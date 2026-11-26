export type BuiltInLanguageService = "typescript" | "html" | "css";

export const BUILT_IN_SERVICE_BY_LANGUAGE: Record<string, BuiltInLanguageService> = {
  typescript: "typescript",
  javascript: "typescript",
  html: "html",
  handlebars: "html",
  razor: "html",
  css: "css",
  scss: "css",
  less: "css",
};

const GATED_REGISTRATIONS = [
  "registerHoverProvider",
  "registerCompletionItemProvider",
  "registerDefinitionProvider",
  "registerReferenceProvider",
  "registerRenameProvider",
  "registerCodeActionProvider",
  "registerDocumentSymbolProvider",
  "registerDocumentFormattingEditProvider",
  "registerDocumentRangeFormattingEditProvider",
  "registerOnTypeFormattingEditProvider",
] as const;

type DiagnosticsDefaults = { getDiagnosticsOptions?: () => object; setDiagnosticsOptions?: (options: object) => void; options?: { validate?: boolean }; setOptions?: (options: object) => void };

type MonacoLike = {
  languages: Record<string, unknown> & {
    typescript?: { typescriptDefaults: DiagnosticsDefaults; javascriptDefaults: DiagnosticsDefaults };
    css?: { cssDefaults: DiagnosticsDefaults; scssDefaults: DiagnosticsDefaults; lessDefaults: DiagnosticsDefaults };
  };
};

const suppressions: Record<BuiltInLanguageService, number> = { typescript: 0, html: 0, css: 0 };
const savedDiagnostics = new Map<DiagnosticsDefaults, object>();
let installedMonaco: MonacoLike | null = null;

function gate<T extends object>(provider: T, service: BuiltInLanguageService): T {
  return new Proxy(provider, {
    get(target, property, receiver) {
      const value = Reflect.get(target, property, receiver);
      if (typeof value !== "function" || !String(property).startsWith("provide")) return value;
      return (...args: unknown[]) => (suppressions[service] > 0 ? undefined : value.apply(target, args));
    },
  });
}

export function installBuiltInLanguageSuppression(monacoApi: MonacoLike): void {
  if (installedMonaco === monacoApi) return;
  installedMonaco = monacoApi;
  for (const name of GATED_REGISTRATIONS) {
    const original = monacoApi.languages[name];
    if (typeof original !== "function") continue;
    monacoApi.languages[name] = (selector: unknown, provider: unknown, ...rest: unknown[]) => {
      const service = typeof selector === "string" ? BUILT_IN_SERVICE_BY_LANGUAGE[selector] : undefined;
      const builtIn = provider !== null && typeof provider === "object" && Object.getPrototypeOf(provider) !== Object.prototype;
      return original.call(monacoApi.languages, selector, service && builtIn ? gate(provider as object, service) : provider, ...rest);
    };
  }
}

function diagnosticsDefaults(service: BuiltInLanguageService): DiagnosticsDefaults[] {
  const languages = installedMonaco?.languages;
  if (service === "typescript" && languages?.typescript) return [languages.typescript.typescriptDefaults, languages.typescript.javascriptDefaults];
  if (service === "css" && languages?.css) return [languages.css.cssDefaults, languages.css.scssDefaults, languages.css.lessDefaults];
  return [];
}

function setBuiltInDiagnostics(service: BuiltInLanguageService, enabled: boolean): void {
  for (const defaults of diagnosticsDefaults(service)) {
    if (defaults.setDiagnosticsOptions && defaults.getDiagnosticsOptions) {
      if (!enabled) {
        savedDiagnostics.set(defaults, defaults.getDiagnosticsOptions());
        defaults.setDiagnosticsOptions({ ...defaults.getDiagnosticsOptions(), noSemanticValidation: true, noSyntaxValidation: true, noSuggestionDiagnostics: true });
      } else defaults.setDiagnosticsOptions(savedDiagnostics.get(defaults) ?? defaults.getDiagnosticsOptions());
    } else if (defaults.setOptions && defaults.options) {
      if (!enabled) savedDiagnostics.set(defaults, defaults.options);
      defaults.setOptions(enabled ? savedDiagnostics.get(defaults) ?? defaults.options : { ...defaults.options, validate: false });
    }
  }
}

export function suppressBuiltInLanguageService(service: BuiltInLanguageService): () => void {
  if (suppressions[service]++ === 0) setBuiltInDiagnostics(service, false);
  let released = false;
  return () => {
    if (released) return;
    released = true;
    if (--suppressions[service] === 0) setBuiltInDiagnostics(service, true);
  };
}
