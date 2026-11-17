import { APPROX_CHARS_PER_TOKEN } from "@/lib/agent-client/types";

export type StatisticsContentRow = { label: string; chars: number; tokens: number };

export function statisticsContentRows(content: ZoraiAgentStatisticsContent | undefined): StatisticsContentRow[] {
  if (!content) return [];
  const rows = [
    { label: "Tool results", chars: content.tool_result_chars },
    { label: "User messages", chars: content.user_chars },
    { label: "System prompts", chars: content.system_chars },
    { label: "Assistant answers", chars: content.assistant_chars },
    { label: "Assistant reasoning", chars: content.reasoning_chars },
  ].map((row) => ({ ...row, chars: Math.max(0, row.chars || 0) }));
  const total = rows.reduce((sum, row) => sum + row.chars, 0);
  return [...rows, { label: "Total stored", chars: total }].map((row) => ({
    ...row,
    tokens: Math.round(row.chars / APPROX_CHARS_PER_TOKEN),
  }));
}
