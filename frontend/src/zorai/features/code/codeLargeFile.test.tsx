import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { CodeUnsupportedFileNotice, exceedsCodeFileLimit, isBinaryFileError } from "./CodeLargeFileGate";

describe("large file gate", () => {
  it("uses a 5 MB boundary without rejecting exactly-at-limit files", () => {
    expect(exceedsCodeFileLimit(5 * 1024 * 1024, 5)).toBe(false);
    expect(exceedsCodeFileLimit(5 * 1024 * 1024 + 1, 5)).toBe(true);
  });
  it("normalizes configured limits into 1–100 MB", () => {
    expect(exceedsCodeFileLimit(1024 * 1024 + 1, 0)).toBe(true);
    expect(exceedsCodeFileLimit(100 * 1024 * 1024, 900)).toBe(false);
  });
});

describe("unsupported binary files", () => {
  it("recognises the editor's binary refusal even after it was wrapped by the IPC layer, so it becomes a notice instead of an error", () => {
    expect(isBinaryFileError(new Error("Error invoking remote method 'workspace-read-file': Error: Binary files cannot be opened in the text editor."))).toBe(true);
    expect(isBinaryFileError(new Error("ENOENT: no such file"))).toBe(false);
  });

  it("names the file and offers a way out rather than a dead end", () => {
    const html = renderToStaticMarkup(<CodeUnsupportedFileNotice path="docs/assets/icon.ico" onOpenExternal={() => undefined} onReveal={() => undefined} onDismiss={() => undefined} />);
    expect(html).toContain("icon.ico can&#x27;t be opened here");
    expect(html).toContain("Open externally");
    expect(html).toContain("Dismiss");
  });
});
