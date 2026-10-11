import { MAX_RENDERER_TEXT_CHARS } from "@/lib/agentStore/rendererText";
import type { AgentContentBlock } from "@/lib/agentStore/types";
import type { ComposerAttachment, SendMessagePayload } from "./types";

const TEXT_ATTACHMENT_EXTENSIONS = new Set([
  "txt", "md", "markdown", "json", "yaml", "yml", "toml", "ini", "cfg", "conf",
  "rs", "ts", "tsx", "js", "jsx", "py", "sh", "sql", "csv", "log",
]);

function fileLooksTextual(file: File): boolean {
  if (file.type.startsWith("text/")) return true;
  const ext = file.name.includes(".") ? file.name.split(".").pop()?.toLowerCase() ?? "" : "";
  return TEXT_ATTACHMENT_EXTENSIONS.has(ext);
}

export async function readComposerAttachment(file: File): Promise<ComposerAttachment | null> {
  const kind = file.type.startsWith("image/")
    ? "image"
    : file.type.startsWith("audio/")
      ? "audio"
      : fileLooksTextual(file)
        ? "text"
        : null;
  if (!kind) return null;

  if (kind === "text") {
    return {
      id: composerAttachmentId(file),
      name: file.name,
      size: file.size,
      kind,
      mimeType: file.type || "text/plain",
      textContent: await file.text(),
    };
  }

  const dataUrl = await new Promise<string>((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(typeof reader.result === "string" ? reader.result : "");
    reader.onerror = () => reject(reader.error ?? new Error("file read failed"));
    reader.readAsDataURL(file);
  });
  return {
    id: composerAttachmentId(file),
    name: file.name,
    size: file.size,
    kind,
    mimeType: file.type || (kind === "image" ? "image/png" : "audio/wav"),
    dataUrl,
    previewDataUrl: kind === "image" ? await createImagePreviewDataUrl(dataUrl) : undefined,
  };
}

const IMAGE_PREVIEW_MAX_DIMENSIONS = [1600, 1280, 960, 640, 320];
const IMAGE_PREVIEW_QUALITIES = [0.85, 0.6];

async function createImagePreviewDataUrl(dataUrl: string): Promise<string | undefined> {
  if (dataUrl.length <= MAX_RENDERER_TEXT_CHARS) {
    return undefined;
  }
  try {
    const image = new Image();
    image.src = dataUrl;
    await image.decode();
    const canvas = document.createElement("canvas");
    const context = canvas.getContext("2d");
    if (!context || !image.naturalWidth || !image.naturalHeight) {
      return undefined;
    }
    for (const maxDimension of IMAGE_PREVIEW_MAX_DIMENSIONS) {
      const scale = Math.min(1, maxDimension / Math.max(image.naturalWidth, image.naturalHeight));
      canvas.width = Math.max(1, Math.round(image.naturalWidth * scale));
      canvas.height = Math.max(1, Math.round(image.naturalHeight * scale));
      context.clearRect(0, 0, canvas.width, canvas.height);
      context.drawImage(image, 0, 0, canvas.width, canvas.height);
      for (const quality of IMAGE_PREVIEW_QUALITIES) {
        const preview = canvas.toDataURL("image/webp", quality);
        if (preview.length <= MAX_RENDERER_TEXT_CHARS) {
          return preview;
        }
      }
    }
  } catch {
    return undefined;
  }
  return undefined;
}

function composerAttachmentId(file: File): string {
  return `${file.name}:${file.size}:${file.lastModified}:${Math.random().toString(36).slice(2, 8)}`;
}

function clipboardFileName(file: File, index: number): string {
  if (file.name.trim() && file.name !== "blob") {
    return file.name;
  }
  const subtype = (file.type.split("/")[1] || "bin").split(";")[0] || "bin";
  const kind = file.type.startsWith("image/")
    ? "image"
    : file.type.startsWith("audio/")
      ? "audio"
      : "file";
  return `pasted-${kind}-${index + 1}.${subtype}`;
}

export function collectClipboardFiles(clipboardData: DataTransfer | null | undefined): File[] {
  if (!clipboardData) {
    return [];
  }
  const fromList = Array.from(clipboardData.files ?? []);
  const raw = fromList.length > 0 ? fromList : filesFromClipboardItems(clipboardData.items);
  return raw.map((file, index) => {
    const name = clipboardFileName(file, index);
    return name === file.name ? file : new File([file], name, { type: file.type, lastModified: file.lastModified });
  });
}

function filesFromClipboardItems(items: DataTransferItemList | undefined): File[] {
  if (!items || items.length === 0) {
    return [];
  }
  const files: File[] = [];
  for (let index = 0; index < items.length; index += 1) {
    const item = items[index];
    if (!item || item.kind !== "file") {
      continue;
    }
    const file = item.getAsFile();
    if (file) {
      files.push(file);
    }
  }
  return files;
}

