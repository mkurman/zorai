import type React from "react";
import type { AgentMessage, AgentThread, AgentTodoItem } from "../../../lib/agentStore";
import type { WelesReviewMeta } from "../../../lib/agentTools";
import type { WelesHealthState } from "../welesHealthPresentation";

import type { AgentContentBlock } from "../../../lib/agentStore/types";

export type ComposerAttachment = {
  id: string;
  name: string;
  size: number;
  kind: "image" | "audio" | "text" | "file";
  mimeType: string;
  path?: string;
  dataUrl?: string;
  previewDataUrl?: string;
  textContent?: string;
};

export type SendMessagePayload = {
  text: string;
  contentBlocksJson?: string | null;
  localContentBlocks?: AgentContentBlock[];
};

export type ChatViewProps = {
  messages: AgentMessage[];
  todos: AgentTodoItem[];
  inputRef: React.RefObject<HTMLTextAreaElement | null>;
  onKeyDown: (event: React.KeyboardEvent) => void;
  agentSettings: {
    enabled: boolean;
    chatFontFamily: string;
    reasoning_effort: string;
    audio_stt_enabled: boolean;
    audio_stt_provider: string;
    audio_stt_model: string;
    audio_stt_language: string;
    audio_tts_enabled: boolean;
    audio_tts_provider: string;
    audio_tts_model: string;
    audio_tts_voice: string;
    audio_tts_auto_speak: boolean;
  };
  isStreamingResponse: boolean;
  activeThread: AgentThread | undefined;
  messagesEndRef: React.RefObject<HTMLDivElement | null>;
  onLoadOlderMessages?: () => Promise<boolean>;
  onTrimMessagesToLatestWindow?: () => boolean;
  onSendMessage: (payload: SendMessagePayload) => void;
  onSendParticipantSuggestion: (threadId: string, suggestionId: string, forceSend?: boolean) => void | Promise<void>;
  onDismissParticipantSuggestion: (threadId: string, suggestionId: string) => void | Promise<void>;
  onStopStreaming: () => void;
  onDeleteMessage?: (messageId: string) => void;
  onForkMessage?: (messageId: string) => void | Promise<void>;
  onExportThread?: (messageId: string) => void | Promise<void>;
  onPinMessage?: (messageId: string) => Promise<ZoraiThreadMessagePinResult | null>;
  onUnpinMessage?: (messageId: string) => Promise<ZoraiThreadMessagePinResult | null>;
  onFeedbackMessage?: (messageId: string, reaction: "up" | "down" | null) => void | Promise<void>;
  onUpdateReasoningEffort: (value: string) => void;
  canStartGoalRun: boolean;
  onStartGoalRun: (text: string) => Promise<boolean>;
  welesHealth?: WelesHealthState | null;
};

export type ToolEventGroup = {
  key: string;
  toolCallId: string;
  toolName: string;
  toolArguments: string;
  status: "requested" | "executing" | "done" | "error";
  resultContent: string;
  createdAt: number;
  welesReview?: WelesReviewMeta;
};

export type ToolEventAttribution = {
  authorAgentName?: string;
  createdAt: number;
};

export type ChatDisplayItem =
  | { type: "message"; message: AgentMessage }
  | { type: "tool"; group: ToolEventGroup }
  | { type: "toolList"; key: string; groups: ToolEventGroup[]; attribution?: ToolEventAttribution };
