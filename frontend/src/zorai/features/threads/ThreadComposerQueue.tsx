import { AttachmentTiles } from "./attachmentTiles";
import { queuedComposerPreview, type QueuedComposerMessage } from "./composerQueue";

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
  if (items.length === 0) return null;
  return (
    <div className="zorai-composer-queue">
      {items.map((queued, index) => {
        const { text, tiles } = queuedComposerPreview(queued);
        const editing = queued.id === editingId;
        return (
          <div
            key={queued.id}
            className={["zorai-composer-queue__chip", editing ? "zorai-composer-queue__chip--editing" : ""]
              .filter(Boolean)
              .join(" ")}
          >
            <div className="zorai-composer-queue__header">
              <span className="zorai-composer-queue__label">
                {editing ? "Editing" : `Queued ${index + 1}`}
                {tiles.length > 0 ? (
                  <span className="zorai-composer-queue__count">
                    {tiles.length} attachment{tiles.length === 1 ? "" : "s"}
                  </span>
                ) : null}
              </span>
              <div className="zorai-composer-queue__actions">
                <button
                  type="button"
                  className="zorai-ghost-button zorai-ghost-button--compact"
                  title="Edit this queued message"
                  onClick={() => onEdit(queued)}
                >
                  Edit
                </button>
                <button
                  type="button"
                  className="zorai-ghost-button zorai-ghost-button--compact zorai-composer-queue__primary"
                  title="Interrupt the current response and send this message now"
                  onClick={() => onSendNow(queued.id)}
                >
                  Send now
                </button>
                <button
                  type="button"
                  className="zorai-composer-queue__remove"
                  aria-label="Cancel queued message"
                  title="Cancel queued message"
                  onClick={() => onCancel(queued.id)}
                >
                  ×
                </button>
              </div>
            </div>
            <div className="zorai-composer-queue__body">
              {text ? <p className="zorai-composer-queue__text" title={text}>{text}</p> : null}
              {tiles.length > 0 ? (
                <div className="zorai-composer-queue__attachments">
                  <AttachmentTiles items={tiles} />
                </div>
              ) : null}
            </div>
          </div>
        );
      })}
    </div>
  );
}
