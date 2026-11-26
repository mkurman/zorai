import { useEffect, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { latestPickedScreenshot } from "@/zorai/features/browser/domToCode";
import { latestSketchImage } from "@/zorai/features/threads/sketchExport";
import { joinVoiceTranscript } from "@/zorai/features/threads/voiceTranscript";
import { useCodeDictation } from "./useCodeDictation";

const ICON = { width: 16, height: 16, viewBox: "0 0 24 24", fill: "none", stroke: "currentColor", strokeWidth: 2, strokeLinecap: "round" as const, strokeLinejoin: "round" as const, "aria-hidden": true };
const MicIcon = () => <svg {...ICON}><path d="M12 1a3 3 0 0 0-3 3v8a3 3 0 0 0 6 0V4a3 3 0 0 0-3-3z" /><path d="M19 10v2a7 7 0 0 1-14 0v-2" /><line x1="12" y1="19" x2="12" y2="23" /><line x1="8" y1="23" x2="16" y2="23" /></svg>;
const BrainIcon = () => <svg {...ICON}><path d="M9 4a3 3 0 0 0-3 3 3 3 0 0 0-2 5 3 3 0 0 0 2 5 3 3 0 0 0 6 1V5a2 2 0 0 0-3-1z" /><path d="M15 4a3 3 0 0 1 3 3 3 3 0 0 1 2 5 3 3 0 0 1-2 5 3 3 0 0 1-6 1" /><path d="M12 9h-2M12 14h-2M12 9h2M12 14h2" /></svg>;
const REASONING_KEY = "zorai.code.rarogReasoning";

function readReasoningPreference(): boolean {
  try {
    return localStorage.getItem(REASONING_KEY) !== "off";
  } catch {
    return true;
  }
}

const SendIcon = () => <svg {...ICON}><line x1="12" y1="19" x2="12" y2="5" /><polyline points="5 12 12 5 19 12" /></svg>;

export function RarogFixPrompt({ startLine, endLine, diagnostics, busy, error, reviewing = false, thinking = "", onSubmit, onAccept, onClose, title, scope, placeholder, acceptLabel = "Accept", preview, onRun, allowImages = false }: {
  startLine: number;
  endLine: number;
  diagnostics: number;
  busy: boolean;
  error: string | null;
  onSubmit: (instruction: string, reasoning: boolean, images: string[]) => void;
  allowImages?: boolean;
  onAccept?: () => void;
  thinking?: string;
  onClose: () => void;
  reviewing?: boolean;
  title?: string;
  scope?: string;
  placeholder?: string;
  acceptLabel?: string;
  preview?: ReactNode;
  onRun?: () => void;
}) {
  const [instruction, setInstruction] = useState("");
  const [images, setImages] = useState<string[]>([]);
  const addImageFiles = (files: File[]) => {
    for (const file of files.filter((entry) => entry.type.startsWith("image/")).slice(0, 4)) {
      const reader = new FileReader();
      reader.onload = () => { if (typeof reader.result === "string") setImages((current) => [...current, reader.result as string].slice(-4)); };
      reader.readAsDataURL(file);
    }
  };
  const [reasoning, setReasoning] = useState(readReasoningPreference);
  const toggleReasoning = () => setReasoning((current) => {
    try {
      localStorage.setItem(REASONING_KEY, current ? "off" : "on");
    } catch {
      return !current;
    }
    return !current;
  });
  const inputRef = useRef<HTMLTextAreaElement | null>(null);
  const dictation = useCodeDictation((transcript) => setInstruction((current) => joinVoiceTranscript(current, transcript)));
  useEffect(() => { inputRef.current?.focus(); }, []);
  const lines = scope ?? (startLine === endLine ? `line ${startLine}` : `lines ${startLine}–${endLine}`);
  const fallback = diagnostics > 0 ? `fixes the ${diagnostics} problem${diagnostics === 1 ? "" : "s"} here` : "fixes the most likely bug";
  const submit = () => {
    if (busy || (reviewing && !instruction.trim())) return;
    onSubmit(instruction.trim(), reasoning, images);
    if (reviewing) setInstruction("");
  };
  const onKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    event.stopPropagation();
    if (event.key === "Escape") {
      event.preventDefault();
      onClose();
    } else if (event.key === "Enter" && (event.ctrlKey || event.metaKey) && event.shiftKey && reviewing && onRun) {
      event.preventDefault();
      onRun();
    } else if (event.key === "Enter" && (event.ctrlKey || event.metaKey) && reviewing) {
      event.preventDefault();
      onAccept?.();
    } else if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      submit();
    }
  };

  return <div className="zorai-rarog-fix" role="dialog" aria-label="Fix with Leszy" onKeyDown={(event) => event.stopPropagation()}>
    <header>
      <strong>{reviewing ? "Review Leszy's change" : title ?? "Fix with Leszy"}</strong>
      <span>{lines}</span>
      <button type="button" className="zorai-rarog-fix__close" aria-label={reviewing ? "Reject change" : "Close"} title={reviewing ? "Reject the change (Esc)" : "Close (Esc)"} onClick={onClose}>×</button>
    </header>
    <div className="zorai-rarog-fix__field">
      <div className="zorai-rarog-fix__field-textarea">
        <textarea
          ref={inputRef}
          rows={1}
          value={instruction}
          disabled={busy}
          placeholder={reviewing ? "Ask for changes to this edit…" : placeholder ?? `What should change? Leave empty and Leszy ${fallback}.`}
          aria-label="Fix instruction"
          onChange={(event) => setInstruction(event.target.value)}
          onKeyDown={onKeyDown}
          onPaste={allowImages ? (event) => {
            const files = Array.from(event.clipboardData.files);
            if (files.some((file) => file.type.startsWith("image/"))) {
              event.preventDefault();
              addImageFiles(files);
            }
          } : undefined}
          onDrop={allowImages ? (event) => {
            const files = Array.from(event.dataTransfer.files);
            if (files.length > 0) {
              event.preventDefault();
              addImageFiles(files);
            }
          } : undefined}
        />
      </div>
      {images.length > 0 ? (
        <div className="zorai-rarog-fix__images">
          {images.map((image, index) => (
            <span key={index}>
              <img src={image} alt={`Attached image ${index + 1}`} />
              <button type="button" aria-label={`Remove image ${index + 1}`} onClick={() => setImages((current) => current.filter((_, position) => position !== index))}>×</button>
            </span>
          ))}
        </div>
      ) : null}
      <div className="zorai-rarog-fix__field-buttons">
        <div className="zorai-rarog-fix__field-left-buttons">
          <button
            type="button"
            className={reasoning ? "zorai-rarog-fix__reasoning is-on" : "zorai-rarog-fix__reasoning"}
            aria-pressed={reasoning}
            disabled={busy}
            title={reasoning ? "Reasoning on: Leszy thinks before editing (click to turn off)" : "Reasoning off: faster, no thinking (click to turn on)"}
            aria-label={reasoning ? "Turn reasoning off" : "Turn reasoning on"}
            onClick={toggleReasoning}
          >
            <BrainIcon />
          </button>
          {allowImages && latestPickedScreenshot() ? (
            <button
              type="button"
              disabled={busy}
              title="Attach the last element you picked in the browser (paste or drop any screenshot or sketch too)"
              aria-label="Attach browser screenshot"
              onClick={() => { const shot = latestPickedScreenshot(); if (shot) setImages((current) => [...current, shot].slice(-4)); }}
            >⌖</button>
          ) : null}
          {allowImages && latestSketchImage() ? (
            <button
              type="button"
              disabled={busy}
              title="Attach your latest sketch from the composer so Leszy can turn it into code"
              aria-label="Attach latest sketch"
              onClick={() => { const sketch = latestSketchImage(); if (sketch) setImages((current) => [...current, sketch].slice(-4)); }}
            >✎</button>
          ) : null}
        </div>
        <div className="zorai-rarog-fix__field-right-buttons">
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
      </div>
    </div>
    {reviewing && !busy && preview ? <div className="zorai-rarog-fix__preview">{preview}</div> : null}
    {reviewing && !busy ? (
      <div className="zorai-rarog-fix__review">
        <button type="button" className="zorai-rarog-fix__accept" title="Keep the change (Ctrl+Enter)" onClick={onAccept}>{acceptLabel} <kbd>Ctrl+Enter</kbd></button>
        {onRun ? <button type="button" title="Insert and run (Ctrl+Shift+Enter)" onClick={onRun}>Run <kbd>Ctrl+Shift+Enter</kbd></button> : null}
        <button type="button" title="Discard the change and restore the original (Esc)" onClick={onClose}>Reject <kbd>Esc</kbd></button>
      </div>
    ) : null}
    {busy && thinking ? <p className="zorai-rarog-fix__thinking" title={thinking}><span>Leszy is thinking…</span> {thinking.slice(-220)}</p> : null}
    {busy && !thinking ? <p className="zorai-rarog-fix__status">Leszy is working on {lines}…</p> : error ? <p className="zorai-rarog-fix__error" role="alert">{error}</p> : null}
  </div>;
}
