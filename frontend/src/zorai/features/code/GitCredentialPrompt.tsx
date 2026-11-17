import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { getBridge } from "@/lib/bridge";
import { gitCredentialPromptKind, gitCredentialPromptTitle } from "./gitCredentialPromptModel";

type GitCredentialRequest = { id: string; prompt: string };

export function GitCredentialPrompt() {
  const [queue, setQueue] = useState<GitCredentialRequest[]>([]);

  useEffect(() => {
    const bridge = getBridge();
    const offRequest = bridge?.onGitAskpassRequest?.((request) => setQueue((current) => [...current, request]));
    const offCancel = bridge?.onGitAskpassCancel?.((id) => setQueue((current) => current.filter((item) => item.id !== id)));
    return () => {
      offRequest?.();
      offCancel?.();
    };
  }, []);

  const current = queue[0];
  if (!current) return null;
  const respond = (value: string | null) => {
    void getBridge()?.gitAskpassRespond?.(current.id, value);
    setQueue((items) => items.filter((item) => item.id !== current.id));
  };
  return createPortal(<GitCredentialDialog key={current.id} request={current} onRespond={respond} />, document.body);
}

function GitCredentialDialog({ request, onRespond }: { request: GitCredentialRequest; onRespond: (value: string | null) => void }) {
  const [value, setValue] = useState("");
  const inputRef = useRef<HTMLInputElement | null>(null);
  const confirmRef = useRef<HTMLButtonElement | null>(null);
  const kind = gitCredentialPromptKind(request.prompt);

  useEffect(() => {
    (kind === "confirm" ? confirmRef.current : inputRef.current)?.focus();
  }, [kind]);

  const cancel = () => onRespond(null);
  return (
    <div className="zorai-code-dialog-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget) cancel(); }}>
      <form
        className="zorai-code-close-dialog zorai-workspace-path-dialog zorai-git-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="zorai-git-credential-title"
        onKeyDown={(event) => { if (event.key === "Escape") cancel(); }}
        onSubmit={(event) => {
          event.preventDefault();
          onRespond(kind === "confirm" ? "yes" : value);
        }}
      >
        <h2 id="zorai-git-credential-title">{gitCredentialPromptTitle(request.prompt)}</h2>
        <p className="zorai-git-credential__prompt">{request.prompt.trim()}</p>
        {kind === "confirm" ? null : (
          <label>
            <span>{kind === "username" ? "Username" : "Used once for this command and never stored"}</span>
            <input
              ref={inputRef}
              type={kind === "username" ? "text" : "password"}
              value={value}
              autoComplete="off"
              spellCheck={false}
              onChange={(event) => setValue(event.target.value)}
            />
          </label>
        )}
        <footer>
          <button type="button" onClick={cancel}>Cancel</button>
          <button ref={confirmRef} type="submit" className="is-primary">{kind === "confirm" ? "Trust host" : "Continue"}</button>
        </footer>
      </form>
    </div>
  );
}
