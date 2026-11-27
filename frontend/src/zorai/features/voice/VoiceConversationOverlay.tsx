import { useEffect, useRef } from "react";
import { createPortal } from "react-dom";
import type { VoiceSession } from "./useBundledVoice";
import { captionWords, speechText, type VoiceStatus } from "./voiceConversation";
import { revealScrollDelta } from "@/zorai/features/code/WorkspaceExplorerTree";
import { shortcutCombo, shortcutMatches } from "@/zorai/features/shortcuts/appShortcuts";
import { useSettingsStore } from "@/lib/settingsStore";

const STATUS_LABEL: Record<VoiceStatus, string> = {
  idle: "",
  connecting: "Connecting…",
  listening: "Listening",
  transcribing: "Got it…",
  thinking: "Thinking…",
  speaking: "Speaking — talk to interrupt",
  error: "Something went wrong",
};

function isTypingTarget(target: EventTarget | null): boolean {
  const element = target as HTMLElement | null;
  return Boolean(element && (element.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(element.tagName) || element.closest(".monaco-editor")));
}

const MicIcon = ({ muted }: { muted: boolean }) => <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 3a3 3 0 0 0-3 3v6a3 3 0 0 0 6 0V6a3 3 0 0 0-3-3zM5 11a7 7 0 0 0 14 0M12 18v3" />{muted ? <path d="M4 4l16 16" /> : null}</svg>;
const HangUpIcon = () => <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M6 6l12 12M18 6L6 18" /></svg>;
const ThreadIcon = () => <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 5h16v11H9l-5 4zM8 9h8M8 12h5" /></svg>;
const ExpandIcon = () => <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M14 4h6v6M10 20H4v-6M20 4l-7 7M4 20l7-7" /></svg>;

export function VoiceConversationOverlay({ session, engineLabel, minimized, onMinimize, onExpand, onToggleMute, onHangUp }: { session: VoiceSession; engineLabel: string; minimized: boolean; onMinimize: () => void; onExpand: () => void; onToggleMute: () => void; onHangUp: () => void }) {
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (shortcutMatches("voice.toggleMinimized", event)) {
        event.preventDefault();
        event.stopPropagation();
        if (minimized) onExpand(); else onMinimize();
        return;
      }
      if (minimized && isTypingTarget(event.target)) return;
      if (event.key === "Escape" && !minimized) onHangUp();
      if ((event.key === "m" || event.key === "M") && !event.ctrlKey && !event.metaKey && !event.altKey && !event.repeat) {
        event.preventDefault();
        onToggleMute();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [minimized, onExpand, onHangUp, onMinimize, onToggleMute]);
  const toggleCombo = useSettingsStore((state) => shortcutCombo(state.settings.shortcuts, "voice.toggleMinimized"));
  const toggleHint = toggleCombo ? ` (${toggleCombo})` : "";

  const scale = 1 + Math.min(0.45, session.level * 4);
  const label = session.muted && session.status === "listening" ? "Muted" : session.status === "thinking" && session.activity ? session.activity : STATUS_LABEL[session.status];
  const orbClass = `zorai-voice__orb is-${session.status}${session.muted ? " is-muted" : ""}`;
  if (minimized) {
    return createPortal(
      <div className="zorai-voice-mini" role="region" aria-label="Voice conversation (minimized)">
        <button type="button" className="zorai-voice-mini__orb-button" title={`Expand voice conversation${toggleHint}`} aria-label="Expand voice conversation" onClick={onExpand}>
          <span className={orbClass} style={{ transform: `scale(${scale})` }} aria-hidden="true" />
        </button>
        <span className="zorai-voice-mini__status" aria-live="polite">{label}</span>
        <button type="button" className={session.muted ? "is-on" : undefined} aria-pressed={session.muted} title={session.muted ? "Unmute microphone (M)" : "Mute microphone (M)"} aria-label={session.muted ? "Unmute microphone" : "Mute microphone"} onClick={onToggleMute}><MicIcon muted={session.muted} /></button>
        <button type="button" title={`Expand${toggleHint}`} aria-label="Expand voice conversation" onClick={onExpand}><ExpandIcon /></button>
        <button type="button" className="is-hangup" title="End voice conversation" aria-label="End voice conversation" onClick={onHangUp}><HangUpIcon /></button>
      </div>,
      document.body,
    );
  }
  return createPortal(
    <div className="zorai-voice" role="dialog" aria-modal="true" aria-label="Voice conversation">
      <div className="zorai-voice__engine">{engineLabel}</div>
      <div className={orbClass} style={{ transform: `scale(${scale})` }} aria-hidden="true" />
      <div className="zorai-voice__status" aria-live="polite">{label}</div>
      <div className="zorai-voice__captions">
        {session.userText ? <p className="zorai-voice__you">{session.userText}</p> : null}
        {session.replyText ? <SpokenCaption text={speechText(session.replyText)} progress={session.progress} /> : null}
        {session.error ? <p className="zorai-voice__error" role="alert">{session.error}</p> : null}
      </div>
      <div className="zorai-voice__controls">
        <button type="button" className={session.muted ? "is-on" : undefined} aria-pressed={session.muted} title={session.muted ? "Unmute microphone (M)" : "Mute microphone (M)"} aria-label={session.muted ? "Unmute microphone" : "Mute microphone"} onClick={onToggleMute}>
          <MicIcon muted={session.muted} />
        </button>
        <button type="button" title={`See the thread — keeps talking in a small orb${toggleHint}`} aria-label="See thread" onClick={onMinimize}>
          <ThreadIcon />
        </button>
        <button type="button" className="is-hangup" title="End voice conversation (Esc)" aria-label="End voice conversation" onClick={onHangUp}>
          <HangUpIcon />
        </button>
      </div>
    </div>,
    document.body,
  );
}

function SpokenCaption({ text, progress }: { text: string; progress: number | undefined }) {
  const { words, spoken } = captionWords(text, progress);
  const currentRef = useRef<HTMLSpanElement | null>(null);
  useEffect(() => {
    const word = currentRef.current;
    const box = word?.closest<HTMLElement>(".zorai-voice__reply");
    if (word && box) box.scrollTop += revealScrollDelta(word.getBoundingClientRect(), box.getBoundingClientRect());
  }, [spoken]);
  return <p className="zorai-voice__reply">
    {words.map((word, index) => <span key={index} ref={index === Math.max(0, spoken - 1) ? currentRef : undefined} className={index < spoken ? "is-spoken" : undefined}>{word} </span>)}
  </p>;
}
