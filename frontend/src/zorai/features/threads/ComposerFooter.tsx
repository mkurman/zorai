import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties } from "react";
import { createPortal } from "react-dom";
import { ModelSelector } from "@/components/settings-panel/shared";
import { SECURITY_LEVELS } from "@/components/setupWizardPlan";
import { useAgentStore, type AgentMessage, type AgentProviderId, type AgentThread } from "@/lib/agentStore";
import { ComposerContextCircle } from "./ComposerContextCircle";
import { effortLabel, footerModelLabel } from "./composerFooterModel";
import { resolveThreadOwnerRuntimeProfile } from "./threadOwnerRuntime";
import {
  applyManagedSecurityLevel,
  applyThreadProviderModel,
  applyThreadReasoningEffort,
  applyThreadSandbox,
  loadThreadSandbox,
  threadHasDaemonRuntime,
  managedSecurityLevels,
  modelForThreadProviderChange,
  threadProviderIds,
  threadReasoningEfforts,
  type ThreadManagedSecurityLevel,
} from "./threadRuntimeActions";
import { useExclusivePopup } from "@/lib/exclusivePopup";

function EffortSlider({ thread, effort }: { thread: AgentThread; effort: string }) {
  const ticks = threadReasoningEfforts();
  const committed = Math.max(0, ticks.indexOf(effort as (typeof ticks)[number]));
  const [index, setIndex] = useState(committed);
  const timer = useRef(0);
  useEffect(() => setIndex(committed), [committed]);
  useEffect(() => () => window.clearTimeout(timer.current), []);
  return (
    <div className="zorai-composer-footer__effort">
      <input
        type="range"
        min={0}
        max={ticks.length - 1}
        step={1}
        value={index}
        aria-label="Reasoning effort"
        aria-valuetext={effortLabel(ticks[index])}
        onChange={(event) => {
          const next = Number(event.target.value);
          setIndex(next);
          window.clearTimeout(timer.current);
          timer.current = window.setTimeout(() => {
            if (next !== committed) void applyThreadReasoningEffort(thread, ticks[next]);
          }, 350);
        }}
      />
      <span>{effortLabel(ticks[index])}</span>
    </div>
  );
}

function SecurityMenu({ level, thread, anchor, onClose }: { level: string; thread: AgentThread | null; anchor: DOMRect; onClose: () => void }) {
  const rootRef = useRef<HTMLDivElement | null>(null);
  useExclusivePopup(onClose);
  const globalSandbox = useAgentStore((state) => state.agentSettings.managed_sandbox_enabled);
  const [threadSandbox, setThreadSandbox] = useState<boolean | null>(null);
  const [sandboxBusy, setSandboxBusy] = useState(false);
  const sandboxAvailable = Boolean(thread && threadHasDaemonRuntime(thread));
  const sandboxOn = threadSandbox ?? globalSandbox;
  useEffect(() => {
    if (!thread) return;
    let cancelled = false;
    void loadThreadSandbox(thread).then((value) => { if (!cancelled) setThreadSandbox(value); });
    return () => { cancelled = true; };
  }, [thread]);
  const toggleSandbox = () => {
    if (!thread || sandboxBusy) return;
    const next = !sandboxOn;
    const previous = threadSandbox;
    setThreadSandbox(next);
    setSandboxBusy(true);
    void applyThreadSandbox(thread, next).catch(() => setThreadSandbox(previous)).finally(() => setSandboxBusy(false));
  };
  const levels = managedSecurityLevels();
  const choose = (next: ThreadManagedSecurityLevel) => {
    onClose();
    if (next !== level) void applyManagedSecurityLevel(next);
  };

  useEffect(() => {
    const onPointerDown = (event: PointerEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) onClose();
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
      const picked = levels[Number(event.key) - 1];
      if (picked) {
        event.preventDefault();
        choose(picked);
      }
    };
    window.addEventListener("pointerdown", onPointerDown);
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("pointerdown", onPointerDown);
      window.removeEventListener("keydown", onKeyDown);
    };
  });

  return createPortal(
    <div
      ref={rootRef}
      className="zorai-security-menu"
      role="menu"
      aria-label="Security level"
      style={{ position: "fixed", right: Math.max(8, window.innerWidth - anchor.right), bottom: Math.max(8, window.innerHeight - anchor.top + 6), zIndex: 90 }}
    >
      <div className="zorai-security-menu__title">Security</div>
      {levels.map((option, index) => (
        <button
          key={option}
          type="button"
          role="menuitemradio"
          aria-checked={option === level}
          className={["zorai-security-menu__item", option === level ? "is-active" : ""].filter(Boolean).join(" ")}
          onClick={() => choose(option)}
        >
          <span className="zorai-security-menu__text">
            <span className="zorai-security-menu__name">{option[0].toUpperCase() + option.slice(1)}</span>
            <span className="zorai-security-menu__desc">{SECURITY_LEVELS.find((entry) => entry.id === option)?.label}</span>
          </span>
          <span className="zorai-security-menu__check" aria-hidden="true">{option === level ? "✓" : ""}</span>
          <span className="zorai-security-menu__key" aria-hidden="true">{index + 1}</span>
        </button>
      ))}
      <div className="zorai-security-menu__separator" role="separator" />
      <button
        type="button"
        role="menuitemcheckbox"
        aria-checked={sandboxOn}
        className="zorai-security-menu__item"
        disabled={!sandboxAvailable || sandboxBusy}
        title={sandboxAvailable ? undefined : "Send a message first; the sandbox choice is saved on the thread"}
        onClick={toggleSandbox}
      >
        <span className="zorai-security-menu__text">
          <span className="zorai-security-menu__name">Sandbox</span>
          <span className="zorai-security-menu__desc">{threadSandbox === null ? `This thread follows Settings (${globalSandbox ? "on" : "off"})` : "Set for this thread only"}</span>
        </span>
        <span className={["zorai-plus-menu__switch", sandboxOn ? "is-on" : ""].filter(Boolean).join(" ")} aria-hidden="true"><span /></span>
      </button>
    </div>,
    document.body,
  );
}

