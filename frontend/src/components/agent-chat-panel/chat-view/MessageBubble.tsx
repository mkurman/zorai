import { memo, useState } from "react";
import type { AgentMessage } from "../../../lib/agentStore";
import { parseHandoffSystemEvent } from "./helpers";
import { MarkdownContent } from "./markdown";
import { getToolDiffPresentation, ToolDiffView } from "./toolDiffPresentation";
import {
  getToolFileTarget,
  getToolStructuredFields,
  ToolFileTargetView,
  ToolStructuredValueView,
} from "./toolValuePresentation";
import { buildProviderFinalResultPresentation } from "../providerFinalResultPresentation";
import {
  compactionArtifactDisplayText,
  compactionArtifactHasExpandablePayload,
  compactionArtifactHeaderText,
  compactionArtifactPayloadText,
  isCompactionArtifactMessage,
} from "./compactionArtifact";

export {
  compactionArtifactDisplayText,
  isCompactionArtifactMessage,
} from "./compactionArtifact";

function ActionBtn({ label, onClick }: { label: string; onClick: () => void }) {
  return (
    <button
      className="acp-action-btn"
      onClick={(event) => {
        event.stopPropagation();
        onClick();
      }}
    >
      {label}
    </button>
  );
}

/**
 * Memoized chat bubble. Custom comparator: `message` object identity plus the
 * small set of props that actually affect rendering. Callback props are
 * ignored on purpose — they are recreated per render in ChatView but only
 * close over stable store actions / the message itself, so skipping them
 * keeps every historical bubble from re-rendering (and re-parsing markdown /
 * tool JSON) on each streaming frame.
 */
export const MemoizedMessageBubble = memo(MessageBubble, (prev, next) => {
  if (prev.message !== next.message) return false;
  if (prev.isSpeaking !== next.isSpeaking) return false;
  if (prev.isSpeechPaused !== next.isSpeechPaused) return false;
  return true;
});

