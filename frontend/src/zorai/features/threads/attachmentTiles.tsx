import { useEffect, useState } from "react";
import { createPortal } from "react-dom";
import type { ComposerAttachment } from "@/components/agent-chat-panel/chat-view/types";
import { getBridge } from "@/lib/bridge";
import { ImageLightbox } from "./ImageLightbox";
import { Presence } from "@/components/Presence";

export type AttachmentTileModel = {
  id: string;
  kind: "image" | "audio" | "video" | "text" | "file";
  name: string;
  previewUrl?: string;
  inventoryPath?: string;
  badge?: string;
};

export function composerAttachmentToTile(attachment: ComposerAttachment): AttachmentTileModel {
  return {
    id: attachment.id,
    kind: attachment.kind,
    name: attachment.name,
    previewUrl: attachment.dataUrl,
  };
}

export function AttachmentTiles({
  items,
  onRemove,
  onAnnotate,
}: {
  items: AttachmentTileModel[];
  onRemove?: (id: string) => void;
  onAnnotate?: (id: string) => void;
}) {
  if (items.length === 0) {
    return null;
  }
  return (
    <div className="zorai-attachment-tiles">
      {items.map((item) => (
        <AttachmentTile key={item.id} item={item} onRemove={onRemove} onAnnotate={onAnnotate} />
      ))}
    </div>
  );
}

function AttachmentTile({
  item,
  onRemove,
  onAnnotate,
}: {
  item: AttachmentTileModel;
  onRemove?: (id: string) => void;
  onAnnotate?: (id: string) => void;
}) {
  const imageUrl = useAttachmentImageUrl(item);
  const videoUrl = item.kind === "video" ? item.previewUrl : undefined;
  const [previewOpen, setPreviewOpen] = useState(false);
  if (item.kind === "audio" && item.previewUrl) {
    return (
      <div className="zorai-attachment-audio">
        <span className="zorai-attachment-audio__name" title={item.name}>{item.name}</span>
        <audio controls preload="metadata" src={item.previewUrl} aria-label={`Play ${item.name}`} />
        {onRemove ? (
          <button
            type="button"
            className="zorai-attachment-tile__remove"
            title={`Remove ${item.name}`}
            aria-label={`Remove ${item.name}`}
            onClick={() => onRemove(item.id)}
          >
            ×
          </button>
        ) : null}
      </div>
    );
  }
  return (
    <div className="zorai-attachment-tile">
      {imageUrl ? (
        <button
          type="button"
          className="zorai-attachment-tile__preview zorai-attachment-tile__preview--button"
          title={onAnnotate ? `Draw on ${item.name}` : `Preview ${item.name}`}
          aria-label={onAnnotate ? `Draw on ${item.name}` : `Preview ${item.name}`}
          onClick={() => (onAnnotate ? onAnnotate(item.id) : setPreviewOpen(true))}
        >
          <img src={imageUrl} alt="" />
        </button>
      ) : videoUrl ? (
        <button
          type="button"
          className="zorai-attachment-tile__preview zorai-attachment-tile__preview--button"
          title={`Play ${item.name}`}
          aria-label={`Play ${item.name}`}
          onClick={() => setPreviewOpen(true)}
        >
          <AttachmentKindIcon kind={item.kind} />
        </button>
      ) : (
        <span className="zorai-attachment-tile__preview zorai-attachment-tile__preview--icon" aria-hidden="true">
          <AttachmentKindIcon kind={item.kind} />
        </span>
      )}
      <span className="zorai-attachment-tile__caption" title={item.name}>{item.name}</span>
      {item.badge ? <span className="zorai-attachment-tile__badge">{item.badge}</span> : null}
      {onRemove ? (
        <button
          type="button"
          className="zorai-attachment-tile__remove"
          title={`Remove ${item.name}`}
          aria-label={`Remove ${item.name}`}
          onClick={() => onRemove(item.id)}
        >
          ×
        </button>
      ) : null}
      <Presence value={previewOpen && imageUrl}>
        {(url, closing) => <ImageLightbox src={url} name={item.name} closing={closing} onClose={() => setPreviewOpen(false)} />}
      </Presence>
      <Presence value={previewOpen && videoUrl}>
        {(url, closing) => <AttachmentImagePreview src={url} name={item.name} video closing={closing} onClose={() => setPreviewOpen(false)} />}
      </Presence>
    </div>
  );
}

function useAttachmentImageUrl(item: AttachmentTileModel): string | undefined {
  const [inventoryUrl, setInventoryUrl] = useState<string | undefined>(undefined);
  const needsInventory = item.kind === "image" && !item.previewUrl && Boolean(item.inventoryPath);
  useEffect(() => {
    if (!needsInventory || !item.inventoryPath) {
      return;
    }
    let cancelled = false;
    void getBridge()?.agentReadImageFile?.(item.inventoryPath).then((dataUrl) => {
      if (!cancelled && dataUrl) {
        setInventoryUrl(dataUrl);
      }
    });
    return () => {
      cancelled = true;
    };
  }, [needsInventory, item.inventoryPath]);
  if (item.kind !== "image") {
    return undefined;
  }
  return item.previewUrl ?? inventoryUrl;
}

function AttachmentImagePreview({ src, name, video, closing, onClose }: { src: string; name: string; video?: boolean; closing?: boolean; onClose: () => void }) {
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        onClose();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onClose]);
  return createPortal(
    <div data-closing={closing || undefined} className="zorai-image-preview" role="dialog" aria-modal="true" aria-label={name} onClick={onClose}>
      {video ? (
        <video src={src} controls autoPlay preload="metadata" onClick={(event) => event.stopPropagation()} />
      ) : (
        <img src={src} alt={name} />
      )}
    </div>,
    document.body,
  );
}

function AttachmentKindIcon({ kind }: { kind: AttachmentTileModel["kind"] }) {
  if (kind === "audio") {
    return (
      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
        <path d="M9 18V5l12-2v13" />
        <circle cx="6" cy="18" r="3" />
        <circle cx="18" cy="16" r="3" />
      </svg>
    );
  }
  if (kind === "video") {
    return (
      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
        <rect x="2" y="5" width="14" height="14" rx="2" />
        <polygon points="22 7 16 12 22 17 22 7" />
      </svg>
    );
  }
  return (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
      <polyline points="14 2 14 8 20 8" />
    </svg>
  );
}
