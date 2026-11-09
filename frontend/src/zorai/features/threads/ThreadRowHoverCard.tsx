import { createPortal } from "react-dom";
import type { AgentThread } from "@/lib/agentStore";
import { normalizeEpochMs } from "./threadFilterModel";

const compactNumber = new Intl.NumberFormat("en", { notation: "compact", maximumFractionDigits: 1 });

export function threadCreatedLabel(createdAt: number): string | null {
  const at = normalizeEpochMs(createdAt);
  if (at == null || at <= 0) return null;
  return new Date(at).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
}

export function threadRowHoverFacts(thread: AgentThread, activity: string, workers: number): Array<[string, string]> {
  const facts: Array<[string, string]> = [];
  const created = threadCreatedLabel(thread.createdAt);
  if (created) facts.push(["Created", created]);
  facts.push(["Updated", activity]);
  if (thread.messageCount > 0) facts.push(["Messages", String(thread.messageCount)]);
  if (workers > 0) facts.push(["Agents", `${workers} spawned`]);
  const model = thread.profileModel ?? thread.upstreamModel;
  const provider = thread.profileProvider ?? thread.upstreamProvider;
  if (model) facts.push(["Model", provider ? `${model} · ${provider}` : model]);
  if (thread.totalTokens > 0) facts.push(["Tokens", compactNumber.format(thread.totalTokens)]);
  return facts;
}

export function ThreadRowHoverCard({
  anchor,
  thread,
  activity,
  workers,
  working,
}: {
  anchor: DOMRect;
  thread: AgentThread;
  activity: string;
  workers: number;
  working: boolean;
}) {
  if (typeof document === "undefined") return null;
  const preview = thread.lastMessagePreview?.trim();
  const top = Math.max(8, Math.min(anchor.top - 6, window.innerHeight - 240));
  return createPortal(
    <div className="zorai-thread-hovercard" role="tooltip" style={{ top, left: anchor.right + 10 }}>
      <div className="zorai-thread-hovercard__title">
        {working ? <span className="zorai-thread-dot zorai-thread-dot--working" aria-hidden="true" /> : null}
        <strong>{thread.title}</strong>
      </div>
      {preview ? <p className="zorai-thread-hovercard__preview">{preview}</p> : null}
      <dl className="zorai-thread-hovercard__facts">
        {threadRowHoverFacts(thread, activity, workers).map(([label, value]) => (
          <div key={label}>
            <dt>{label}</dt>
            <dd>{value}</dd>
          </div>
        ))}
      </dl>
    </div>,
    document.body,
  );
}
