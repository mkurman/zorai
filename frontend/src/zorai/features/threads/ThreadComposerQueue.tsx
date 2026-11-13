import { useState } from "react";
import { AttachmentTiles } from "./attachmentTiles";
import { queuedComposerPreview, type QueuedComposerMessage } from "./composerQueue";
import { usePresence } from "@/hooks/usePresence";

export function ThreadComposerQueue({
  items,
  editingId,
  onEdit,
  onSendNow,
  onCancel,
}: {
  items: QueuedComposerMessage[];
  editingId: string | null;
  onEdit: (item: QueuedComposerMessage) => void;
  onSendNow: (id: string) => void;
  onCancel: (id: string) => void;
}) {
  const [collapsed, setCollapsed] = useState(false);
  const presence = usePresence(!collapsed);
  if (items.length === 0) return null;
  return (
    <div className="zorai-composer-queue">
      <button
        type="button"
        className="zorai-composer-queue__header"
        aria-expanded={!collapsed}
        onClick={() => setCollapsed((value) => !value)}
      >
        <span className={collapsed ? "zorai-composer-queue__chevron zorai-motion-caret" : "zorai-composer-queue__chevron zorai-motion-caret is-open"} aria-hidden="true">▸</span>
        <span className="zorai-composer-queue__label">Queued</span>
        <span className="zorai-composer-queue__count">{items.length}</span>
        {collapsed ? (
          <span className="zorai-composer-queue__peek">{queuedComposerPreview(items[0]).text}</span>
        ) : null}
      </button>
      {!presence.mounted ? null : (
        <ol data-closing={presence.closing || undefined} className="zorai-composer-queue__list zorai-reveal">
          {items.map((queued, index) => {
            const { text, tiles } = queuedComposerPreview(queued);
            const editing = queued.id === editingId;
            return (
              <li
                key={queued.id}
                className={["zorai-composer-queue__row", editing ? "zorai-composer-queue__row--editing" : ""]
                  .filter(Boolean)
                  .join(" ")}
              >
                <div className="zorai-composer-queue__line">
                  <span className="zorai-composer-queue__index">{editing ? "✎" : index + 1}</span>
                  <span className="zorai-composer-queue__text" title={text}>
                    {text || <em>{tiles.length === 1 ? "1 attachment" : `${tiles.length} attachments`}</em>}
                  </span>
                  {tiles.length > 0 && text ? (
                    <span className="zorai-composer-queue__attachment-count" title={tiles.map((tile) => tile.name).join(", ")}>
                      +{tiles.length} file{tiles.length === 1 ? "" : "s"}
                    </span>
                  ) : null}
                  <span className="zorai-composer-queue__actions">
                    <button type="button" className="zorai-composer-queue__action" title="Edit this queued message" onClick={() => onEdit(queued)}>
                      Edit
                    </button>
                    <button
                      type="button"
                      className="zorai-composer-queue__action zorai-composer-queue__action--primary"
                      title="Interrupt the current response and send this message now"
                      onClick={() => onSendNow(queued.id)}
                    >
                      Send now
                    </button>
                    <button
                      type="button"
                      className="zorai-composer-queue__action zorai-composer-queue__action--remove"
                      aria-label="Cancel queued message"
                      title="Cancel queued message"
                      onClick={() => onCancel(queued.id)}
                    >
                      ×
                    </button>
                  </span>
                </div>
                {tiles.length > 0 ? (
                  <div className="zorai-composer-queue__attachments">
                    <AttachmentTiles items={tiles} />
                  </div>
                ) : null}
              </li>
            );
          })}
        </ol>
      )}
    </div>
  );
}
