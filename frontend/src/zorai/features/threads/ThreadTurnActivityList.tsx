import { memo, useEffect, useRef, useState } from "react";
import { MemoizedToolEventRow, sameToolEventGroup } from "@/components/agent-chat-panel/chat-view/ToolEventRow";
import { ZoraiToolMark } from "@/components/agent-chat-panel/chat-view/ZoraiToolMark";
import { ToolListCollapseFooter } from "@/components/agent-chat-panel/chat-view/ToolEventList";
import {
  activityPreview,
  formatActivityElapsed,
  humanizeToolName,
  toolIntentLabel,
} from "@/components/agent-chat-panel/chat-view/activityPreview";
import type { ToolEventGroup } from "@/components/agent-chat-panel/chat-view/types";
import type {
  OperationStatusView,
  ThreadMutationResult,
} from "@/components/agent-chat-panel/runtime/types";
import { visibleGeminiReasoning } from "@/lib/geminiThought";
import { ThreadActivityRow } from "./ThreadActivityRow";
import { classifySystemNotice, classifyThreadActivityMessage } from "./threadActivityModel";
import { ThreadReasoningBlock } from "./NativeThreadMessageBubble";
import type { TurnActivityEntry } from "./threadTurnActivity";
import { usePresence } from "@/hooks/usePresence";
import { finishedTurnSummary } from "./turnActivitySummary";
import { useToolRevealStore } from "@/components/agent-chat-panel/chat-view/toolRevealStore";
import { useSettingsStore } from "@/lib/settingsStore";

const EXPAND_SCROLL_DELAY_MS = 220;

export function summarizeTurnActivity(entries: TurnActivityEntry[]): {
  title: string;
  working: boolean;
  toolCount: number;
  doneCount: number;
} {
  let toolCount = 0;
  let doneCount = 0;
  let working = false;
  let runningGroup: ToolEventGroup | null = null;
  for (const entry of entries) {
    if (entry.kind !== "tools") continue;
    for (const group of entry.groups) {
      toolCount += 1;
      if (group.status === "done") doneCount += 1;
      else if (group.status === "requested" || group.status === "executing") {
        working = true;
        runningGroup = group;
      }
    }
  }
  const last = entries[entries.length - 1];
  if (last && last.kind !== "tools" && last.message.isStreaming) {
    working = true;
    runningGroup = null;
  }
  const title = runningGroup ? toolGroupTitle(runningGroup) : last ? turnActivityEntryTitle(last) : "Activity";
  return { title, working, toolCount, doneCount };
}

export function turnActivitySpan(entries: TurnActivityEntry[]): { startedAt: number; endedAt: number } {
  let startedAt = Number.POSITIVE_INFINITY;
  let endedAt = 0;
  for (const entry of entries) {
    if (entry.kind === "tools") {
      for (const group of entry.groups) {
        startedAt = Math.min(startedAt, group.createdAt);
        endedAt = Math.max(endedAt, group.completedAt ?? group.createdAt);
      }
    } else {
      startedAt = Math.min(startedAt, entry.message.createdAt);
      endedAt = Math.max(endedAt, entry.message.createdAt);
    }
  }
  return Number.isFinite(startedAt) ? { startedAt, endedAt } : { startedAt: 0, endedAt: 0 };
}

function finishedSummary(entries: TurnActivityEntry[], elapsedMs: number): string {
  const groups = entries.flatMap((entry) => (entry.kind === "tools" ? entry.groups : []));
  const lastGroup = groups[groups.length - 1];
  return finishedTurnSummary(entries[0]?.key ?? "", groups, elapsedMs, lastGroup ? toolGroupTitle(lastGroup) : null);
}

export function turnActivityBounds(
  span: { startedAt: number; endedAt: number },
  startedAfter: number | undefined,
  endedBefore: number | undefined,
): { startedAt: number; endedAt: number } {
  if (!span.startedAt) return span;
  const startedAt = startedAfter !== undefined && startedAfter < span.startedAt ? startedAfter : span.startedAt;
  const endedAt = endedBefore !== undefined && endedBefore > span.endedAt ? endedBefore : span.endedAt;
  return { startedAt, endedAt };
}

function toolGroupTitle(group: ToolEventGroup): string {
  return toolIntentLabel(group.toolArguments) || humanizeToolName(group.toolName || "Tools");
}

function turnActivityEntryTitle(entry: TurnActivityEntry): string {
  if (entry.kind === "tools") {
    const group = entry.groups[entry.groups.length - 1];
    return group ? toolGroupTitle(group) : "Tools";
  }
  if (entry.kind === "reasoning") {
    return activityPreview(visibleGeminiReasoning(entry.message.reasoning)) || "Thinking";
  }
  const activity = classifyThreadActivityMessage(entry.message) ?? classifySystemNotice(entry.message);
  if (!activity) return "Notice";
  if (activity.kind === "handoff") return "Handoff";
  return activity.title;
}

export function finishedTurnStats(toolCount: number, failedCount: number, entryCount: number): string {
  const steps = toolCount > 0 ? toolCount : entryCount;
  const label = `${steps} ${steps === 1 ? "step" : "steps"}`;
  return failedCount > 0 ? `${label} · ${failedCount} failed` : label;
}

