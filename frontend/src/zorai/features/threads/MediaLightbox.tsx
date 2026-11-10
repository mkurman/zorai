import { useEffect } from "react";
import { createPortal } from "react-dom";

export function MediaLightbox({ src, name, kind, onClose }: {
  src: string | null;
  name: string;
  kind: "video" | "audio";
  onClose: () => void;
}) {
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onClose]);

  return createPortal(
    <div className="zorai-image-lightbox" role="dialog" aria-modal="true" aria-label={name}>
      <div className="zorai-image-lightbox__toolbar">
        <span className="zorai-image-lightbox__name" title={name}>{name}</span>
        <button type="button" className="zorai-ghost-button zorai-ghost-button--compact" title="Close (Esc)" aria-label="Close preview" onClick={onClose}>×</button>
      </div>
      <div className="zorai-media-lightbox__stage" onClick={(event) => { if (event.target === event.currentTarget) onClose(); }}>
        {src === null ? (
          <span className="zorai-media-lightbox__loading">Loading…</span>
        ) : kind === "video" ? (
          <video className="zorai-media-lightbox__video" src={src} controls autoPlay preload="metadata" />
        ) : (
          <div className="zorai-media-lightbox__audio">
            <strong title={name}>{name}</strong>
            <audio src={src} controls autoPlay preload="metadata" />
          </div>
        )}
      </div>
    </div>,
    document.body,
  );
}
