import type { AgentMessage } from "@/lib/agentStore";

export type ThreadFindMatch = { messageId: string; occurrence: number };

const ALL_HIGHLIGHT = "zorai-find";
const CURRENT_HIGHLIGHT = "zorai-find-current";

export function buildFindPattern(query: string, regex: boolean): RegExp | "invalid" | null {
  const text = query.trim();
  if (!text) return null;
  if (regex) {
    try {
      return new RegExp(text, "gi");
    } catch {
      return "invalid";
    }
  }
  const source = text
    .replace(/[.+^${}()|[\]\\]/g, "\\$&")
    .replace(/\*/g, "\\S*")
    .replace(/\?/g, "\\S");
  return new RegExp(source, "gi");
}

function patternRanges(text: string, pattern: RegExp): Array<[number, number]> {
  const ranges: Array<[number, number]> = [];
  for (const match of text.matchAll(new RegExp(pattern.source, "gi"))) {
    if (match[0].length > 0 && match.index !== undefined) ranges.push([match.index, match.index + match[0].length]);
  }
  return ranges;
}

export function findThreadMatches(messages: AgentMessage[], pattern: RegExp | null): ThreadFindMatch[] {
  if (!pattern) return [];
  const matches: ThreadFindMatch[] = [];
  for (const message of messages) {
    if (message.role !== "user" && message.role !== "assistant") continue;
    const count = patternRanges(message.content, pattern).length;
    for (let occurrence = 0; occurrence < count; occurrence += 1) matches.push({ messageId: message.id, occurrence });
  }
  return matches;
}

function textRanges(root: Element, pattern: RegExp): Range[] {
  const ranges: Range[] = [];
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    for (const [start, end] of patternRanges(node.nodeValue ?? "", pattern)) {
      const range = document.createRange();
      range.setStart(node, start);
      range.setEnd(node, end);
      ranges.push(range);
    }
  }
  return ranges;
}

type HighlightRegistry = { set: (name: string, value: unknown) => void; delete: (name: string) => void };

function highlightRegistry(): { registry: HighlightRegistry; Highlight: new (...ranges: Range[]) => unknown } | null {
  const registry = (globalThis.CSS as unknown as { highlights?: HighlightRegistry } | undefined)?.highlights;
  const Highlight = (globalThis as unknown as { Highlight?: new (...ranges: Range[]) => unknown }).Highlight;
  return registry && Highlight ? { registry, Highlight } : null;
}

export function clearThreadFindHighlights(): void {
  const api = highlightRegistry();
  api?.registry.delete(ALL_HIGHLIGHT);
  api?.registry.delete(CURRENT_HIGHLIGHT);
}

export function paintThreadFindHighlights(scroller: HTMLElement, pattern: RegExp | null, current: ThreadFindMatch | null): Range | null {
  const api = highlightRegistry();
  if (!api || !pattern) {
    clearThreadFindHighlights();
    return null;
  }
  const all = [...scroller.querySelectorAll(".zorai-message")].flatMap((element) => textRanges(element, pattern));
  api.registry.set(ALL_HIGHLIGHT, new api.Highlight(...all));
  const element = current ? document.getElementById(`zorai-message-${current.messageId}`) : null;
  const ranges = element && scroller.contains(element) ? textRanges(element, pattern) : [];
  const range = current && ranges.length > 0 ? ranges[Math.min(current.occurrence, ranges.length - 1)] : null;
  if (range) api.registry.set(CURRENT_HIGHLIGHT, new api.Highlight(range));
  else api.registry.delete(CURRENT_HIGHLIGHT);
  return range;
}
