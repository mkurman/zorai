import { useEffect, useLayoutEffect, useRef } from "react";
import { createPortal } from "react-dom";

export function ThreadOperationDialog({ operation, status, ended, busy, onStop, onRerun, onShowInChat, onClose }: {
  operation: ZoraiThreadOperation;
  status: string;
  ended: boolean;
  busy: boolean;
  onStop: () => void;
  onRerun: () => void;
  onShowInChat: (() => void) | null;
  onClose: () => void;
}) {
  const closeRef = useRef<HTMLButtonElement | null>(null);
  const outputRef = useRef<HTMLPreElement | null>(null);
  const followRef = useRef(true);
  const output = operation.output_tail ?? "";
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;

  useEffect(() => {
    closeRef.current?.focus();
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.stopPropagation();
      onCloseRef.current();
    };
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, []);

  useLayoutEffect(() => {
    const node = outputRef.current;
    if (node && followRef.current) node.scrollTop = node.scrollHeight;
  }, [output]);

  const title = operation.handle || operation.operation_id;
  return createPortal(
    <div className="zorai-code-dialog-backdrop" onMouseDown={(event) => { if (event.target === event.currentTarget) onClose(); }}>
      <section className="zorai-code-close-dialog zorai-operation-dialog" role="dialog" aria-modal="true" aria-labelledby="zorai-operation-dialog-title">
        <header className="zorai-operation-dialog__head">
          <h2 id="zorai-operation-dialog-title">{title}</h2>
          <span>{operation.tool_name}</span>
          <span className="zorai-operation-dialog__state">{status}</span>
        </header>
        <h3>Input</h3>
        <pre className="zorai-operation-dialog__block">{operation.command || "No command recorded."}</pre>
        <h3>{ended ? "Output (last check before it ended)" : "Output"}</h3>
        <pre
          ref={outputRef}
          className="zorai-operation-dialog__block zorai-operation-dialog__output"
          onScroll={(event) => {
            const node = event.currentTarget;
            followRef.current = node.scrollHeight - node.scrollTop - node.clientHeight < 24;
          }}
        >{output || (ended ? "No output was captured before it ended." : "No output yet.")}</pre>
        <footer>
          {onShowInChat ? <button type="button" onClick={onShowInChat}>Show in chat</button> : null}
          {ended
            ? <button type="button" disabled={!operation.command} onClick={onRerun}>Rerun</button>
            : <button type="button" className="is-danger" disabled={busy} onClick={onStop}>{busy ? "Stopping…" : "Stop"}</button>}
          <button ref={closeRef} type="button" onClick={onClose}>Close</button>
        </footer>
      </section>
    </div>,
    document.body,
  );
}
