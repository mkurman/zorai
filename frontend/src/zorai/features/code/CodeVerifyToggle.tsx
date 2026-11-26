import { useEffect, useState } from "react";
import { getBridge } from "@/lib/bridge";

export function withRoot(roots: string[], root: string, enabled: boolean): string[] {
  const rest = roots.filter((entry) => entry !== root);
  return enabled ? [...rest, root] : rest;
}

export function CodeVerifyToggle({ root }: { root: string }) {
  const [enabled, setEnabled] = useState(false);
  const [browserCheck, setBrowserCheck] = useState(false);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let cancelled = false;
    void getBridge()?.agentGetConfig?.().then((config) => {
      if (cancelled) return;
      const verify = (config as { verify?: { roots?: string[]; browser_check?: boolean } } | null)?.verify;
      setEnabled((verify?.roots ?? []).includes(root));
      setBrowserCheck(verify?.browser_check === true);
    }).catch(() => undefined);
    return () => { cancelled = true; };
  }, [root]);

  return (
    <>
    <label className="zorai-workspace-isolation-toggle" title="After each agent turn that edits files, run type/compile checks and related tests. On failure the agent gets up to 2 extra attempts to fix it, which costs extra model tokens.">
      <input
        type="checkbox"
        checked={enabled}
        disabled={busy}
        onChange={(event) => {
          const next = event.target.checked;
          const bridge = getBridge();
          setBusy(true);
          void bridge?.agentGetConfig?.()
            .then((config) => bridge.agentSetConfigItem?.("/verify/roots", withRoot(((config as { verify?: { roots?: string[] } } | null)?.verify?.roots ?? []), root, next)))
            .then(() => setEnabled(next))
            .finally(() => setBusy(false));
        }}
      />
      <span>Verify agent edits (checks + related tests, up to 2 fix attempts)</span>
    </label>
    {enabled ? (
      <label className="zorai-workspace-isolation-toggle" title="When UI files change and the thread's browser has a localhost page open, reload it, fail on a dev-server error overlay and save a screenshot.">
        <input
          type="checkbox"
          checked={browserCheck}
          disabled={busy}
          onChange={(event) => {
            const next = event.target.checked;
            setBusy(true);
            void Promise.resolve(getBridge()?.agentSetConfigItem?.("/verify/browser_check", next))
              .then(() => setBrowserCheck(next))
              .finally(() => setBusy(false));
          }}
        />
        <span>Also check the open localhost page in the browser</span>
      </label>
    ) : null}
    </>
  );
}
