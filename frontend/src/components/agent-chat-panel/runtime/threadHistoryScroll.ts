export const THREAD_HISTORY_SCROLL_THRESHOLD_PX = 24;
export const THREAD_HISTORY_OLDER_LOAD_DEBOUNCE_MS = 200;
export const THREAD_HISTORY_OLDER_LOAD_COOLDOWN_MS = 400;

export type ThreadHistoryScrollAction = "load-older" | "none";

let followBottom = true;
let ignoreScroll = false;
let olderLoadInFlight = false;
let olderHistoryExhausted = false;
let olderLoadCooldownUntil = 0;
let scheduledOlderLoad: ReturnType<typeof setTimeout> | null = null;
let paginationGeneration = 0;

export function setFollowThreadHistoryBottom(follow: boolean): void {
  followBottom = follow;
}

export function shouldFollowThreadHistoryBottom(): boolean {
  return followBottom;
}

export function beginProgrammaticThreadHistoryScroll(): void {
  ignoreScroll = true;
}

export function endProgrammaticThreadHistoryScroll(): void {
  afterPaint(() => {
    ignoreScroll = false;
  });
}

export function shouldIgnoreThreadHistoryScroll(): boolean {
  return ignoreScroll;
}

export function resetThreadHistoryPagination(): void {
  paginationGeneration += 1;
  ignoreScroll = false;
  olderLoadInFlight = false;
  olderHistoryExhausted = false;
  olderLoadCooldownUntil = 0;
  cancelScheduledOlderThreadHistoryLoad();
}

export function resetThreadHistoryScrollStateForTest(): void {
  followBottom = true;
  ignoreScroll = false;
  resetThreadHistoryPagination();
}

export function threadHasOlderHistory(
  thread: {
    loadedMessageStart?: number | null;
    loadedMessageEnd?: number | null;
    messageCount?: number;
  } | null | undefined,
  loadedMessageCount = 0,
): boolean {
  if ((thread?.loadedMessageStart ?? 0) > 0) {
    return true;
  }
  const total = thread?.messageCount ?? 0;
  if (total <= 0) {
    return false;
  }
  if (loadedMessageCount > 0 && loadedMessageCount < total) {
    return true;
  }
  const loadedEnd = thread?.loadedMessageEnd ?? loadedMessageCount;
  return loadedEnd < total;
}

export function resolveOlderThreadPageMessageOffset(params: {
  loadedMessageStart?: number | null;
  loadedMessageEnd?: number | null;
  messageCount?: number;
  currentMessageCount: number;
}): number | null {
  const total = params.messageCount ?? params.currentMessageCount;
  if (total <= 0) {
    return null;
  }
  const loadedEnd = params.loadedMessageEnd ?? params.currentMessageCount;
  const hasOlder = (params.loadedMessageStart ?? 0) > 0
    || loadedEnd < total
    || params.currentMessageCount < total;
  if (!hasOlder) {
    return null;
  }
  const effectiveStart = (params.loadedMessageStart ?? 0) > 0
    ? params.loadedMessageStart as number
    : Math.max(0, total - params.currentMessageCount);
  return Math.max(0, total - effectiveStart);
}

export function resolveThreadHistoryScrollAction(params: {
  scrollTop: number;
  scrollHeight: number;
  clientHeight: number;
  thresholdPx?: number;
  hasOlderHistory?: boolean;
}): ThreadHistoryScrollAction {
  const threshold = params.thresholdPx ?? THREAD_HISTORY_SCROLL_THRESHOLD_PX;
  const atTop = params.scrollTop <= threshold;
  const distanceFromBottom = params.scrollHeight - params.scrollTop - params.clientHeight;
  const atBottom = distanceFromBottom <= threshold;

  if (atTop && atBottom) {
    followBottom = true;
    return params.hasOlderHistory ? "load-older" : "none";
  }
  if (atTop) {
    followBottom = false;
    return olderHistoryExhausted ? "none" : "load-older";
  }
  olderHistoryExhausted = false;
  if (atBottom) {
    followBottom = true;
    return "none";
  }
  followBottom = false;
  return "none";
}

