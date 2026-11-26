import { fileTypeForPath } from "./codeFileTypes";

export function CodeFileIcon({ path }: { path: string }) {
  const type = fileTypeForPath(path);
  return <span className={`zorai-code-file-icon type-${type.id}`} title={`${type.label} file`} role="img" aria-label={`${type.label} file`}>{type.glyph}</span>;
}

export function CodeFolderChevron({ expanded }: { expanded: boolean }) {
  return <svg className={expanded ? "zorai-code-folder-chevron zorai-motion-caret is-open" : "zorai-code-folder-chevron zorai-motion-caret"} viewBox="0 0 16 16" aria-hidden="true"><path d="m5 3 5 5-5 5" /></svg>;
}

export function CodeFolderIcon() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />
    </svg>
  );
}
