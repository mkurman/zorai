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
      name: "video.mp4",
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
});