function ProviderPicker({ providers, value, disabled, onChange }: { providers: string[]; value: string; disabled: boolean; onChange: (next: string) => void }) {
  const [open, setOpen] = useState(false);
  const listRef = useRef<HTMLDivElement | null>(null);
  const rootRef = useRef<HTMLDivElement | null>(null);
  useEffect(() => {
    if (!open) return;
    const onPointerDown = (event: PointerEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) setOpen(false);
    };
    document.addEventListener("pointerdown", onPointerDown);
    return () => document.removeEventListener("pointerdown", onPointerDown);
  }, [open]);
  useLayoutEffect(() => {
    if (open) listRef.current?.querySelector<HTMLElement>("[aria-selected=true]")?.scrollIntoView({ block: "nearest" });
  }, [open]);
  return (
    <div ref={rootRef} className="zorai-provider-picker">
      <button type="button" className="zorai-provider-picker__button" disabled={disabled} aria-haspopup="listbox" aria-expanded={open} onClick={() => setOpen((current) => !current)}>
        <span>{value || "Select provider"}</span>
        <span aria-hidden="true">▾</span>
      </button>
      {open ? (
        <div ref={listRef} className="zorai-provider-picker__list" role="listbox" aria-label="Provider" onKeyDown={(event) => { if (event.key === "Escape") { event.stopPropagation(); setOpen(false); } }}>
          {providers.map((id) => (
            <button
              key={id}
              type="button"
              role="option"
              aria-selected={id === value}
              className={["zorai-provider-picker__option", id === value ? "is-active" : ""].filter(Boolean).join(" ")}
              onClick={() => {
                setOpen(false);
                if (id !== value) onChange(id);
              }}
            >
              <span>{id}</span>
              <span className="zorai-provider-picker__check" aria-hidden="true">{id === value ? "✓" : ""}</span>
            </button>
          ))}
        </div>
      ) : null}
    </div>
  );
}

