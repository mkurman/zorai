import { useEffect, useState } from "react";
import { useSettingsStore } from "@/lib/settingsStore";
import { shortcutCombo, shortcutMatches } from "@/zorai/features/shortcuts/appShortcuts";
import { useBundledVoice, type VoiceRuntime } from "./useBundledVoice";
import { useRealtimeVoice } from "./useRealtimeVoice";
import { VoiceConversationOverlay } from "./VoiceConversationOverlay";
import { useVoiceModeStore } from "./voiceModeStore";
import { REALTIME_PROVIDER_LABELS } from "./realtimeCatalog";

let activeVoiceSessions = 0;

export function VoiceConversationButton({ runtime }: { runtime: VoiceRuntime }) {
  const [active, setActive] = useState(false);
  const [minimized, setMinimized] = useState(false);
  const combo = useSettingsStore((state) => shortcutCombo(state.settings.shortcuts, "voice.toggleMinimized"));
  const start = () => { setMinimized(false); setActive(true); };

  useEffect(() => {
    if (!active) return;
    activeVoiceSessions += 1;
    return () => { activeVoiceSessions -= 1; };
  }, [active]);

  useEffect(() => {
    if (active) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.defaultPrevented || activeVoiceSessions > 0 || !shortcutMatches("voice.toggleMinimized", event)) return;
      event.preventDefault();
      start();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [active]);
  const settings = useVoiceModeStore();
  const realtime = settings.engine === "realtime";
  const bundled = useBundledVoice(runtime, active && !realtime, settings.bargeIn);
  const live = useRealtimeVoice(settings, active && realtime, runtime);
  const engine = realtime ? live : bundled;
  const engineLabel = realtime ? `Realtime · ${REALTIME_PROVIDER_LABELS[settings.realtimeProvider]} · ${settings.realtimeModel} · ${settings.realtimeVoice}` : "Voice · speech-to-text → agent → text-to-speech";
  return <>
    <button type="button" className="zorai-composer-icon-button" title={`Start voice conversation${combo ? ` (${combo})` : ""}`} aria-label="Start voice conversation" onClick={start}>
      <svg className="zorai-voice-start-icon" viewBox="0 0 24 24" aria-hidden="true"><path d="M4 10v4M8 7v10M12 4v16M16 7v10M20 10v4" /></svg>
    </button>
    {active ? <VoiceConversationOverlay session={engine.session} engineLabel={engineLabel} minimized={minimized} onMinimize={() => setMinimized(true)} onExpand={() => setMinimized(false)} onToggleMute={engine.toggleMute} onHangUp={() => { setActive(false); setMinimized(false); }} /> : null}
  </>;
}
