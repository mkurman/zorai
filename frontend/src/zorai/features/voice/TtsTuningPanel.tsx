import { useAgentStore } from "@/lib/agentStore";
import { Panel, SettingRow } from "@/zorai/features/settings/settingsLayout";
import { TTS_EMOTIONS, useTtsTuningStore, type TtsSoundEffect, type TtsTuning } from "./ttsTuningStore";

const EFFECTS: Array<[TtsSoundEffect, string]> = [["spacious_echo", "Spacious echo"], ["auditorium_echo", "Auditorium echo"], ["lofi_telephone", "Lo-fi telephone"], ["robotic", "Robotic"]];

function Slider({ value, min, max, step, onChange, format = String }: { value: number; min: number; max: number; step: number; onChange: (value: number) => void; format?: (value: number) => string }) {
  return <span className="zorai-tts-slider">
    <input type="range" min={min} max={max} step={step} value={value} onChange={(event) => onChange(Number(event.target.value))} />
    <output>{format(value)}</output>
  </span>;
}

function Polar({ left, right, value, onChange }: { left: string; right: string; value: number; onChange: (value: number) => void }) {
  return <span className="zorai-tts-polar">
    <span>{left}</span>
    <input type="range" min={-100} max={100} step={1} value={value} onChange={(event) => onChange(Number(event.target.value))} aria-label={`${left} to ${right}`} />
    <span>{right}</span>
    <output>{value > 0 ? `+${value}` : value}</output>
  </span>;
}

export function TtsTuningPanel() {
  const provider = useAgentStore((state) => state.agentSettings.audio_tts_provider) ?? "";
  const tuning = useTtsTuningStore();
  const minimax = /minimax/i.test(provider);
  const set = (patch: Partial<TtsTuning>) => tuning.update(patch);
  return (
    <Panel section="Audio" title="Voice tuning">
      <SettingRow label="Speed" description={minimax ? "Speaking rate, 0.5–2×." : "Speaking rate, 0.25–4×. Other tuning options are available with MiniMax voices."}>
        <Slider value={tuning.speed} min={minimax ? 0.5 : 0.25} max={minimax ? 2 : 4} step={0.05} onChange={(speed) => set({ speed })} format={(value) => `${value.toFixed(2)}×`} />
      </SettingRow>
      {minimax ? <>
        <SettingRow label="Volume" description="Output loudness, 0.1–10.">
          <Slider value={tuning.volume} min={0.1} max={10} step={0.1} onChange={(volume) => set({ volume })} format={(value) => value.toFixed(1)} />
        </SettingRow>
        <SettingRow label="Pitch" description="Semitones, −12 to +12.">
          <Slider value={tuning.pitch} min={-12} max={12} step={1} onChange={(pitch) => set({ pitch })} format={(value) => (value > 0 ? `+${value}` : String(value))} />
        </SettingRow>
        <SettingRow label="Emotion" description="Leave on Auto to let the model pick from the text. Whisper is not supported by speech-2.8 models.">
          <select className="zorai-input" value={tuning.emotion} onChange={(event) => set({ emotion: event.target.value })}>
            <option value="">Auto</option>
            {TTS_EMOTIONS.map((emotion) => <option key={emotion} value={emotion}>{emotion}</option>)}
          </select>
        </SettingRow>
        <SettingRow label="Voice modifier" description="Reshape the voice's character.">
          <span className="zorai-tts-modifier">
            <Polar left="Deepen" right="Lighten" value={tuning.modifyPitch} onChange={(modifyPitch) => set({ modifyPitch })} />
            <Polar left="Stronger" right="Softer" value={tuning.modifyIntensity} onChange={(modifyIntensity) => set({ modifyIntensity })} />
            <Polar left="Nasal" right="Crisp" value={tuning.modifyTimbre} onChange={(modifyTimbre) => set({ modifyTimbre })} />
          </span>
        </SettingRow>
        <SettingRow label="Sound effect" description="One effect at a time.">
          <select className="zorai-input" value={tuning.soundEffect} onChange={(event) => set({ soundEffect: event.target.value as TtsSoundEffect })}>
            <option value="">None</option>
            {EFFECTS.map(([value, label]) => <option key={value} value={value}>{label}</option>)}
          </select>
        </SettingRow>
      </> : null}
      <SettingRow label="Reset" description="Restore the provider's default voice settings.">
        <button type="button" className="zorai-ghost-button" onClick={tuning.reset}>Reset tuning</button>
      </SettingRow>
    </Panel>
  );
}