export function consumeThreadHistoryScroll(options: {
  scroller: HTMLElement;
  loadOlder?: () => Promise<boolean>;
  hasOlderHistory?: boolean;
  onFollowBottomChange?: (follow: boolean) => void;
  now?: number;
}): void {
  if (shouldIgnoreThreadHistoryScroll() || olderLoadInFlight) return;
  const action = resolveThreadHistoryScrollAction({
    scrollTop: options.scroller.scrollTop,
    scrollHeight: options.scroller.scrollHeight,
    clientHeight: options.scroller.clientHeight,
    hasOlderHistory: options.hasOlderHistory,
  });
  options.onFollowBottomChange?.(shouldFollowThreadHistoryBottom());
  if (action !== "load-older") {
    cancelScheduledOlderThreadHistoryLoad();
  }
  if (action === "load-older") {
    scheduleOlderThreadHistoryLoad(options.scroller, options.loadOlder, options.now);
  }
}

function scheduleOlderThreadHistoryLoad(
  scroller: HTMLElement,
  loadOlder: (() => Promise<boolean>) | undefined,
  now = Date.now(),
): void {
  if (!loadOlder || olderLoadInFlight || olderHistoryExhausted) return;
  if (now < olderLoadCooldownUntil) return;
  if (scheduledOlderLoad != null) return;
  scheduledOlderLoad = setTimeout(() => {
    scheduledOlderLoad = null;
    void runOlderThreadHistoryLoad(scroller, loadOlder);
  }, THREAD_HISTORY_OLDER_LOAD_DEBOUNCE_MS);
}

async function runOlderThreadHistoryLoad(
  scroller: HTMLElement,
  loadOlder: () => Promise<boolean>,
): Promise<void> {
  if (olderLoadInFlight || olderHistoryExhausted) return;
  if (scroller.scrollTop > THREAD_HISTORY_SCROLL_THRESHOLD_PX) return;

  olderLoadInFlight = true;
  const loadGeneration = paginationGeneration;
  beginProgrammaticThreadHistoryScroll();
  const previousHeight = scroller.scrollHeight;
  const previousTop = scroller.scrollTop;
  let loaded = false;
  try {
    loaded = await loadOlder();
  } finally {
    if (loadGeneration === paginationGeneration) {
      olderLoadInFlight = false;
    }
  }
  if (loadGeneration !== paginationGeneration) return;
  if (!loaded) {
    olderHistoryExhausted = true;
    endProgrammaticThreadHistoryScroll();
    return;
  }
  olderLoadCooldownUntil = Date.now() + THREAD_HISTORY_OLDER_LOAD_COOLDOWN_MS;
  afterLayout(() => {
    if (shouldFollowThreadHistoryBottom()) {
      scroller.scrollTop = scroller.scrollHeight;
    } else {
      scroller.scrollTop = scroller.scrollHeight - previousHeight + previousTop;
    }
    endProgrammaticThreadHistoryScroll();
    const stillUnscrollable = scroller.scrollHeight <= scroller.clientHeight + THREAD_HISTORY_SCROLL_THRESHOLD_PX;
    if (stillUnscrollable || scroller.scrollTop <= THREAD_HISTORY_SCROLL_THRESHOLD_PX) {
      scheduleOlderThreadHistoryLoad(scroller, loadOlder, olderLoadCooldownUntil);
    }
  });
}

export function fillThreadHistoryIfUnscrollable(options: {
  scroller: HTMLElement | null | undefined;
  loadOlder?: () => Promise<boolean>;
  hasOlderHistory: boolean;
}): void {
  const scroller = options.scroller;
  if (!scroller || !options.hasOlderHistory || !options.loadOlder) return;
  if (olderLoadInFlight || olderHistoryExhausted) return;
  if (scroller.clientHeight <= 0) return;
  const unscrollable = scroller.scrollHeight <= scroller.clientHeight + THREAD_HISTORY_SCROLL_THRESHOLD_PX;
  if (!unscrollable) return;
  scheduleOlderThreadHistoryLoad(scroller, options.loadOlder);
}

function cancelScheduledOlderThreadHistoryLoad(): void {
  if (scheduledOlderLoad == null) return;
  clearTimeout(scheduledOlderLoad);
  scheduledOlderLoad = null;
}

function afterPaint(fn: () => void): void {
  if (typeof requestAnimationFrame === "function") {
    requestAnimationFrame(fn);
    return;
  }
  setTimeout(fn, 0);
}

function afterLayout(fn: () => void): void {
  afterPaint(() => afterPaint(fn));
}