export const ThreadTurnActivityList = memo(function ThreadTurnActivityList({
  entries,
  live,
  startedAfter,
  endedBefore,
  onRefreshOperation,
  onCancelOperation,
}: {
  entries: TurnActivityEntry[];
  live: boolean;
  startedAfter?: number;
  endedBefore?: number;
  onRefreshOperation: (operationId: string) => Promise<OperationStatusView | null>;
  onCancelOperation: (operationId: string) => Promise<ThreadMutationResult>;
}) {
  const [expanded, setExpanded] = useState(false);
  const presence = usePresence(expanded);
  const headerRef = useRef<HTMLButtonElement | null>(null);
  const bodyRef = useRef<HTMLDivElement | null>(null);
  const scrollOnExpandRef = useRef(false);
  const scrollToLatestOnExpand = useSettingsStore((state) => state.settings.scrollToLatestToolOnExpand);
  const revealNonce = useToolRevealStore((state) => (
    state.key && entries.some((entry) => entry.kind === "tools" && entry.groups.some((group) => group.key === state.key)) ? state.nonce : 0
  ));
  useEffect(() => {
    if (revealNonce) setExpanded(true);
  }, [revealNonce]);
  useEffect(() => {
    if (!expanded || !scrollOnExpandRef.current) return;
    scrollOnExpandRef.current = false;
    const timer = window.setTimeout(
      () => bodyRef.current?.lastElementChild?.scrollIntoView({ block: "nearest" }),
      EXPAND_SCROLL_DELAY_MS,
    );
    return () => window.clearTimeout(timer);
  }, [expanded]);
  if (entries.length === 0) return null;
  const { title, working, toolCount } = summarizeTurnActivity(entries);
  const { startedAt, endedAt } = turnActivityBounds(turnActivitySpan(entries), startedAfter, live ? undefined : endedBefore);
  const failedCount = entries.reduce((sum, entry) => sum + (entry.kind === "tools" ? entry.groups.filter((group) => group.status === "error").length : 0), 0);
  const active = live || working;
  const finished = !active;
  const headerTitle = finished ? finishedSummary(entries, endedAt - startedAt) : title;

  return (
    <div className="acp-tool-list zorai-turn-activity">
      <button
        ref={headerRef}
        type="button"
        aria-expanded={expanded}
        className="acp-tool-list__header"
        onClick={() => {
          scrollOnExpandRef.current = !expanded && scrollToLatestOnExpand;
          setExpanded(!expanded);
        }}
      >
        <div className="acp-tool-list__title-container">
          <ZoraiToolMark active={active} />
          <span className="acp-tool-list__title-slot" title={title}>
            <span className={`acp-tool-list__title${active ? " acp-tool-list__title--working" : ""}`}>{headerTitle}</span>
          </span>
          {finished && headerTitle.startsWith("Worked") ? null : <TurnActivityElapsed startedAt={startedAt} endedAt={endedAt} live={active} />}
        </div>
        <span className="acp-tool-list__stats">
          {finishedTurnStats(toolCount, failedCount, entries.length)}
        </span>
        <span className={expanded ? "acp-tool-row__caret zorai-motion-caret is-open" : "acp-tool-row__caret zorai-motion-caret"} aria-hidden="true">▶</span>
      </button>
      {presence.mounted ? (
        <div ref={bodyRef} data-closing={presence.closing || undefined} className="acp-tool-list__body zorai-reveal">
          {entries.map((entry) => {
            if (entry.kind === "tools") {
              return entry.groups.map((group) => <MemoizedToolEventRow key={group.key} group={group} />);
            }
            if (entry.kind === "reasoning") {
              return (
                <ThreadReasoningBlock
                  key={entry.key}
                  content={visibleGeminiReasoning(entry.message.reasoning)}
                  streaming={Boolean(entry.message.isStreaming)}
                />
              );
            }
            const activity = classifyThreadActivityMessage(entry.message) ?? classifySystemNotice(entry.message);
            return activity ? (
              <div key={entry.key} id={`zorai-message-${entry.message.id}`}>
                <ThreadActivityRow
                  activity={activity}
                  createdAt={entry.message.createdAt}
                  onRefreshOperation={onRefreshOperation}
                  onCancelOperation={onCancelOperation}
                />
              </div>
            ) : null;
          })}
          <ToolListCollapseFooter headerRef={headerRef} onCollapse={() => setExpanded(false)} />
        </div>
      ) : null}
    </div>
  );
}, (prev, next) => (
  prev.live === next.live
  && prev.startedAfter === next.startedAfter
  && prev.endedBefore === next.endedBefore
  && prev.onRefreshOperation === next.onRefreshOperation
  && prev.onCancelOperation === next.onCancelOperation
  && sameTurnActivityEntries(prev.entries, next.entries)
));

function TurnActivityElapsed({ startedAt, endedAt, live }: { startedAt: number; endedAt: number; live: boolean }) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!live) return;
    setNow(Date.now());
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [live]);
  if (!startedAt) return null;
  const elapsed = (live ? Math.max(now, endedAt) : endedAt) - startedAt;
  if (!live && elapsed < 1000) return null;
  return <span className="acp-tool-list__elapsed">{formatActivityElapsed(elapsed)}</span>;
}

function sameTurnActivityEntries(prev: TurnActivityEntry[], next: TurnActivityEntry[]): boolean {
  if (prev.length !== next.length) return false;
  for (let index = 0; index < prev.length; index += 1) {
    const left = prev[index];
    const right = next[index];
    if (left.kind !== right.kind || left.key !== right.key) return false;
    if (left.kind === "tools" && right.kind === "tools") {
      if (left.groups.length !== right.groups.length) return false;
      for (let group = 0; group < left.groups.length; group += 1) {
        if (!sameToolEventGroup(left.groups[group], right.groups[group])) return false;
      }
    } else if (left.kind !== "tools" && right.kind !== "tools" && left.message !== right.message) {
      return false;
    }
  }
  return true;
}
