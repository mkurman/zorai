import { useState } from "react";
import { getBridge } from "@/lib/bridge";
import { useAgentStore } from "@/lib/agentStore";
import { useComputerControlStore } from "./computerControlStore";

export function ComputerControlBanner() {
  const active = useComputerControlStore((state) => state.active);
  const threadId = useComputerControlStore((state) => state.threadId);
  const threadTitle = useAgentStore((state) => state.threads.find((thread) => thread.daemonThreadId === threadId || thread.id === threadId)?.title ?? null);
  const [stopping, setStopping] = useState(false);
  if (!active || getBridge()) return null;
  const stop = async () => {
    setStopping(true);
    try {
      await getBridge()?.agentStopComputerControl?.();
    } finally {
      setStopping(false);
    }
  };
  return (
    <>
      <div className="zorai-computer-control-frame" aria-hidden="true" />
      <div className="zorai-computer-control-banner" role="status" aria-live="polite">
        <span className="zorai-computer-control-banner__dot" aria-hidden="true" />
        <span>
          <strong>Zorai is controlling your computer</strong>
          {threadTitle ? <small>{threadTitle}</small> : null}
        </span>
        <button type="button" className="zorai-primary-button" disabled={stopping} onClick={() => void stop()}>
          {stopping ? "Stopping…" : "Stop"}
        </button>
      </div>
    </>
  );
}
