import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { joinVoiceTranscript } from "@/zorai/features/threads/voiceTranscript";
import { useCodeDictation } from "./useCodeDictation";

const ICON = { width: 16, height: 16, viewBox: "0 0 24 24", fill: "none", stroke: "currentColor", strokeWidth: 2, strokeLinecap: "round" as const, strokeLinejoin: "round" as const, "aria-hidden": true };
const MicIcon = () => <svg {...ICON}><path d="M12 1a3 3 0 0 0-3 3v8a3 3 0 0 0 6 0V4a3 3 0 0 0-3-3z" /><path d="M19 10v2a7 7 0 0 1-14 0v-2" /><line x1="12" y1="19" x2="12" y2="23" /><line x1="8" y1="23" x2="16" y2="23" /></svg>;
const SendIcon = () => <svg {...ICON}><line x1="12" y1="19" x2="12" y2="5" /><polyline points="5 12 12 5 19 12" /></svg>;

export function RarogFixPrompt({ startLine, endLine, diagnostics, busy, error, onSubmit, onClose }: {
  startLine: number;
  endLine: number;
  diagnostics: number;
  busy: boolean;
  error: string | null;
  onSubmit: (instruction: string) => void;
  onClose: () => void;
}) {
  const [instruction, setInstruction] = useState("");
  const inputRef = useRef<HTMLTextAreaElement | null>(null);
  const dictation = useCodeDictation((transcript) => setInstruction((current) => joinVoiceTranscript(current, transcript)));
  useEffect(() => { inputRef.current?.focus(); }, []);
  const lines = startLine === endLine ? `line ${startLine}` : `lines ${startLine}–${endLine}`;
  const fallback = diagnostics > 0 ? `fixes the ${diagnostics} problem${diagnostics === 1 ? "" : "s"} here` : "fixes the most likely bug";
  const submit = () => { if (!busy) onSubmit(instruction.trim()); };
  const onKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    event.stopPropagation();
    if (event.key === "Escape") {
      event.preventDefault();
      onClose();
    } else if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      submit();
    }
  };

  return <div className="zorai-rarog-fix" role="dialog" aria-label="Fix with Rarog" onKeyDown={(event) => event.stopPropagation()}>
    <header>
      <strong>Fix with Rarog</strong>
      <span>{lines}</span>
      <button type="button" className="zorai-rarog-fix__close" aria-label="Close" title="Close (Esc)" onClick={onClose}>×</button>
    </header>
    <div className="zorai-rarog-fix__field">
      <textarea
        ref={inputRef}
        rows={1}
        value={instruction}
        disabled={busy}
        placeholder={`What should change? Leave empty and Rarog ${fallback}.`}
        aria-label="Fix instruction"
        onChange={(event) => setInstruction(event.target.value)}
        onKeyDown={onKeyDown}
      />
      {dictation.available ? (
        <button
          type="button"
          className={dictation.recording ? "is-recording" : undefined}
          disabled={busy || dictation.transcribing}
          title={dictation.recording ? "Stop dictation" : dictation.transcribing ? "Transcribing…" : "Dictate"}
          aria-label={dictation.recording ? "Stop dictation" : "Dictate"}
          onClick={() => void dictation.toggle()}
        >
          <MicIcon />
        </button>
      ) : null}
      <button type="button" className="zorai-rarog-fix__send" disabled={busy || dictation.recording} title="Send (Enter)" aria-label="Send" onClick={submit}>
        <SendIcon />
      </button>
    </div>
    {busy ? <p className="zorai-rarog-fix__status">Rarog is working on {lines}…</p> : error ? <p className="zorai-rarog-fix__error" role="alert">{error}</p> : null}
  </div>;
}
