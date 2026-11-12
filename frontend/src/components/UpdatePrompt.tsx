import { useEffect } from "react";
import { create } from "zustand";
import { getBridge } from "@/lib/bridge";

export type UpdatePromptState =
  | { phase: "hidden" }
  | { phase: "available" | "installing" | "installed" | "restarting"; currentVersion: string; latestVersion: string }
  | { phase: "failed"; currentVersion: string; latestVersion: string; output: string };

export function UpdatePromptCard({ state, onUpdate, onRestart, onDismiss }: {
  state: UpdatePromptState; onUpdate: () => void; onRestart: () => void; onDismiss: () => void;
}) {
  if (state.phase === "hidden") return null;
  const { latestVersion, currentVersion } = state;
  return <div className="zorai-toast zorai-update-prompt" role="status" aria-live="polite">
    {state.phase === "available" ? <>
      <strong>Zorai {latestVersion} is available</strong>
      <p>You have {currentVersion}. The update installs in the background while you keep working.</p>
      <div><button type="button" className="is-primary" onClick={onUpdate}>Update</button><button type="button" onClick={onDismiss}>Later</button></div>
    </> : state.phase === "installing" ? <>
      <strong>Installing Zorai {latestVersion}…</strong>
      <p>You can keep working. You'll be asked before Zorai restarts.</p>
    </> : state.phase === "installed" ? <>
      <strong>Zorai {latestVersion} is installed</strong>
      <p>Restart now, or it applies when you next close Zorai. Either way the daemon stops, so running agent tasks are interrupted.</p>
      <div><button type="button" className="is-primary" onClick={onRestart}>Restart now</button><button type="button" onClick={onDismiss}>Later</button></div>
    </> : state.phase === "restarting" ? <>
      <strong>Restarting Zorai…</strong>
    </> : state.phase === "failed" ? <>
      <strong>Update to {latestVersion} failed</strong>
      <pre>{state.output.split("\n").slice(-6).join("\n")}</pre>
      <div><button type="button" className="is-primary" onClick={onUpdate}>Retry</button><button type="button" onClick={onDismiss}>Dismiss</button></div>
    </> : null}
  </div>;
}

type ManualCheck = "idle" | "checking" | "current" | "failed";
type UpdateSimulation = "success" | "failure";

interface UpdateStore {
  state: UpdatePromptState;
  manualCheck: ManualCheck;
  simulation: UpdateSimulation | null;
  simulate: (outcome: UpdateSimulation, currentVersion: string) => void;
  check: (manual?: boolean) => Promise<void>;
  install: () => Promise<void>;
  restart: () => void;
  dismiss: () => void;
}

export const useUpdateStore = create<UpdateStore>((set, get) => ({
  state: { phase: "hidden" },
  manualCheck: "idle",
  simulation: null,
  simulate: (outcome, currentVersion) => set({ simulation: outcome, state: { phase: "available", currentVersion, latestVersion: "99.0.0" } }),
  check: async (manual = false) => {
    if (manual) set({ manualCheck: "checking" });
    const status = await getBridge()?.appUpdateCheck?.();
    if (status?.updateAvailable && status.latestVersion) {
      if (get().state.phase === "hidden") set({ state: { phase: "available", currentVersion: status.currentVersion, latestVersion: status.latestVersion } });
      if (manual) set({ manualCheck: "idle" });
      return;
    }
    if (manual) set({ manualCheck: status && !status.error ? "current" : "failed" });
  },
  install: async () => {
    const current = get().state;
    if (current.phase === "hidden") return;
    const versions = { currentVersion: current.currentVersion, latestVersion: current.latestVersion };
    set({ state: { phase: "installing", ...versions } });
    const simulation = get().simulation;
    const result = simulation
      ? await new Promise<{ ok: boolean; output: string }>((resolve) => setTimeout(() => resolve({ ok: simulation === "success", output: "Simulated installer failure: nothing was downloaded or changed." }), 1500))
      : await getBridge()?.appUpdateInstall?.();
    set({ state: result?.ok
      ? { phase: "installed", ...versions }
      : { phase: "failed", ...versions, output: result?.output || "The desktop bridge does not support updates." } });
  },
  restart: () => {
    const current = get().state;
    if (current.phase === "hidden") return;
    if (get().simulation) {
      set({ simulation: null, state: { phase: "hidden" } });
      return;
    }
    set({ state: { phase: "restarting", currentVersion: current.currentVersion, latestVersion: current.latestVersion } });
    void getBridge()?.appUpdateRestart?.();
  },
  dismiss: () => set({ simulation: null, state: { phase: "hidden" } }),
}));

export function UpdatePrompt() {
  const state = useUpdateStore((store) => store.state);

  useEffect(() => {
    void useUpdateStore.getState().check();
  }, []);

  const { install, restart, dismiss } = useUpdateStore.getState();
  return <UpdatePromptCard state={state} onUpdate={() => void install()} onRestart={restart} onDismiss={dismiss} />;
}

export function UpdateCheckButton() {
  const manualCheck = useUpdateStore((store) => store.manualCheck);
  const phase = useUpdateStore((store) => store.state.phase);
  const label = manualCheck === "checking" ? "Checking…" : manualCheck === "current" ? "Up to date" : manualCheck === "failed" ? "Check failed — retry" : "Check for updates";
  return <button type="button" className="zorai-ghost-button" disabled={manualCheck === "checking" || phase === "installing" || phase === "restarting"} onClick={() => void useUpdateStore.getState().check(true)}>{label}</button>;
}
