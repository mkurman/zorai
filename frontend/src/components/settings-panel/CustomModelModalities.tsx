import { getBridge } from "@/lib/bridge";
import { useAgentStore, type Modality } from "@/lib/agentStore";

const OPTIONS: Array<{ modality: Modality; label: string }> = [
    { modality: "text", label: "Text" },
    { modality: "image", label: "Image" },
    { modality: "video", label: "Video" },
    { modality: "audio", label: "Audio" },
];

export function modelModalityKey(providerId: string, modelId: string): string {
    return `${providerId}::${modelId}`;
}

export function nextModelModalities(
    current: Record<string, Modality[]>,
    key: string,
    modality: Modality,
    enabled: boolean,
): Record<string, Modality[]> {
    const selected = new Set(current[key] ?? ["text"]);
    if (enabled) selected.add(modality);
    else selected.delete(modality);
    const list = OPTIONS.map((option) => option.modality).filter((item) => item === "text" || selected.has(item));
    const next = { ...current };
    if (list.length > 1) next[key] = list;
    else delete next[key];
    return next;
}

export function CustomModelModalities({ providerId, modelId, disabled }: { providerId: string; modelId: string; disabled?: boolean }) {
    const key = modelModalityKey(providerId, modelId);
    const selected = useAgentStore((state) => state.agentSettings.model_modalities?.[key]) ?? ["text"];
    const toggle = (modality: Modality, enabled: boolean) => {
        const next = nextModelModalities(useAgentStore.getState().agentSettings.model_modalities ?? {}, key, modality, enabled);
        useAgentStore.setState((state) => ({ agentSettings: { ...state.agentSettings, model_modalities: next } }));
        void getBridge()?.agentSetConfigItem?.("/model_modalities", next);
    };
    return (
        <div className="zorai-model-modalities" role="group" aria-label={`Inputs ${modelId} accepts`}>
            <span className="zorai-model-modalities__label">Accepts</span>
            {OPTIONS.map(({ modality, label }) => (
                <label key={modality} className="zorai-model-modalities__option">
                    <input
                        type="checkbox"
                        checked={modality === "text" || selected.includes(modality)}
                        disabled={disabled || modality === "text"}
                        onChange={(event) => toggle(modality, event.target.checked)}
                    />
                    {label}
                </label>
            ))}
        </div>
    );
}
