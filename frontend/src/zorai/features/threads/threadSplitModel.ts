import { create } from "zustand";

export const MAX_SPLIT_PANES = 4;
export const THREAD_DRAG_TYPE = "application/x-zorai-thread";

export type ThreadSplitLayout = "columns" | "rows" | "grid" | "main-left" | "main-right" | "main-top" | "main-bottom";
export type ThreadSplitDropZone = "left" | "right" | "top" | "bottom";
export type ThreadSplitNode =
  | { kind: "pane"; id: string }
  | { kind: "split"; direction: "row" | "column"; children: ThreadSplitNode[]; sizes?: number[] };

const LAYOUT_LABELS: Record<number, Array<{ layout: ThreadSplitLayout; label: string }>> = {
  2: [
    { layout: "columns", label: "Side by side (2×1)" },
    { layout: "rows", label: "Stacked (1×2)" },
  ],
  3: [
    { layout: "columns", label: "Three columns (3×1)" },
    { layout: "rows", label: "Three rows (1×3)" },
    { layout: "main-left", label: "One large left, two stacked" },
    { layout: "main-right", label: "Two stacked, one large right" },
    { layout: "main-top", label: "One wide on top, two below" },
    { layout: "main-bottom", label: "Two on top, one wide below" },
  ],
  4: [
    { layout: "grid", label: "Grid (2×2)" },
    { layout: "columns", label: "Four columns (4×1)" },
    { layout: "rows", label: "Four rows (1×4)" },
  ],
};

export function splitLayoutsFor(count: number): Array<{ layout: ThreadSplitLayout; label: string }> {
  return LAYOUT_LABELS[count] ?? [];
}

const pane = (id: string): ThreadSplitNode => ({ kind: "pane", id });
const row = (children: ThreadSplitNode[]): ThreadSplitNode => ({ kind: "split", direction: "row", children });
const column = (children: ThreadSplitNode[]): ThreadSplitNode => ({ kind: "split", direction: "column", children });

export function splitLayoutTree(layout: ThreadSplitLayout, ids: readonly string[]): ThreadSplitNode | null {
  const panes = [...new Set(ids)].slice(0, MAX_SPLIT_PANES).map(pane);
  if (panes.length < 2) return null;
  const [a, b, c, d] = panes;
  if (layout === "rows") return column(panes);
  if (layout === "grid" && d) return column([row([a, b]), row([c, d])]);
  if (panes.length === 3) {
    if (layout === "main-left") return row([a, column([b, c])]);
    if (layout === "main-right") return row([column([a, b]), c]);
    if (layout === "main-top") return column([a, row([b, c])]);
    if (layout === "main-bottom") return column([row([a, b]), c]);
  }
  return row(panes);
}

export function splitPaneIds(node: ThreadSplitNode | null): string[] {
  if (!node) return [];
  return node.kind === "pane" ? [node.id] : node.children.flatMap(splitPaneIds);
}

function normalize(node: ThreadSplitNode | null): ThreadSplitNode | null {
  if (!node || node.kind === "pane") return node;
  const children = node.children
    .map(normalize)
    .filter((child): child is ThreadSplitNode => Boolean(child))
    .flatMap((child) => (child.kind === "split" && child.direction === node.direction ? child.children : [child]));
  if (children.length === 0) return null;
  if (children.length === 1) return children[0];
  const { sizes, ...rest } = node;
  return sizes && sizes.length === children.length ? { ...rest, children, sizes } : { ...rest, children };
}

export function splitChildSizes(node: Extract<ThreadSplitNode, { kind: "split" }>): number[] {
  const sizes = node.sizes;
  const valid = Array.isArray(sizes)
    && sizes.length === node.children.length
    && sizes.every((size) => typeof size === "number" && Number.isFinite(size) && size > 0);
  return valid ? sizes : node.children.map(() => 1);
}

export function resizeSplitChildren(sizes: readonly number[], index: number, fraction: number, minFraction = 0.12): number[] {
  const total = sizes.reduce((sum, size) => sum + size, 0);
  const pair = sizes[index] + sizes[index + 1];
  const before = sizes.slice(0, index).reduce((sum, size) => sum + size, 0);
  const min = Math.min(minFraction * total, pair / 2);
  const first = Math.min(pair - min, Math.max(min, fraction * total - before));
  const next = [...sizes];
  next[index] = first;
  next[index + 1] = pair - first;
  return next;
}

export function setSplitSizesAt(root: ThreadSplitNode | null, path: readonly number[], sizes: number[]): ThreadSplitNode | null {
  if (!root || root.kind === "pane") return root;
  if (path.length === 0) return sizes.length === root.children.length ? { ...root, sizes } : root;
  const [head, ...rest] = path;
  return { ...root, children: root.children.map((child, index) => (index === head ? setSplitSizesAt(child, rest, sizes) ?? child : child)) };
}

export function mapSplitPaneIds(node: ThreadSplitNode | null, map: (id: string) => string | null): ThreadSplitNode | null {
  if (!node) return null;
  if (node.kind === "pane") {
    const id = map(node.id);
    return id ? pane(id) : null;
  }
  return normalize({ ...node, children: node.children.map((child) => mapSplitPaneIds(child, map)).filter((child): child is ThreadSplitNode => Boolean(child)) });
}

export function removeSplitPaneNode(node: ThreadSplitNode | null, id: string): ThreadSplitNode | null {
  if (!node) return null;
  if (node.kind === "pane") return node.id === id ? null : node;
  return normalize({ ...node, children: node.children.map((child) => removeSplitPaneNode(child, id)).filter((child): child is ThreadSplitNode => Boolean(child)) });
}

