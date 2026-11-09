import { describe, expect, it } from "vitest";
import { splitMessageAttachments } from "./messageAttachments";

describe("splitMessageAttachments", () => {
  it("surfaces image content blocks as tiles even when the text body is empty", () => {
    const result = splitMessageAttachments("", [{
      type: "image",
      data_url: "data:image/png;base64,abc",
      mime_type: "image/png",
    }]);

    expect(result.displayText).toBe("");
    expect(result.tiles).toEqual([{
      id: "block:image:0",
      kind: "image",
      name: "image.png",
      previewUrl: "data:image/png;base64,abc",
    }]);
  });

  it("surfaces generated video blocks as video tiles that carry the playable url", () => {
    const result = splitMessageAttachments("Generated video.", [{
      type: "video",
      url: "file:///tmp/media/video-1.mp4",
      mime_type: "video/mp4",
    }]);

    expect(result.tiles).toEqual([{
      id: "block:video:0",
      kind: "video",
      name: "video-1.mp4",
      previewUrl: "file:///tmp/media/video-1.mp4",
    }]);
  });

  it("turns inlined attached_file wrappers into file tiles and hides the raw markup", () => {
    const content = `<attached_file name="notes.txt">\nhello\n</attached_file>\n\nlook at this`;
    const result = splitMessageAttachments(content);

    expect(result.displayText).toBe("look at this");
    expect(result.tiles).toEqual([{
      id: "attached-file:notes.txt:0",
      kind: "text",
      name: "notes.txt",
    }]);
  });

  it("hides the daemon's text stand-in for media that is already shown as a tile", () => {
    const result = splitMessageAttachments("[Image attachment: image/png]\nlook here\n[Video attachment]", [{
      type: "image",
      data_url: "data:image/png;base64,abc",
      mime_type: "image/png",
    }]);

    expect(result.displayText).toBe("look here\n[Video attachment]");
  });

  it("keeps the stand-in text when the media block itself is missing, so the attachment is still mentioned", () => {
    expect(splitMessageAttachments("[Image attachment: image/png]").displayText).toBe("[Image attachment: image/png]");
  });

  it("shows a generated song under its saved file name so several tracks in one thread can be told apart", () => {
    const result = splitMessageAttachments("Generated music.", [{
      type: "audio",
      url: "file:///home/me/.zorai/threads/t1/artifacts/media/music-1793960000000.mp3",
      mime_type: "audio/mpeg",
    }]);

    expect(result.tiles[0]).toMatchObject({ kind: "audio", name: "music-1793960000000.mp3" });
    expect(result.displayText).toBe("Generated music.");
  });

  it("turns path-only attached_file references into file tiles so queued and sent messages never show raw markup", () => {
    const { displayText, tiles } = splitMessageAttachments(
      '<attached_file name="report.pdf" path="/tmp/report.pdf" mime_type="application/pdf" />\n\nsummarize this',
    );
    expect(tiles).toEqual([{ id: "attached-path:/tmp/report.pdf:0", kind: "file", name: "report.pdf" }]);
    expect(displayText).toBe("summarize this");
  });
});
