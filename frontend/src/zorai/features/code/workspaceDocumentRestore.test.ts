import { describe, expect, it } from "vitest";
import { codeMediaKind, shouldRestoreWorkspaceDocument } from "./workspaceDocumentRestore";

describe("shouldRestoreWorkspaceDocument", () => {
  it("reloads a persisted tab whose contents were not kept in memory across a window restart", () => {
    expect(shouldRestoreWorkspaceDocument("README.md", {})).toBe(true);
  });

  it("does not refetch a tab that already has a document after the explorer opened it", () => {
    expect(shouldRestoreWorkspaceDocument("README.md", { "README.md": { content: "# hi" } })).toBe(false);
    expect(shouldRestoreWorkspaceDocument(null, {})).toBe(false);
  });
});

describe("code tab media files", () => {
  it("never restores a persisted image, video or audio tab into the text editor, which cannot read binary files", () => {
    expect(shouldRestoreWorkspaceDocument("docs/assets/og_image.jpg", {})).toBe(false);
    expect(shouldRestoreWorkspaceDocument("clips/demo.mp4", {})).toBe(false);
  });

  it("sends binary media to the lightbox but keeps SVG editable as text", () => {
    expect(codeMediaKind("docs/assets/og_image.jpg")).toBe("image");
    expect(codeMediaKind("clips/demo.mp4")).toBe("video");
    expect(codeMediaKind("sounds/ping.mp3")).toBe("audio");
    expect(codeMediaKind("docs/assets/icon.ico")).toBe("image");
    expect(codeMediaKind("docs/assets/logo.svg")).toBeNull();
    expect(codeMediaKind("src/main.ts")).toBeNull();
  });
});