function ModelPopover({ thread, anchor, onClose }: { thread: AgentThread; anchor: DOMRect; onClose: () => void }) {
  const agentSettings = useAgentStore((state) => state.agentSettings);
  const conciergeConfig = useAgentStore((state) => state.conciergeConfig);
  const coderConfig = useAgentStore((state) => state.coderConfig);
  const subAgents = useAgentStore((state) => state.subAgents);
  const profile = resolveThreadOwnerRuntimeProfile(thread, subAgents, agentSettings, conciergeConfig, coderConfig);
  const providers = threadProviderIds(profile.provider);
  const providerId = (profile.provider || providers[0] || "") as AgentProviderId;
  const providerConfig = agentSettings[providerId] as { base_url?: string; api_key?: string; auth_source?: "api_key" | "chatgpt_subscription" | "github_copilot" } | undefined;
  const [busy, setBusy] = useState(false);
  const rootRef = useRef<HTMLDivElement | null>(null);
  useExclusivePopup(onClose);
  const run = (action: () => Promise<void>) => {
    setBusy(true);
    void action().finally(() => setBusy(false));
  };

  useEffect(() => {
    const onPointerDown = (event: PointerEvent) => {
      const target = event.target as HTMLElement;
      if (rootRef.current?.contains(target) || target.closest?.("[role=listbox], [role=option]")) return;
      onClose();
    };
    const onKeyDown = (event: KeyboardEvent) => { if (event.key === "Escape") onClose(); };
    window.addEventListener("pointerdown", onPointerDown);
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("pointerdown", onPointerDown);
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [onClose]);

  const style: CSSProperties = {
    position: "fixed",
    left: Math.min(Math.max(8, anchor.left), window.innerWidth - 328),
    bottom: Math.max(8, window.innerHeight - anchor.top + 6),
    zIndex: 90,
  };
  return createPortal(
    <div ref={rootRef} className="zorai-composer-footer__popover" role="dialog" aria-label="Thread model" style={style}>
      <div className="zorai-composer-footer__field">
        <span>Provider</span>
        <ProviderPicker providers={providers} value={providerId} disabled={busy} onChange={(next) => run(() => applyThreadProviderModel(thread, next, modelForThreadProviderChange(next, profile.model)))} />
      </div>
      <label>
        <span>Model</span>
        {providerId ? (
          <ModelSelector
            providerId={providerId}
            value={profile.model}
            customName={profile.model}
            disabled={busy}
            base_url={providerConfig?.base_url}
            api_key={providerConfig?.api_key}
            auth_source={providerConfig?.auth_source}
            dropUp
            onChange={(nextModel, _name, details) => run(() => applyThreadProviderModel(thread, providerId, nextModel, details?.contextWindow))}
          />
        ) : <span className="zorai-empty-state">Configure a provider in Settings first</span>}
      </label>
      <label>
        <span>Reasoning effort</span>
        <EffortSlider thread={thread} effort={profile.effort || "none"} />
      </label>
    </div>,
    document.body,
  );
}

export function ComposerFooter({ thread, messages, showEffort, showContext, showSecurity }: {
  thread: AgentThread | null;
  messages: AgentMessage[];
  showEffort: boolean;
  showContext: boolean;
  showSecurity: boolean;
}) {
  const agentSettings = useAgentStore((state) => state.agentSettings);
  const conciergeConfig = useAgentStore((state) => state.conciergeConfig);
  const coderConfig = useAgentStore((state) => state.coderConfig);
  const subAgents = useAgentStore((state) => state.subAgents);
  const level = agentSettings.managed_security_level;
  const [modelAnchor, setModelAnchor] = useState<DOMRect | null>(null);
  const [securityAnchor, setSecurityAnchor] = useState<DOMRect | null>(null);
  const modelButton = useRef<HTMLButtonElement | null>(null);
  const profile = thread ? resolveThreadOwnerRuntimeProfile(thread, subAgents, agentSettings, conciergeConfig, coderConfig) : null;

  useLayoutEffect(() => {
    if (!modelAnchor) return;
    const update = () => { if (modelButton.current) setModelAnchor(modelButton.current.getBoundingClientRect()); };
    window.addEventListener("resize", update);
    return () => window.removeEventListener("resize", update);
  }, [modelAnchor]);

  return (
    <div className="zorai-composer-footer">
      {thread && profile ? (
        <button
          ref={modelButton}
          type="button"
          className="zorai-composer-footer__model"
          title={`${profile.provider} · ${profile.model} — click to change this thread's model`}
          aria-haspopup="dialog"
          aria-expanded={Boolean(modelAnchor)}
          onPointerDown={(event) => event.stopPropagation()}
          onClick={(event) => {
            const rect = event.currentTarget.getBoundingClientRect();
            setSecurityAnchor(null);
            setModelAnchor((current) => (current ? null : rect));
          }}
        >
          {footerModelLabel(profile.model)}
          {showEffort ? <span className="zorai-composer-footer__effort-label">{effortLabel(profile.effort || "none")}</span> : null}
        </button>
      ) : null}
      {showSecurity ? (
        <button
          type="button"
          className="zorai-composer-footer__security"
          title="Managed security: how strictly Weles reviews shell commands before they run"
          aria-haspopup="menu"
          aria-expanded={Boolean(securityAnchor)}
          onPointerDown={(event) => event.stopPropagation()}
          onClick={(event) => {
            const rect = event.currentTarget.getBoundingClientRect();
            setModelAnchor(null);
            setSecurityAnchor((current) => (current ? null : rect));
          }}
        >
          {`Security: ${level}`}
        </button>
      ) : null}
      {showContext ? <ComposerContextCircle key={thread?.id ?? "none"} thread={thread} messages={messages} compact /> : null}
      {securityAnchor ? <SecurityMenu level={level} thread={thread} anchor={securityAnchor} onClose={() => setSecurityAnchor(null)} /> : null}
      {thread && modelAnchor ? <ModelPopover thread={thread} anchor={modelAnchor} onClose={() => setModelAnchor(null)} /> : null}
    </div>
  );
}