export async function blobToBase64(blob: Blob): Promise<string> {
  const dataUrl = await new Promise<string>((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(typeof reader.result === "string" ? reader.result : "");
    reader.onerror = () => reject(reader.error ?? new Error("blob read failed"));
    reader.readAsDataURL(blob);
  });
  const commaIndex = dataUrl.indexOf(",");
  return commaIndex >= 0 ? dataUrl.slice(commaIndex + 1) : dataUrl;
}

const RECORDER_MIME_CANDIDATES = [
  "audio/webm;codecs=opus",
  "audio/webm",
  "audio/ogg;codecs=opus",
];

export function mediaRecorderOptions(): MediaRecorderOptions | undefined {
  if (typeof MediaRecorder === "undefined" || typeof MediaRecorder.isTypeSupported !== "function") {
    return undefined;
  }
  const mimeType = RECORDER_MIME_CANDIDATES.find((type) => MediaRecorder.isTypeSupported(type));
  return mimeType ? { mimeType } : undefined;
}

export function stopMediaTracks(stream: MediaStream | null | undefined): void {
  stream?.getTracks().forEach((track) => track.stop());
}

export async function collectMediaRecorderBlob(
  recorder: MediaRecorder,
  chunks: Blob[],
): Promise<Blob> {
  const mimeType = recorder.mimeType || chunks[0]?.type || "audio/webm";
  if (recorder.state !== "inactive") {
    await new Promise<void>((resolve, reject) => {
      recorder.addEventListener("error", () => reject(new Error("recording failed")), { once: true });
      recorder.addEventListener("stop", () => resolve(), { once: true });
      try {
        recorder.requestData();
      } catch {
        // Some Chromium builds throw if no timeslice chunk is pending.
      }
      recorder.stop();
    });
    await new Promise((resolve) => window.setTimeout(resolve, 50));
  }
  return new Blob(chunks, { type: mimeType });
}

function readNestedSpeechText(value: unknown): string {
  if (typeof value === "string") {
    return value.trim();
  }
  if (!value || typeof value !== "object") {
    return "";
  }
  const record = value as Record<string, unknown>;
  if (typeof record.text === "string") {
    return record.text.trim();
  }
  if (typeof record.content === "string") {
    return record.content.trim();
  }
  if (typeof record.transcript === "string") {
    return record.transcript.trim();
  }
  return "";
}

export function readSpeechToTextError(result: unknown): string {
  if (!result || typeof result !== "object") {
    return "";
  }
  const record = result as Record<string, unknown>;
  if (typeof record.error === "string" && record.error.trim()) {
    return record.error.trim();
  }
  if (record.ok === false && typeof record.message === "string" && record.message.trim()) {
    return record.message.trim();
  }
  return "";
}

export function readSpeechToTextContent(result: unknown): string {
  if (typeof result === "string") {
    return result.trim();
  }
  if (!result || typeof result !== "object") {
    return "";
  }
  const record = result as Record<string, unknown>;
  const direct = readNestedSpeechText(record);
  if (direct) {
    return direct;
  }
  return readNestedSpeechText(record.data);
}

export function buildAttachmentSendPayload(text: string, attachments: ComposerAttachment[]): SendMessagePayload {
  const trimmedText = text.trim();
  const textAttachmentWrappers = attachments
    .filter((attachment) => attachment.kind === "text" && attachment.textContent)
    .map((attachment) => `<attached_file name="${attachment.name}">\n${attachment.textContent}\n</attached_file>`);
  const mediaAttachments = attachments.filter((attachment) => attachment.kind !== "text");
  const finalText = [...textAttachmentWrappers, trimmedText].filter(Boolean).join("\n\n").trim();

  if (mediaAttachments.length === 0) {
    return { text: finalText };
  }
  const contentBlocks: AgentContentBlock[] = [
    ...(finalText ? [{ type: "text", text: finalText } as const] : []),
    ...mediaAttachments.map((attachment) =>
      attachment.kind === "image"
        ? ({
            type: "image",
            data_url: attachment.dataUrl,
            mime_type: attachment.mimeType,
          } as const)
        : ({
            type: "audio",
            data_url: attachment.dataUrl,
            mime_type: attachment.mimeType,
          } as const),
    ),
  ];
  const localContentBlocks = contentBlocks.map((block, index) => {
    const attachment = mediaAttachments[index - (finalText ? 1 : 0)];
    return block.type === "image" && attachment?.previewDataUrl
      ? { ...block, data_url: attachment.previewDataUrl }
      : block;
  });
  return {
    text: finalText,
    contentBlocksJson: JSON.stringify(contentBlocks),
    localContentBlocks,
  };
}