export function MessageBubble({
  message,
  onCopy,
  onRerun,
  onRegenerate,
  onDelete,
  onPin,
  onUnpin,
  onSpeak,
  onFeedback,
  onFork,
  onExport,
  isSpeaking = false,
  isSpeechPaused = false,
}: {
  message: AgentMessage;
  onCopy?: () => void;
  onRerun?: () => void;
  onRegenerate?: () => void;
  onDelete?: () => void;
  onPin?: () => void | Promise<void>;
  onUnpin?: () => void | Promise<void>;
  onSpeak?: () => void | Promise<void>;
  onFeedback?: (reaction: "up" | "down" | null) => void | Promise<void>;
  onFork?: () => void | Promise<void>;
  onExport?: () => void | Promise<void>;
  isSpeaking?: boolean;
  isSpeechPaused?: boolean;
}) {
  const isCompactionArtifact = isCompactionArtifactMessage(message);
  const isUser = message.role === "user";
  const isSystem = message.role === "system";
  const isTool = message.role === "tool";
  const isAssistant = message.role === "assistant";
  const toolStatusLabel = message.toolStatus ? message.toolStatus.toUpperCase() : "DONE";
  const [hovered, setHovered] = useState(false);
  const [copied, setCopied] = useState(false);
  const [expandedCompaction, setExpandedCompaction] = useState(false);
  const [expandedHandoff, setExpandedHandoff] = useState(false);
  const [expandedProviderResult, setExpandedProviderResult] = useState(false);
  const handoffEvent = isSystem && typeof message.content === "string"
    ? parseHandoffSystemEvent(message.content)
    : null;
  const providerFinalResult = buildProviderFinalResultPresentation(
    message.providerFinalResult,
  );
  const toolDiff = isTool && message.toolName && message.toolArguments
    ? getToolDiffPresentation(message.toolName, message.toolArguments)
    : null;
  const fileTarget = isTool && message.toolName && message.toolArguments
    ? getToolFileTarget(message.toolName, message.toolArguments)
    : null;
  const structuredArgs = isTool && message.toolName && message.toolArguments
    ? getToolStructuredFields(message.toolName, message.toolArguments, "arguments")
    : null;
  const structuredArgDetails = fileTarget && structuredArgs
    ? structuredArgs.filter((field) => field.key !== "path")
    : structuredArgs;
  const structuredResult = isTool && message.toolName && message.content
    ? getToolStructuredFields(message.toolName, message.content, "result")
    : null;
  const displayContent = (() => {
    if (!isUser || typeof message.content !== "string") return message.content;
    if (!message.content.startsWith("[Gateway Context]")) return message.content;

    const marker = "User message:\n";
    const markerIndex = message.content.indexOf(marker);
    if (markerIndex < 0) return message.content;

    return message.content.slice(markerIndex + marker.length).trim();
  })();
  const mediaBlocks = Array.isArray(message.contentBlocks)
    ? message.contentBlocks.filter((block) => block.type === "image" || block.type === "audio")
    : [];

  const handleCopy = () => {
    onCopy?.();
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  };

  if (isCompactionArtifact) {
    const headerText = compactionArtifactHeaderText(message);
    const payloadText = compactionArtifactPayloadText(message);
    const compactionContent = compactionArtifactDisplayText(message);
    const showPayload = expandedCompaction && payloadText.length > 0;
    const visibleContent = expandedCompaction || compactionContent.length <= 280
      ? compactionContent
      : `${compactionContent.slice(0, 280).trimEnd()}...`;
    const canExpand = compactionArtifactHasExpandablePayload(message) || compactionContent.length > 280;

    return (
      <div
        id={`agent-message-${message.id}`}
        className="acp-compaction"
        onMouseEnter={() => setHovered(true)}
        onMouseLeave={() => setHovered(false)}
      >
        <div className="acp-compaction__rule">---- auto compaction ----</div>
        <div className="acp-compaction__body">
          {payloadText.length > 0 ? (
            <>
              <div>{headerText || "rule based"}</div>
              {showPayload ? <div>{payloadText}</div> : null}
            </>
          ) : (
            visibleContent || "rule based"
          )}
        </div>
        {canExpand && (
          <button
            className="acp-toggle-btn"
            onClick={() => setExpandedCompaction((current) => !current)}
          >
            {expandedCompaction ? "Collapse" : "Expand"}
          </button>
        )}
        <div className="acp-compaction__rule">------------------------</div>
        {hovered && !message.isStreaming && (
          <div className="acp-compaction__actions">
            <ActionBtn label={copied ? "Copied!" : "Copy"} onClick={handleCopy} />
            {onDelete && <ActionBtn label="Delete" onClick={onDelete} />}
            {onFork && <ActionBtn label="Fork" onClick={() => { void onFork(); }} />}
            {onExport && <ActionBtn label="Export" onClick={() => { void onExport(); }} />}
          </div>
        )}
      </div>
    );
  }

  return (
    <div className={isUser ? "acp-message-row acp-message-row--user" : "acp-message-row"}>
      <div
        id={`agent-message-${message.id}`}
        className={[
          "acp-message",
          isUser ? "acp-message--user" : "",
          isSystem ? "acp-message--system" : "",
          isTool ? "acp-message--tool" : "",
          isAssistant ? "acp-message--assistant" : "",
          message.pinnedForCompaction ? "acp-message--pinned" : "",
        ].filter(Boolean).join(" ")}
        onMouseEnter={() => setHovered(true)}
        onMouseLeave={() => setHovered(false)}
      >
        {isAssistant && (
          <div className="acp-message__author">
            <span>{`> ${message.authorAgentName || "assistant"}`}</span>
            {message.pinnedForCompaction && (
              <span className="acp-message__pinned">pinned</span>
            )}
          </div>
        )}

        {!isAssistant && message.pinnedForCompaction && (
          <div className="acp-message__pinned">pinned</div>
        )}

        {isAssistant && message.reasoning && (
          <details className="acp-reasoning">
            <summary>Reasoning</summary>
            <div className="acp-reasoning__body">
              <MarkdownContent content={message.reasoning} streaming={Boolean(message.isStreaming)} />
            </div>
          </details>
        )}

        {isAssistant && providerFinalResult && (
          <div style={{ marginTop: 8, display: "grid", gap: 6 }}>
            <button
              className="acp-toggle-btn acp-toggle-btn--capitalize"
              onClick={() => setExpandedProviderResult((current) => !current)}
            >
              {expandedProviderResult
                ? `Hide ${providerFinalResult.label}`
                : `Show ${providerFinalResult.label}`}
            </button>
            {expandedProviderResult && (
              <pre className="acp-pre">{providerFinalResult.prettyJson}</pre>
            )}
          </div>
        )}

        {handoffEvent ? (
          <div className="acp-handoff">
            <div className="acp-handoff__title">Thread Handoff</div>
            <div className="acp-handoff__route">
              {(handoffEvent.from_agent_name ?? "Agent")} {"->"} {(handoffEvent.to_agent_name ?? "Agent")}
            </div>
            {handoffEvent.reason && (
              <div className="acp-handoff__reason">{handoffEvent.reason}</div>
            )}
            {handoffEvent.summary && (
              <div style={{ display: "grid", gap: 6 }}>
                <button
                  className="acp-toggle-btn"
                  onClick={() => setExpandedHandoff((current) => !current)}
                >
                  {expandedHandoff ? "Collapse Summary" : "Expand Summary"}
                </button>
                {expandedHandoff && (
                  <div className="acp-handoff__summary">{handoffEvent.summary}</div>
                )}
              </div>
            )}
          </div>
        ) : isTool && message.toolName ? (
          <div style={{ display: "grid", gap: 6 }}>
            <div style={{ display: "flex", justifyContent: "space-between", gap: 8, alignItems: "center" }}>
              <span style={{ fontSize: "var(--text-xs)", color: "var(--agent)", fontWeight: 700 }}>
                Tool: {message.toolName}
              </span>
              <span className="acp-pill">{toolStatusLabel}</span>
            </div>

            {fileTarget ? (
              <ToolFileTargetView label="file" path={fileTarget.path} summaryText={message.content || undefined} />
            ) : toolDiff ? (
              <ToolDiffView sections={toolDiff} />
            ) : structuredArgDetails ? (
              <ToolStructuredValueView label="args" fields={structuredArgDetails} />
            ) : message.toolArguments ? (
              <pre className="acp-pre">
                {(() => {
                  try {
                    return JSON.stringify(JSON.parse(message.toolArguments), null, 2);
                  } catch {
                    return message.toolArguments;
                  }
                })()}
              </pre>
            ) : null}

            {!fileTarget && structuredResult ? (
              <ToolStructuredValueView label="result" fields={structuredResult} />
            ) : !fileTarget && message.content ? (
              <div className="acp-tool-result">{message.content}</div>
            ) : null}
          </div>
        ) : (
          <>
            <MarkdownContent content={displayContent} streaming={Boolean(message.isStreaming)} />
            {mediaBlocks.length > 0 && (
              <div className="acp-media">
                {mediaBlocks.map((block, index) => {
                  const source = block.data_url || block.url;
                  if (!source) return null;
                  return block.type === "image" ? (
                    <figure key={`${message.id}:media:${index}`} className="acp-media__figure">
                      <img
                        src={source}
                        alt={block.mime_type || "attached image"}
                        className="acp-media__img"
                      />
                      <figcaption className="acp-media__caption">
                        Image attachment{block.mime_type ? ` · ${block.mime_type}` : ""}
                      </figcaption>
                    </figure>
                  ) : (
                    <div key={`${message.id}:media:${index}`} className="acp-media__audio">
                      <div className="acp-media__caption">
                        Audio attachment{block.mime_type ? ` · ${block.mime_type}` : ""}
                      </div>
                      <audio controls src={source} />
                    </div>
                  );
                })}
              </div>
            )}
          </>
        )}

        {message.isStreaming && <span className="acp-message__streaming-cursor">▌</span>}

        {message.model && !isUser && (!isAssistant || hovered) && (
          <div className="acp-message__model">
            {message.provider}/{message.model}
          </div>
        )}

        {isAssistant && !message.isStreaming && (message.totalTokens > 0 || message.cost !== undefined || message.tps !== undefined) && (
          <div className="acp-message__usage">
            <span>∑ {message.totalTokens.toLocaleString()} (⇅ {message.inputTokens.toLocaleString()} / {message.outputTokens.toLocaleString()})</span>
            {message.reasoningTokens !== undefined && <span>🧠 {message.reasoningTokens}</span>}
            {message.audioTokens !== undefined && message.audioTokens > 0 && <span>🎵 {message.audioTokens}</span>}
            {message.videoTokens !== undefined && message.videoTokens > 0 && <span>🎥 {message.videoTokens}</span>}
            {message.cost !== undefined && <span>${message.cost.toFixed(6)}</span>}
            {message.tps !== undefined && Number.isFinite(message.tps) && <span>↯ {message.tps.toFixed(1)} tok/s</span>}
          </div>
        )}

        {hovered && !message.isStreaming && (
          <div className="acp-message__actions">
            <ActionBtn label={copied ? "Copied!" : "Copy"} onClick={handleCopy} />
            {message.pinnedForCompaction
              ? onUnpin && <ActionBtn label="Unpin" onClick={() => { void onUnpin(); }} />
              : onPin && <ActionBtn label="Pin" onClick={() => { void onPin(); }} />}
            {isUser && onRerun && <ActionBtn label="Rerun" onClick={onRerun} />}
            {isAssistant && onRegenerate && <ActionBtn label="Regen" onClick={onRegenerate} />}
            {isAssistant && onSpeak && <ActionBtn label={!isSpeaking ? "Speak" : isSpeechPaused ? "Resume" : "Pause"} onClick={() => { void onSpeak(); }} />}
            {(isAssistant || isTool) && onFeedback && (
              <>
                <ActionBtn
                  label={message.feedback === "up" ? "👍✓" : "👍"}
                  onClick={() => {
                    void onFeedback(message.feedback === "up" ? null : "up");
                  }}
                />
                <ActionBtn
                  label={message.feedback === "down" ? "👎✓" : "👎"}
                  onClick={() => {
                    void onFeedback(message.feedback === "down" ? null : "down");
                  }}
                />
              </>
            )}
            {onDelete && <ActionBtn label="Delete" onClick={onDelete} />}
            {onFork && <ActionBtn label="Fork" onClick={() => { void onFork(); }} />}
            {onExport && <ActionBtn label="Export" onClick={() => { void onExport(); }} />}
          </div>
        )}
      </div>
    </div>
  );
}
