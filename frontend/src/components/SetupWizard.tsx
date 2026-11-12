import { useEffect, useMemo, useState } from "react";
import { getBridge } from "@/lib/bridge";
import {
  GATEWAY_CHOICES,
  SECURITY_LEVELS,
  SETUP_TIERS,
  WEB_SEARCH_CHOICES,
  defaultSecurityLevel,
  filterSetupModels,
  gatewayWrites,
  isLocalProvider,
  providerAuthWrites,
  webSearchWrites,
  type SetupConfigWrite,
  type SetupProvider,
  type SetupTier,
} from "./setupWizardPlan";

const STEPS = ["Experience", "Provider", "Authentication", "Model", "Safety", "Web search", "Chat gateway", "Done"] as const;

type Validation = { state: "idle" | "testing" | "ok" | "failed"; message?: string };

async function applyWrites(writes: SetupConfigWrite[], dryRun = false) {
  if (dryRun) return;
  const bridge = getBridge();
  for (const write of writes) await bridge?.agentSetConfigItem?.(write.keyPath, write.value);
}

function Choice({ selected, label, onSelect }: { selected: boolean; label: string; onSelect: () => void }) {
  return <button type="button" className={["zorai-onboarding-button", selected ? "zorai-onboarding-button--selected" : ""].filter(Boolean).join(" ")} aria-pressed={selected} onClick={onSelect}>{label}</button>;
}

