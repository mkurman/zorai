import { useEffect, useState } from "react";
import { getBridge } from "@/lib/bridge";
import { Panel, SettingRow } from "@/zorai/features/settings/settingsLayout";
import { REALTIME_PROVIDER_LABELS, REALTIME_VOICES, realtimeModelOptions } from "./realtimeCatalog";
import { REALTIME_DEFAULTS, useVoiceModeStore, type RealtimeProvider, type VoiceEngineMode } from "./voiceModeStore";

const CUSTOM = "__custom__";

function PickerWithCustom({ value, options, onChange, loading, hint }: { value: string; options: string[]; onChange: (value: string) => void; loading?: boolean; hint?: string | null }) {
  const [custom, setCustom] = useState(false);
  const known = options.includes(value);
  return <span className="zorai-realtime-picker">
    {custom || !known ? (
      <input className="zorai-input" value={value} autoFocus={custom} placeholder="Custom id" onChange={(event) => onChange(event.target.value)} onBlur={() => setCustom(false)} />
    ) : (
      <select className="zorai-input" value={value} onChange={(event) => { if (event.target.value === CUSTOM) setCustom(true); else onChange(event.target.value); }}>
        {options.map((option) => <option key={option} value={option}>{option}</option>)}
        <option value={CUSTOM}>Custom…</option>
      </select>
    )}
    {loading ? <small>Loading models…</small> : hint ? <small>{hint}</small> : null}
  </span>;
}

export function VoiceModeSettingsPanel() {
  const settings = useVoiceModeStore();
  const realtime = settings.engine === "realtime";
  const [fetched, setFetched] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);
  const [fetchError, setFetchError] = useState<string | null>(null);

  useEffect(() => {
    const fetchModels = getBridge()?.agentFetchModels;
    if (!realtime || !fetchModels) return;
    let cancelled = false;
    setLoading(true);
    setFetchError(null);
    setFetched([]);
    void fetchModels(settings.realtimeProvider, "", "").then((result) => {
      if (cancelled) return;
      const data = result as { models?: Array<{ id: string }>; error?: string } | null;
      if (Array.isArray(data?.models)) setFetched(data.models.map((model) => model.id));
      else if (data?.error) setFetchError("Couldn't list models; showing known ones.");
    }).catch(() => { if (!cancelled) setFetchError("Couldn't list models; showing known ones."); }).finally(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, [realtime, settings.realtimeProvider]);

  return (
    <Panel section="Audio" title="Voice conversation">
      <SettingRow label="Engine" description="Bundled chains your speech-to-text, the thread's agent and text-to-speech, turn by turn. Realtime streams speech to a speech-to-speech model that can hand requests to your agent.">
        <select className="zorai-input" value={settings.engine} onChange={(event) => settings.update({ engine: event.target.value as VoiceEngineMode })}>
          <option value="bundled">Bundled (STT → agent → TTS)</option>
          <option value="realtime">Realtime (speech-to-speech)</option>
        </select>
      </SettingRow>
      <SettingRow label="Interrupt by talking" description="Speaking while the assistant talks stops playback and starts listening. Turn off on speakers if the assistant interrupts itself.">
        <button type="button" className={["zorai-switch", settings.bargeIn ? "zorai-switch--on" : ""].filter(Boolean).join(" ")} aria-pressed={settings.bargeIn} onClick={() => settings.update({ bargeIn: !settings.bargeIn })}><span /></button>
      </SettingRow>
      {realtime ? (
        <>
          <SettingRow label="Realtime Provider" description="Uses the API key saved in Providers; subscription logins are not accepted by realtime APIs.">
            <select className="zorai-input" value={settings.realtimeProvider} onChange={(event) => {
              const provider = event.target.value as RealtimeProvider;
              settings.update({ realtimeProvider: provider, realtimeModel: REALTIME_DEFAULTS[provider].model, realtimeVoice: REALTIME_DEFAULTS[provider].voice });
            }}>
              {(Object.keys(REALTIME_PROVIDER_LABELS) as RealtimeProvider[]).map((provider) => <option key={provider} value={provider}>{REALTIME_PROVIDER_LABELS[provider]}</option>)}
            </select>
          </SettingRow>
          <SettingRow label="Realtime Model" description="Speech-to-speech models from the provider, plus known defaults.">
            <PickerWithCustom value={settings.realtimeModel} options={realtimeModelOptions(settings.realtimeProvider, fetched, "")} loading={loading} hint={fetchError} onChange={(realtimeModel) => settings.update({ realtimeModel })} />
          </SettingRow>
          <SettingRow label="Realtime Voice" description="Voice used by the realtime model.">
            <PickerWithCustom value={settings.realtimeVoice} options={REALTIME_VOICES[settings.realtimeProvider]} onChange={(realtimeVoice) => settings.update({ realtimeVoice })} />
          </SettingRow>
        </>
      ) : null}
    </Panel>
  );
}
