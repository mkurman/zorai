import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  agentReadImageFile: vi.fn(),
  agentReadAudioFile: vi.fn(),
  workspaceListDirectory: vi.fn(),
  fetchFilePreview: vi.fn(),
}));

vi.mock("@/lib/bridge", () => ({
  getBridge: () => ({ agentReadImageFile: mocks.agentReadImageFile, agentReadAudioFile: mocks.agentReadAudioFile, workspaceListDirectory: mocks.workspaceListDirectory }),
}));

vi.mock("@/lib/agentWorkContext", () => ({
  fetchFilePreview: mocks.fetchFilePreview,
  fetchGitDiff: vi.fn(),
}));

vi.mock("@/components/WorkspaceCodeEditor", () => ({ MonacoReadOnlyView: () => null }));

import { loadFilePreviewSection, mediaKindForPath, mediaUrlForPath } from "./ThreadFilePreviewOverlay";

describe("loadFilePreviewSection", () => {
  beforeEach(() => {
    mocks.agentReadImageFile.mockReset();
    mocks.agentReadAudioFile.mockReset();
    mocks.workspaceListDirectory.mockReset();
    mocks.fetchFilePreview.mockReset();
  });

  it("lists a folder's files and subfolders instead of calling it a binary file", async () => {
    mocks.fetchFilePreview.mockResolvedValue({ path: "~/Downloads/zorai-promo", content: "", truncated: false, isText: false });
    mocks.workspaceListDirectory.mockResolvedValue([
      { name: "frames", path: "frames", isDirectory: true, isSymbolicLink: false, sizeBytes: 0, modifiedAt: 1 },
      { name: "promo.mp4", path: "promo.mp4", isDirectory: false, isSymbolicLink: false, sizeBytes: 2048, modifiedAt: 1 },
    ]);

    const section = await loadFilePreviewSection("~/Downloads/zorai-promo");

    expect(mocks.workspaceListDirectory).toHaveBeenCalledWith("~/Downloads/zorai-promo", "");
    expect(section).toMatchObject({ kind: "directory", title: "Folder · 2 items" });
    expect(section?.text).not.toContain("Binary");
  });

  it("keeps the binary-file notice for real binary files", async () => {
    mocks.fetchFilePreview.mockResolvedValue({ path: "/work/blob.bin", content: "", truncated: false, isText: false });
    mocks.workspaceListDirectory.mockRejectedValue(new Error("not a directory"));

    const section = await loadFilePreviewSection("/work/blob.bin");

    expect(section).toMatchObject({ kind: "file-preview", text: "Binary file preview is not available." });
  });

  it("plays generated speech and other audio files instead of showing the binary-file notice", async () => {
    mocks.agentReadAudioFile.mockResolvedValue("data:audio/mpeg;base64,bXAz");

    const section = await loadFilePreviewSection("/tmp/zorai-speech-04b1.mp3");

    expect(mocks.agentReadAudioFile).toHaveBeenCalledWith("/tmp/zorai-speech-04b1.mp3");
    expect(section).toMatchObject({ kind: "audio", text: "data:audio/mpeg;base64,bXAz" });
    expect(mocks.fetchFilePreview).not.toHaveBeenCalled();
  });

  it("renders image files as images instead of the binary-file notice", async () => {
    mocks.agentReadImageFile.mockResolvedValue("data:image/png;base64,cG5n");

    const section = await loadFilePreviewSection("/work/out/chart.PNG");

    expect(section).toEqual({
      title: "Image preview",
      kind: "image",
      text: "data:image/png;base64,cG5n",
      path: "/work/out/chart.PNG",
    });
    expect(mocks.fetchFilePreview).not.toHaveBeenCalled();
  });

  it("falls back to the daemon file preview when the image cannot be read", async () => {
    mocks.agentReadImageFile.mockResolvedValue(null);
    mocks.fetchFilePreview.mockResolvedValue({ path: "/work/logo.svg", content: "<svg/>", truncated: false, isText: true });

    const section = await loadFilePreviewSection("/work/logo.svg");

    expect(section?.kind).toBe("file-preview");
    expect(section?.text).toBe("<svg/>");
  });

  it("previews generated mp4 files as playable video through the app media scheme instead of the binary notice", async () => {
    const section = await loadFilePreviewSection("/root/threads/t1/artifacts/media/video 1.mp4");

    expect(section).toEqual({
      title: "Video preview",
      kind: "video",
      text: "zorai-media://local/root/threads/t1/artifacts/media/video%201.mp4",
      path: "/root/threads/t1/artifacts/media/video 1.mp4",
    });
    expect(mocks.fetchFilePreview).not.toHaveBeenCalled();
    expect(mocks.agentReadImageFile).not.toHaveBeenCalled();
  });

  it("does not try the image reader for text files", async () => {
    mocks.fetchFilePreview.mockResolvedValue({ path: "/work/a.ts", content: "x", truncated: false, isText: true });

    await loadFilePreviewSection("/work/a.ts");

    expect(mocks.agentReadImageFile).not.toHaveBeenCalled();
  });
});

describe("media previews", () => {
  it("streams video through the app media scheme so it plays in dev and packaged builds", async () => {
    const section = await loadFilePreviewSection("~/Downloads/zorai-promo/this thread #1.mp4");
    expect(section).toMatchObject({ kind: "video", text: "zorai-media://local/~/Downloads/zorai-promo/this%20thread%20%231.mp4" });
    expect(mediaUrlForPath("/tmp/a?b.webm")).toBe("zorai-media://local/tmp/a%3Fb.webm");
  });

  it("recognises which files open in the image or media lightbox", () => {
    expect(mediaKindForPath("/x/shot.PNG")).toBe("image");
    expect(mediaKindForPath("/x/clip.mov")).toBe("video");
    expect(mediaKindForPath("/x/voice.flac")).toBe("audio");
    expect(mediaKindForPath("/x/notes.md")).toBeNull();
  });
});
