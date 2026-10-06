import type { ChatDisplayItem } from "@/components/agent-chat-panel/chat-view/types";

export const THREAD_MESSAGE_ESTIMATED_HEIGHT_PX = 160;
export const THREAD_MESSAGE_OVERSCAN_PX = 400;
export const THREAD_MESSAGE_ROW_GAP_PX = 8;
const THREAD_MESSAGE_UNMEASURED_VIEWPORT_PX = 768;

export type ThreadMessageWindow = {
  start: number;
  end: number;
  topSpacer: number;
  bottomSpacer: number;
};

export function threadMessageItemKey(item: ChatDisplayItem): string {
  if (item.type === "message") return `message:${item.message.id}`;
  if (item.type === "toolList") return `tools:${item.key}`;
  return `tool:${item.group.key}`;
}

export function stackedThreadMessageHeight(
  heights: readonly (number | undefined)[],
  from: number,
  to: number,
  gap = THREAD_MESSAGE_ROW_GAP_PX,
  estimatedHeight = THREAD_MESSAGE_ESTIMATED_HEIGHT_PX,
): number {
  if (to <= from) return 0;
  let total = 0;
  for (let index = from; index < to; index += 1) {
    if (index > from) total += gap;
    total += rowHeight(heights, index, estimatedHeight);
  }
  return total;
}

export function resolveThreadMessageWindow(params: {
  count: number;
  heights: readonly (number | undefined)[];
  scrollTop: number;
  viewport: number;
  overscan?: number;
  gap?: number;
  estimatedHeight?: number;
  anchor?: "scroll" | "bottom";
}): ThreadMessageWindow {
  const count = Math.max(0, params.count);
  const gap = params.gap ?? THREAD_MESSAGE_ROW_GAP_PX;
  const estimatedHeight = params.estimatedHeight ?? THREAD_MESSAGE_ESTIMATED_HEIGHT_PX;
  const overscan = params.overscan ?? THREAD_MESSAGE_OVERSCAN_PX;
  const viewport = params.viewport > 0 ? params.viewport : THREAD_MESSAGE_UNMEASURED_VIEWPORT_PX;
  if (count === 0) {
    return { start: 0, end: 0, topSpacer: 0, bottomSpacer: 0 };
  }

  const total = stackedThreadMessageHeight(params.heights, 0, count, gap, estimatedHeight);
  const scrollTop = params.anchor === "bottom"
    ? Math.max(0, total - viewport)
    : Math.max(0, params.scrollTop);
  const viewStart = scrollTop - overscan;
  const viewEnd = scrollTop + viewport + overscan;

  let start = count;
  let end = count;
  let cursor = 0;
  for (let index = 0; index < count; index += 1) {
    const height = rowHeight(params.heights, index, estimatedHeight);
    const itemStart = cursor;
    const itemEnd = cursor + height;
    if (itemEnd > viewStart && itemStart < viewEnd) {
      if (start === count) start = index;
      end = index + 1;
    }
    cursor = itemEnd + gap;
  }

  if (start === count) {
    return { start: 0, end: 0, topSpacer: 0, bottomSpacer: total };
  }

  return {
    start,
    end,
    topSpacer: stackedThreadMessageHeight(params.heights, 0, start, gap, estimatedHeight),
    bottomSpacer: stackedThreadMessageHeight(params.heights, end, count, gap, estimatedHeight),
  };
}

function rowHeight(
  heights: readonly (number | undefined)[],
  index: number,
  estimatedHeight: number,
): number {
  const measured = heights[index];
  return measured != null && measured > 0 ? measured : estimatedHeight;
}
