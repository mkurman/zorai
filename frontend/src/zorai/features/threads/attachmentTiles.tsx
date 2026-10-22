import { useEffect, useState } from "react";
import { createPortal } from "react-dom";
import type { ComposerAttachment } from "@/components/agent-chat-panel/chat-view/types";
import { getBridge } from "@/lib/bridge";

export type AttachmentTileModel = {
  id: string;
  kind: "image" | "audio" | "text" | "file";
  name: string;
  previewUrl?: string;
  inventoryPath?: string;
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
}: {
  items: AttachmentTileModel[];
  onRemove?: (id: string) => void;
}) {
  if (items.length === 0) {
    return null;
  }
  return (
    <div className="zorai-attachment-tiles">
      {items.map((item) => (
        <AttachmentTile key={item.id} item={item} onRemove={onRemove} />
      ))}
    </div>
  );
}

function AttachmentTile({ item, onRemove }: { item: AttachmentTileModel; onRemove?: (id: string) => void }) {
  const imageUrl = useAttachmentImageUrl(item);
  const [previewOpen, setPreviewOpen] = useState(false);
  return (
    <div className="zorai-attachment-tile">
      {imageUrl ? (
        <button
          type="button"
          className="zorai-attachment-tile__preview zorai-attachment-tile__preview--button"
          title={`Preview ${item.name}`}
          aria-label={`Preview ${item.name}`}
          onClick={() => setPreviewOpen(true)}
        >
          <img src={imageUrl} alt="" />
        </button>
      ) : (
        <span className="zorai-attachment-tile__preview zorai-attachment-tile__preview--icon" aria-hidden="true">
          <AttachmentKindIcon kind={item.kind} />
        </span>
      )}
      <span className="zorai-attachment-tile__caption" title={item.name}>{item.name}</span>
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
      {previewOpen && imageUrl ? (
        <AttachmentImagePreview src={imageUrl} name={item.name} onClose={() => setPreviewOpen(false)} />
      ) : null}
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

function AttachmentImagePreview({ src, name, onClose }: { src: string; name: string; onClose: () => void }) {
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
    <div className="zorai-image-preview" role="dialog" aria-modal="true" aria-label={name} onClick={onClose}>
      <img src={src} alt={name} />
    </div>,
    document.body,
  );
}

function AttachmentKindIcon({ kind }: { kind: "image" | "audio" | "text" | "file" }) {
  if (kind === "audio") {
    return (
      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
        <path d="M9 18V5l12-2v13" />
        <circle cx="6" cy="18" r="3" />
        <circle cx="18" cy="16" r="3" />
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