export function replaceSplitPaneNode(node: ThreadSplitNode, id: string, nextId: string): ThreadSplitNode {
  if (node.kind === "pane") return node.id === id ? pane(nextId) : node;
  return { ...node, children: node.children.map((child) => replaceSplitPaneNode(child, id, nextId)) };
}

export function insertSplitPane(root: ThreadSplitNode | null, targetId: string, newId: string, zone: ThreadSplitDropZone): ThreadSplitNode | null {
  if (targetId === newId) return root;
  const base = removeSplitPaneNode(root, newId) ?? pane(targetId);
  if (!splitPaneIds(base).includes(targetId) || splitPaneIds(base).length >= MAX_SPLIT_PANES) return root;
  const direction = zone === "left" || zone === "right" ? "row" : "column";
  const before = zone === "left" || zone === "top";
  const place = (node: ThreadSplitNode): ThreadSplitNode => {
    if (node.kind === "pane") {
      if (node.id !== targetId) return node;
      return { kind: "split", direction, children: before ? [pane(newId), node] : [node, pane(newId)] };
    }
    const index = node.children.findIndex((child) => child.kind === "pane" && child.id === targetId);
    if (index >= 0 && node.direction === direction) {
      const children = [...node.children];
      children.splice(before ? index : index + 1, 0, pane(newId));
      return { ...node, children };
    }
    return { ...node, children: node.children.map(place) };
  };
  return normalize(place(base));
}

export function insertSplitPaneAtEdge(root: ThreadSplitNode | null, newId: string, zone: ThreadSplitDropZone): ThreadSplitNode | null {
  const base = removeSplitPaneNode(root, newId);
  if (!base || splitPaneIds(base).length >= MAX_SPLIT_PANES) return root;
  const direction = zone === "left" || zone === "right" ? "row" : "column";
  const before = zone === "left" || zone === "top";
  return normalize({ kind: "split", direction, children: before ? [pane(newId), base] : [base, pane(newId)] });
}

export function splitEdgeDropZone(rect: { left: number; top: number; width: number; height: number }, x: number, y: number, band = 36): ThreadSplitDropZone | null {
  const distances: Array<[ThreadSplitDropZone, number]> = [
    ["left", x - rect.left],
    ["right", rect.left + rect.width - x],
    ["top", y - rect.top],
    ["bottom", rect.top + rect.height - y],
  ];
  const [zone, distance] = distances.reduce((best, entry) => (entry[1] < best[1] ? entry : best));
  return distance >= 0 && distance <= band ? zone : null;
}

export function nextSplitCandidate({ activeId, shownIds, openTabIds, recentIds }: {
  activeId: string | null;
  shownIds: readonly string[];
  openTabIds: readonly string[];
  recentIds: readonly string[];
}): string | null {
  if (!activeId || shownIds.length >= MAX_SPLIT_PANES) return null;
  return [...openTabIds, ...recentIds].find((id) => id !== activeId && !shownIds.includes(id)) ?? null;
}

export function splitDropZone(rect: { left: number; top: number; width: number; height: number }, x: number, y: number): ThreadSplitDropZone {
  const dx = (x - rect.left) / Math.max(rect.width, 1) - 0.5;
  const dy = (y - rect.top) / Math.max(rect.height, 1) - 0.5;
  if (Math.abs(dx) >= Math.abs(dy)) return dx < 0 ? "left" : "right";
  return dy < 0 ? "top" : "bottom";
}

type ThreadSplitState = {
  root: ThreadSplitNode | null;
  focusedId: string | null;
  drafts: Record<string, string>;
  setLayout: (ids: string[], layout: ThreadSplitLayout) => void;
  insert: (targetId: string, newId: string, zone: ThreadSplitDropZone) => void;
  insertAtEdge: (newId: string, zone: ThreadSplitDropZone) => void;
  setSizes: (path: number[], sizes: number[]) => void;
  setFocused: (id: string) => void;
  replaceFocused: (id: string) => void;
  removePane: (id: string) => void;
  clear: () => void;
};

export const splitOrNull = (root: ThreadSplitNode | null) => (splitPaneIds(root).length >= 2 ? root : null);

export const useThreadSplitStore = create<ThreadSplitState>((set) => ({
  root: null,
  focusedId: null,
  drafts: {},
  setLayout: (ids, layout) => set({ root: splitLayoutTree(layout, ids) }),
  insert: (targetId, newId, zone) => set((state) => ({ root: splitOrNull(insertSplitPane(state.root, targetId, newId, zone)) })),
  setSizes: (path, sizes) => set((state) => ({ root: setSplitSizesAt(state.root, path, sizes) })),
  insertAtEdge: (newId, zone) => set((state) => ({ root: splitOrNull(insertSplitPaneAtEdge(state.root, newId, zone)) })),
  setFocused: (id) => set((state) => (splitPaneIds(state.root).includes(id) && state.focusedId !== id ? { focusedId: id } : state)),
  replaceFocused: (id) => set((state) => {
    const ids = splitPaneIds(state.root);
    if (!state.root || ids.length < 2 || ids.includes(id)) return state;
    const target = state.focusedId && ids.includes(state.focusedId) ? state.focusedId : ids[ids.length - 1];
    return { root: replaceSplitPaneNode(state.root, target, id), focusedId: id };
  }),
  removePane: (id) => set((state) => {
    const root = splitOrNull(removeSplitPaneNode(state.root, id));
    return { root, focusedId: root && state.focusedId !== id ? state.focusedId : null };
  }),
  clear: () => set({ root: null, focusedId: null, drafts: {} }),
}));
