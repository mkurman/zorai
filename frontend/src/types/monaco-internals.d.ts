declare module "monaco-editor/esm/vs/editor/standalone/browser/standaloneServices" {
  export const StandaloneServices: { get<T>(id: unknown): T };
}

declare module "monaco-editor/esm/vs/platform/storage/common/storage" {
  export const IStorageService: unknown;
}
