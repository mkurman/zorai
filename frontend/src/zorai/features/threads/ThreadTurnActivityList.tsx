import { memo, useState } from "react";
import { MemoizedToolEventRow, sameToolEventGroup } from "@/components/agent-chat-panel/chat-view/ToolEventRow";
import { ZoraiToolMark } from "@/components/agent-chat-panel/chat-view/ZoraiToolMark";
import type {
  OperationStatusView,
  ThreadMutationResult,
} from "@/components/agent-chat-panel/runtime/types";
import { visibleGeminiReasoning } from "@/lib/geminiThought";
import { ThreadActivityRow } from "./ThreadActivityRow";
import { classifySystemNotice, classifyThreadActivityMessage } from "./threadActivityModel";
import { ThreadReasoningBlock } from "./NativeThreadMessageBubble";
import type { TurnActivityEntry } from "./threadTurnActivity";

export function summarizeTurnActivity(entries: TurnActivityEntry[]): {
  title: string;
  working: boolean;
  toolCount: number;
  doneCount: number;
} {
  let toolCount = 0;
  let doneCount = 0;
  let working = false;
  for (const entry of entries) {
    if (entry.kind !== "tools") continue;
    for (const group of entry.groups) {
      toolCount += 1;
      if (group.status === "done") doneCount += 1;
      else if (group.status === "requested" || group.status === "executing") working = true;
    }
  }
  const last = entries[entries.length - 1];
  if (last && last.kind !== "tools" && last.message.isStreaming) working = true;
  return { title: last ? turnActivityEntryTitle(last) : "Activity", working, toolCount, doneCount };
}

function turnActivityEntryTitle(entry: TurnActivityEntry): string {
  if (entry.kind === "tools") {
    const rawName = entry.groups[entry.groups.length - 1]?.toolName || "Tools";
    return rawName.split("_").map((word) => word.charAt(0).toUpperCase() + word.slice(1)).join(" ");
  }
  if (entry.kind === "reasoning") return "Thinking";
  const activity = classifyThreadActivityMessage(entry.message) ?? classifySystemNotice(entry.message);
  if (!activity) return "Notice";
  if (activity.kind === "handoff") return "Handoff";
  return activity.title;
}

export const ThreadTurnActivityList = memo(function ThreadTurnActivityList({
  entries,
  onRefreshOperation,
  onCancelOperation,
}: {
  entries: TurnActivityEntry[];
  onRefreshOperation: (operationId: string) => Promise<OperationStatusView | null>;
  onCancelOperation: (operationId: string) => Promise<ThreadMutationResult>;
}) {
  const [expanded, setExpanded] = useState(false);
  if (entries.length === 0) return null;
  const { title, working, toolCount, doneCount } = summarizeTurnActivity(entries);

  return (
    <div className="acp-tool-list zorai-turn-activity">
      <button
        type="button"
        aria-expanded={expanded}
        className="acp-tool-list__header"
        onClick={() => setExpanded((prev) => !prev)}
      >
        <div className="acp-tool-list__title-container">
          <span className="acp-tool-list__title-slot" title={title}>
            <span className={`acp-tool-list__title${working ? " acp-tool-list__title--working" : ""}`}>{title}</span>
          </span>
          <ZoraiToolMark active={working} />
        </div>
        <span className="acp-tool-list__stats">
          {toolCount > 0 ? `[${doneCount} / ${toolCount}]` : `[${entries.length}]`}
        </span>
      </button>
      {expanded ? (
        <div className="acp-tool-list__body">
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
        </div>
      ) : null}
    </div>
  );
}, (prev, next) => (
  prev.onRefreshOperation === next.onRefreshOperation
  && prev.onCancelOperation === next.onCancelOperation
  && sameTurnActivityEntries(prev.entries, next.entries)
));

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
