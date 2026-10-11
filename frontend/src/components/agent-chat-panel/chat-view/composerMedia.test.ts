import { describe, expect, it } from "vitest";
import {
  buildAttachmentSendPayload,
  collectClipboardFiles,
  readSpeechToTextContent,
  readSpeechToTextError,
} from "./composerMedia";
import { boundContentBlocks } from "@/lib/agentStore/rendererText";

describe("speech-to-text result parsing", () => {
  it("reads a daemon plain-text payload that arrives as event data", () => {
    expect(readSpeechToTextContent("hello from mic")).toBe("hello from mic");
    expect(readSpeechToTextContent({ data: "hello from mic" })).toBe("hello from mic");
  });

  it("reads nested JSON transcription objects", () => {
    expect(readSpeechToTextContent({ text: "direct" })).toBe("direct");
    expect(readSpeechToTextContent({ data: { text: "nested" } })).toBe("nested");
    expect(readSpeechToTextContent({ data: { transcript: "legacy" } })).toBe("legacy");
  });

  it("surfaces bridge errors instead of treating them as empty transcripts", () => {
    expect(readSpeechToTextContent({ error: "missing audio" })).toBe("");
    expect(readSpeechToTextError({ error: "missing audio" })).toBe("missing audio");
    expect(readSpeechToTextError({ ok: false, message: "bridge down" })).toBe("bridge down");
  });
});

describe("collectClipboardFiles", () => {
  it("reads screenshot-style clipboard items when files is empty", () => {
    const pasted = new File([new Uint8Array([1, 2, 3])], "", { type: "image/png" });
    const clipboardData = {
      files: [],
      items: [{
        kind: "file",
        type: "image/png",
        getAsFile: () => pasted,
      }],
    } as unknown as DataTransfer;

    const files = collectClipboardFiles(clipboardData);
    expect(files).toHaveLength(1);
    expect(files[0]?.type).toBe("image/png");
    expect(files[0]?.name).toBe("pasted-image-1.png");
  });

  it("prefers DataTransfer.files when Chromium already populated that list", () => {
    const named = new File([new Uint8Array([9])], "shot.png", { type: "image/png" });
    const clipboardData = {
      files: [named],
      items: [{
        kind: "file",
        type: "image/png",
        getAsFile: () => named,
      }],
    } as unknown as DataTransfer;

    const files = collectClipboardFiles(clipboardData);
    expect(files).toHaveLength(1);
    expect(files[0]?.name).toBe("shot.png");
  });
});

describe("buildAttachmentSendPayload", () => {
  it("serializes image data URLs for both local preview and daemon transport", () => {
    const dataUrl = "data:image/png;base64,iVBORw0KGgo=";
    const payload = buildAttachmentSendPayload("what is shown?", [{
      id: "image-1",
      name: "picture.png",
      size: 11,
      kind: "image",
      mimeType: "image/png",
      dataUrl,
    }]);

    expect(payload.localContentBlocks).toEqual([
      { type: "text", text: "what is shown?" },
      { type: "image", data_url: dataUrl, mime_type: "image/png" },
    ]);
    expect(JSON.parse(payload.contentBlocksJson ?? "null")).toEqual(payload.localContentBlocks);
  });

  it("shows a renderer-sized preview locally while the daemon still receives the full image", () => {
    const fullDataUrl = `data:image/png;base64,${"A".repeat(400_000)}`;
    const previewDataUrl = "data:image/webp;base64,UklGRg==";
    const payload = buildAttachmentSendPayload("look", [{
      id: "image-1",
      name: "pasted-image-1.png",
      size: 300_000,
      kind: "image",
      mimeType: "image/png",
      dataUrl: fullDataUrl,
      previewDataUrl,
    }]);

    expect(payload.localContentBlocks?.[1]).toEqual({ type: "image", data_url: previewDataUrl, mime_type: "image/png" });
    expect(boundContentBlocks(payload.localContentBlocks)?.[1]).toEqual(payload.localContentBlocks?.[1]);
    expect(JSON.parse(payload.contentBlocksJson ?? "null")[1]).toEqual({ type: "image", data_url: fullDataUrl, mime_type: "image/png" });
  });
});