export function SetupWizard({ dataDir, onClose, dryRun = false }: { dataDir?: string; onClose: () => void; dryRun?: boolean }) {
  const [step, setStep] = useState(0);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [tier, setTier] = useState<SetupTier>("newcomer");
  const [providers, setProviders] = useState<SetupProvider[]>([]);
  const [providerQuery, setProviderQuery] = useState("");
  const [providerId, setProviderId] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [models, setModels] = useState<Array<{ id: string; name?: string }>>([]);
  const [modelsError, setModelsError] = useState<string | null>(null);
  const [modelQuery, setModelQuery] = useState("");
  const [model, setModel] = useState("");
  const [validation, setValidation] = useState<Validation>({ state: "idle" });
  const [security, setSecurity] = useState<string>(defaultSecurityLevel("newcomer"));
  const [webSearch, setWebSearch] = useState("skip");
  const [webSearchKey, setWebSearchKey] = useState("");
  const [gateway, setGateway] = useState("skip");
  const [gatewayToken, setGatewayToken] = useState("");

  const provider = providers.find((item) => item.provider_id === providerId) ?? null;
  const visibleProviders = useMemo(() => {
    const query = providerQuery.trim().toLowerCase();
    return query ? providers.filter((item) => `${item.provider_name} ${item.provider_id}`.toLowerCase().includes(query)) : providers;
  }, [providerQuery, providers]);
  const visibleModels = useMemo(() => filterSetupModels(models, modelQuery, provider?.model ?? ""), [modelQuery, models, provider?.model]);
  const webChoice = WEB_SEARCH_CHOICES.find((item) => item.id === webSearch) ?? null;

  useEffect(() => {
    void getBridge()?.agentGetProviderAuthStates?.().then((states) => {
      const list = Array.isArray(states) ? states as SetupProvider[] : [];
      setProviders(list);
      setProviderId((current) => current || list.find((item) => item.authenticated)?.provider_id || list[0]?.provider_id || "");
    });
  }, []);

  const loadModels = async (target: SetupProvider, key: string) => {
    setModels([]);
    setModelsError(null);
    const result = await getBridge()?.agentFetchModels?.(target.provider_id, target.base_url, key) as { models?: Array<{ id: string; name?: string }>; error?: string } | undefined;
    if (result?.error) setModelsError(result.error);
    setModels(result?.models ?? []);
    setModelQuery("");
  };

  const validate = async (target: SetupProvider, key: string, authSource: string) => {
    setValidation({ state: "testing" });
    const result = await getBridge()?.agentValidateProvider?.(target.provider_id, target.base_url, key, authSource);
    setValidation(result?.valid ? { state: "ok" } : { state: "failed", message: result?.error || "The provider rejected the credentials." });
  };

  const next = async () => {
    setBusy(true);
    setError(null);
    try {
      if (step === 0) {
        if (!dryRun) await getBridge()?.agentSetTierOverride?.(tier);
        setSecurity(defaultSecurityLevel(tier));
      } else if (step === 2 && provider) {
        await applyWrites(providerAuthWrites(provider, apiKey), dryRun);
        const key = apiKey.trim();
        const authSource = key ? "api_key" : provider.auth_source;
        setModel(provider.model);
        void loadModels(provider, key);
        if (!isLocalProvider(provider.provider_id) && (provider.authenticated || key)) void validate(provider, key, authSource);
        else setValidation({ state: "idle" });
      } else if (step === 3 && model && model !== provider?.model) {
        await applyWrites([{ keyPath: "/model", value: model }], dryRun);
      } else if (step === 4) {
        await applyWrites([{ keyPath: "/managed_execution/security_level", value: security }], dryRun);
      } else if (step === 5 && webSearch !== "skip") {
        const writes = webSearchWrites(webSearch, webSearchKey);
        if (!writes) throw new Error(`${webChoice?.label ?? "This provider"} needs an API key.`);
        await applyWrites(writes, dryRun);
      } else if (step === 6 && gateway !== "skip") {
        const writes = gatewayWrites(gateway, gatewayToken);
        if (writes.length === 0) throw new Error("Enter a bot token, or choose Skip.");
        await applyWrites(writes, dryRun);
      }
      setStep((value) => Math.min(value + 1, STEPS.length - 1));
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBusy(false);
    }
  };

  const canContinue = step !== 1 || Boolean(provider);
  const progress = Math.round((step / (STEPS.length - 1)) * 100);

  return <>
    <div className="zorai-onboarding-progress">
      <div className="zorai-onboarding-progress__meta"><span>{STEPS[step]}</span><span>Step {step + 1} of {STEPS.length}</span></div>
      <div className="zorai-onboarding-progress__track"><div className="zorai-onboarding-progress__bar" style={{ width: `${progress}%` }} /></div>
    </div>

    <div className="zorai-onboarding-section">
      {step === 0 ? <>
        <div className="zorai-onboarding-section__title">How familiar are you with AI agents?</div>
        <div className="zorai-setup-choices">{SETUP_TIERS.map((item) => <Choice key={item.id} selected={tier === item.id} label={item.label} onSelect={() => setTier(item.id)} />)}</div>
      </> : null}

      {step === 1 ? <>
        <div className="zorai-onboarding-section__title">Select your LLM provider</div>
        <input className="zorai-onboarding-input" type="search" placeholder="Search providers" aria-label="Search providers" value={providerQuery} onChange={(event) => setProviderQuery(event.target.value)} />
        {providers.length === 0 ? <div className="zorai-onboarding-copy">Waiting for the daemon to list providers…</div> : null}
        <div className="zorai-setup-choices zorai-setup-choices--scroll">
          {visibleProviders.map((item) => <Choice key={item.provider_id} selected={providerId === item.provider_id} label={item.authenticated ? `${item.provider_name} · connected` : item.provider_name} onSelect={() => { setProviderId(item.provider_id); setApiKey(""); }} />)}
        </div>
      </> : null}

      {step === 2 && provider ? <>
        <div className="zorai-onboarding-section__title">Authenticate {provider.provider_name}</div>
        {isLocalProvider(provider.provider_id) ? <div className="zorai-onboarding-copy">Local provider — no API key needed.</div> : <>
          {provider.authenticated ? <div className="zorai-onboarding-copy">{provider.provider_name} is already authenticated. Leave the field empty to keep the existing credentials.</div> : null}
          {provider.provider_id === "github-copilot" ? <div className="zorai-onboarding-copy zorai-onboarding-copy--sm">To sign in with GitHub in the browser instead, finish setup and use Settings → Auth.</div> : null}
          <input className="zorai-onboarding-input" type="password" autoFocus placeholder={provider.authenticated ? "New API key (optional)" : `${provider.provider_name} API key`} aria-label={`${provider.provider_name} API key`} value={apiKey} onChange={(event) => setApiKey(event.target.value)} />
          {!provider.authenticated && !apiKey.trim() ? <div className="zorai-onboarding-copy zorai-onboarding-copy--sm">No key yet? You can continue and add it later in Settings → Auth.</div> : null}
        </>}
      </> : null}

      {step === 3 && provider ? <>
        <div className="zorai-onboarding-section__title">Default model for Svarog</div>
        {validation.state === "testing" ? <div className="zorai-onboarding-copy">Testing connection to {provider.provider_name}…</div> : null}
        {validation.state === "ok" ? <div className="zorai-onboarding-copy">Connection successful.</div> : null}
        {validation.state === "failed" ? <div className="zorai-onboarding-error">Connection test failed: {validation.message}</div> : null}
        {modelsError ? <div className="zorai-onboarding-copy zorai-onboarding-copy--sm">Could not fetch models: {modelsError}</div> : null}
        {models.length > 0 ? <input className="zorai-onboarding-input" type="search" placeholder="Search models" aria-label="Search models" value={modelQuery} onChange={(event) => setModelQuery(event.target.value)} /> : null}
        <div className="zorai-setup-choices zorai-setup-choices--scroll">
          <Choice selected={model === provider.model} label={`Keep default${provider.model ? ` (${provider.model})` : ""}`} onSelect={() => setModel(provider.model)} />
          {visibleModels.map((item) => <Choice key={item.id} selected={model === item.id} label={item.name && item.name !== item.id ? `${item.name} · ${item.id}` : item.id} onSelect={() => setModel(item.id)} />)}
        </div>
      </> : null}

      {step === 4 ? <>
        <div className="zorai-onboarding-section__title">How cautious should Zorai be?</div>
        <div className="zorai-setup-choices">{SECURITY_LEVELS.map((item) => <Choice key={item.id} selected={security === item.id} label={item.label} onSelect={() => setSecurity(item.id)} />)}</div>
      </> : null}

      {step === 5 ? <>
        <div className="zorai-onboarding-section__title">Configure web search? (enables agent web browsing)</div>
        <div className="zorai-setup-choices">
          {WEB_SEARCH_CHOICES.map((item) => <Choice key={item.id} selected={webSearch === item.id} label={item.label} onSelect={() => { setWebSearch(item.id); setWebSearchKey(""); }} />)}
          <Choice selected={webSearch === "skip"} label="Skip" onSelect={() => setWebSearch("skip")} />
        </div>
        {webChoice?.keyName ? <input className="zorai-onboarding-input" type="password" placeholder={webChoice.keyRequired ? `${webChoice.label} API key` : `${webChoice.label} API key (optional, leave empty for the free public endpoint)`} aria-label={`${webChoice.label} API key`} value={webSearchKey} onChange={(event) => setWebSearchKey(event.target.value)} /> : null}
      </> : null}

      {step === 6 ? <>
        <div className="zorai-onboarding-section__title">Connect a chat gateway?</div>
        <div className="zorai-setup-choices">
          {GATEWAY_CHOICES.map((item) => <Choice key={item.id} selected={gateway === item.id} label={item.label} onSelect={() => { setGateway(item.id); setGatewayToken(""); }} />)}
          <Choice selected={gateway === "skip"} label="Skip" onSelect={() => setGateway("skip")} />
        </div>
        {gateway !== "skip" ? <input className="zorai-onboarding-input" type="password" placeholder={`${GATEWAY_CHOICES.find((item) => item.id === gateway)?.label} bot token`} aria-label="Gateway token" value={gatewayToken} onChange={(event) => setGatewayToken(event.target.value)} /> : null}
        <div className="zorai-onboarding-copy zorai-onboarding-copy--sm">WhatsApp needs a QR link — set it up in Settings → Gateway after setup.</div>
      </> : null}

      {step === 7 ? <>
        <div className="zorai-onboarding-section__title">You're set up</div>
        <div className="zorai-onboarding-copy">
          Provider: {provider?.provider_name ?? "unchanged"}{model ? ` · ${model}` : ""}<br />
          Safety: {SECURITY_LEVELS.find((item) => item.id === security)?.label}<br />
          Web search: {webChoice?.label ?? "not configured"}<br />
          Gateway: {gateway === "skip" ? "not configured" : GATEWAY_CHOICES.find((item) => item.id === gateway)?.label}
          {dataDir ? <><br />Data stored at: <code>{dataDir}</code></> : null}
        </div>
        <div className="zorai-onboarding-copy zorai-onboarding-copy--sm">Everything here can be changed later in Settings.</div>
      </> : null}
    </div>

    {error ? <div className="zorai-onboarding-error">{error}</div> : null}

    <div className="zorai-onboarding-actions zorai-onboarding-actions--split">
      <div className="zorai-onboarding-actions__group">
        {dryRun && step < STEPS.length - 1 ? <button type="button" className="zorai-onboarding-button" onClick={onClose}>Close test</button> : null}
        {step > 0 && step < STEPS.length - 1 ? <button type="button" className="zorai-onboarding-button" disabled={busy} onClick={() => setStep((value) => value - 1)}>Back</button> : null}
      </div>
      <div className="zorai-onboarding-actions__group">
        {step < STEPS.length - 1
          ? <button type="button" className="zorai-onboarding-button zorai-onboarding-button--primary" disabled={busy || !canContinue} onClick={() => void next()}>{busy ? "Saving…" : "Continue"}</button>
          : <button type="button" className="zorai-onboarding-button zorai-onboarding-button--primary" onClick={() => { if (!dryRun) void getBridge()?.agentRequestConciergeWelcome?.(); onClose(); }}>Finish</button>}
      </div>
    </div>
  </>;
}
