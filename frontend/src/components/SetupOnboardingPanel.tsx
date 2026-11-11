import { useCallback, useEffect, useMemo, useState } from "react";

import { CONCIERGE_AGENT_NAME, PRIMARY_AGENT_NAME } from "../lib/agentNames";
import { ZORAI_APP_DESCRIPTION, ZORAI_APP_NAME } from "../zorai/branding";
import { SetupWizard } from "./SetupWizard";

const SETUP_PANEL_STATE_KEY = "zorai-setup-onboarding-state-v1";
const SETUP_PANEL_VERSION = "1";
const OPEN_EVENTS = ["zorai-open-setup-onboarding", "zorai-open-setup-onboarding"] as const;

type SetupPanelState = {
  seenVersion?: string;
  dismissedAt?: number;
};

function readSetupPanelState(): SetupPanelState {
  try {
    const raw = window.localStorage.getItem(SETUP_PANEL_STATE_KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw);
    if (!parsed || typeof parsed !== "object") return {};
    return parsed as SetupPanelState;
  } catch {
    return {};
  }
}

function writeSetupPanelState(next: SetupPanelState): void {
  try {
    window.localStorage.setItem(SETUP_PANEL_STATE_KEY, JSON.stringify(next));
  } catch {
    // Ignore localStorage failures.
  }
}

function bridge(): ZoraiBridge | null {
  return (window.zorai ?? window.zorai) ?? null;
}

export function SetupOnboardingPanel() {
  const [report, setReport] = useState<ZoraiSetupPrereqReport | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [forcedOpen, setForcedOpen] = useState(false);
  const [wizardActive, setWizardActive] = useState(false);
  const [panelState, setPanelState] = useState<SetupPanelState>(() => readSetupPanelState());

  const refresh = useCallback(async () => {
    const zorai = bridge();
    if (!zorai?.checkSetupPrereqs) {
      setLoading(false);
      setReport(null);
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const next = await zorai.checkSetupPrereqs("desktop");
      setReport(next);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    const openPanel = () => setForcedOpen(true);
    OPEN_EVENTS.forEach((eventName) => window.addEventListener(eventName, openPanel));
    return () => {
      OPEN_EVENTS.forEach((eventName) => window.removeEventListener(eventName, openPanel));
    };
  }, []);

  const shouldShow = useMemo(() => {
    if (forcedOpen) return true;
    if (!report) return false;
    return panelState.seenVersion !== SETUP_PANEL_VERSION;
  }, [forcedOpen, panelState.seenVersion, report]);

  const dismiss = useCallback(() => {
    const next: SetupPanelState = {
      seenVersion: SETUP_PANEL_VERSION,
      dismissedAt: Date.now(),
    };
    writeSetupPanelState(next);
    setPanelState(next);
    setForcedOpen(false);
    setWizardActive(false);
  }, []);

  if (!shouldShow) return null;

  return (
    <div className="zorai-onboarding-backdrop">
      <div className="zorai-onboarding-dialog">
        <div className="zorai-onboarding-stack">
          <div className="zorai-onboarding-kicker">First-run setup</div>
          <h2>{ZORAI_APP_NAME} Setup Assistant</h2>
          {wizardActive ? null : <div className="zorai-onboarding-copy">
            {report?.whatIsZorai ?? ZORAI_APP_DESCRIPTION}
          </div>}
        </div>

        {wizardActive ? <SetupWizard dataDir={report?.dataDir} onClose={dismiss} /> : <>

        <div className="zorai-onboarding-grid-2">
          <div className="zorai-onboarding-card">
            <span className="zorai-onboarding-card__label">{PRIMARY_AGENT_NAME}</span>
            <span className="zorai-onboarding-card__title">Main agent runtime</span>
            <span className="zorai-onboarding-card__body">
              {PRIMARY_AGENT_NAME} is your primary agent for chat, tasks, provider selection, and execution.
            </span>
          </div>
          <div className="zorai-onboarding-card">
            <span className="zorai-onboarding-card__label">{CONCIERGE_AGENT_NAME}</span>
            <span className="zorai-onboarding-card__title">Concierge and briefing agent</span>
            <span className="zorai-onboarding-card__body">
              {CONCIERGE_AGENT_NAME} handles welcomes, briefings, and guidance, and can inherit {PRIMARY_AGENT_NAME}'s provider defaults unless you override them.
            </span>
          </div>
        </div>

        {loading ? (
          <div className="zorai-onboarding-copy">Checking dependencies...</div>
        ) : null}

        {error ? (
          <div className="zorai-onboarding-error">
            Setup check failed: {error}
          </div>
        ) : null}

        {report ? (
          <>
            <div className="zorai-onboarding-grid-3">
              <InfoCard label="Install Root" value={report.installRoot} />
              <InfoCard label="Daemon Path" value={report.daemonPath} />
              <InfoCard label="Data Directory" value={report.dataDir} />
            </div>

            <div className="zorai-onboarding-section">
              <div className="zorai-onboarding-section__title">
                Required runtime dependencies ({report.required.length})
              </div>
              {report.required.length === 0 ? (
                <div className="zorai-onboarding-copy--sm zorai-onboarding-copy">
                  No hard blockers for this runtime profile. Optional integrations are listed below.
                </div>
              ) : null}
              {report.required.map((dep) => (
                <div
                  key={dep.name}
                  className={["zorai-onboarding-dep", dep.found ? "zorai-onboarding-dep--found" : ""].filter(Boolean).join(" ")}
                >
                  <div className="zorai-onboarding-dep__row">
                    <span>{dep.label}</span>
                    <span className="zorai-onboarding-dep__status">
                      {dep.found ? "installed" : "missing"}
                    </span>
                  </div>
                  {dep.path ? <code>{dep.path}</code> : null}
                  {!dep.found && dep.installHints.length > 0 ? (
                    <div className="zorai-onboarding-copy--sm zorai-onboarding-copy">
                      Install: <code>{dep.installHints[0]}</code>
                    </div>
                  ) : null}
                </div>
              ))}
            </div>

            {report.optional.length > 0 ? (
              <div className="zorai-onboarding-copy--sm zorai-onboarding-copy">
                Optional tools: {report.optional.map((dep) => dep.name).join(", ")}
              </div>
            ) : null}
          </>
        ) : null}

        <div className="zorai-onboarding-actions">
          <button type="button" className="zorai-onboarding-button" onClick={() => void refresh()}>
            Re-check
          </button>
          <button type="button" className="zorai-onboarding-button" onClick={dismiss}>
            Dismiss
          </button>
          <button type="button" className="zorai-onboarding-button zorai-onboarding-button--primary" onClick={() => setWizardActive(true)}>
            Start setup
          </button>
        </div>
        </>}
      </div>
    </div>
  );
}

function InfoCard({ label, value }: { label: string; value: string }) {
  return (
    <div className="zorai-onboarding-card zorai-onboarding-card--compact">
      <span className="zorai-onboarding-card__label">{label}</span>
      <code>{value}</code>
    </div>
  );
}
