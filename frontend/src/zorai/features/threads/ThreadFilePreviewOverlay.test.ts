import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  agentReadImageFile: vi.fn(),
  fetchFilePreview: vi.fn(),
}));

vi.mock("@/lib/bridge", () => ({
  getBridge: () => ({ agentReadImageFile: mocks.agentReadImageFile }),
}));

vi.mock("@/lib/agentWorkContext", () => ({
  fetchFilePreview: mocks.fetchFilePreview,
  fetchGitDiff: vi.fn(),
}));

vi.mock("@/components/WorkspaceCodeEditor", () => ({ MonacoReadOnlyView: () => null }));

import { loadFilePreviewSection } from "./ThreadFilePreviewOverlay";

describe("loadFilePreviewSection", () => {
  beforeEach(() => {
    mocks.agentReadImageFile.mockReset();
    mocks.fetchFilePreview.mockReset();
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

  it("does not try the image reader for text files", async () => {
    mocks.fetchFilePreview.mockResolvedValue({ path: "/work/a.ts", content: "x", truncated: false, isText: true });

    await loadFilePreviewSection("/work/a.ts");

    expect(mocks.agentReadImageFile).not.toHaveBeenCalled();
  });
});
