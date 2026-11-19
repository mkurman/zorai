import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";
import type { CodeToolbarMenuItem } from "@/zorai/features/code/CodeToolbarMenu";
import { getBridge } from "@/lib/bridge";

export const TINT_COLORS = {
  red: "#e5484d",
  orange: "#f76b15",
  yellow: "#f5b800",
  green: "#46a758",
  teal: "#12a594",
  blue: "#0090ff",
  purple: "#8e4ec6",
  pink: "#d6409f",
} as const;

export type TintName = keyof typeof TINT_COLORS;

type TintState = {
  threads: Record<string, TintName>;
  roots: Record<string, TintName>;
  daemonSynced: boolean;
  setThreadTint: (key: string, tint: TintName | null) => void;
  setRootTint: (root: string, tint: TintName | null) => void;
  applyDaemonColors: (colors: ZoraiUiColors) => void;
};

type TintKind = "thread" | "root";

function tintMap(entries: Record<string, string> | undefined): Record<string, TintName> {
  return Object.fromEntries(Object.entries(entries ?? {}).filter((entry): entry is [string, TintName] => entry[1] in TINT_COLORS));
}

export function tintsFromDaemon(colors: ZoraiUiColors): { threads: Record<string, TintName>; roots: Record<string, TintName> } {
  return { threads: tintMap(colors.thread), roots: tintMap(colors.root) };
}

export function localTintsToUpload(local: { threads: Record<string, TintName>; roots: Record<string, TintName>; daemonSynced: boolean }, daemon: ZoraiUiColors): Array<{ kind: TintKind; key: string; tint: TintName }> {
  if (local.daemonSynced || Object.keys(daemon.thread ?? {}).length > 0 || Object.keys(daemon.root ?? {}).length > 0) return [];
  return [
    ...Object.entries(local.threads).map(([key, tint]) => ({ kind: "thread" as const, key, tint })),
    ...Object.entries(local.roots).map(([key, tint]) => ({ kind: "root" as const, key, tint })),
  ];
}

let daemonColorsUnavailable = false;

export function resetDaemonColorsAvailabilityForTest(): void {
  daemonColorsUnavailable = false;
}

function saveTint(kind: TintKind, key: string, tint: TintName | null): void {
  if (daemonColorsUnavailable) return;
  const request = getBridge()?.agentSetUiColor?.(kind, key, tint);
  void request?.then((colors) => {
    if (colors && !colors.error) useTintStore.getState().applyDaemonColors(colors);
  }).catch(() => undefined);
}

export async function syncTintsWithDaemon(): Promise<void> {
  const bridge = getBridge();
  if (daemonColorsUnavailable || !bridge?.agentListUiColors) return;
  const colors = await bridge.agentListUiColors().catch(() => null);
  if (!colors || colors.error) {
    daemonColorsUnavailable = true;
    return;
  }
  const uploads = localTintsToUpload(useTintStore.getState(), colors);
  if (uploads.length === 0) {
    useTintStore.getState().applyDaemonColors(colors);
    return;
  }
  let latest: ZoraiUiColors | null = null;
  for (const upload of uploads) {
    latest = await bridge.agentSetUiColor?.(upload.kind, upload.key, upload.tint).catch(() => null) ?? latest;
  }
  if (latest && !latest.error) useTintStore.getState().applyDaemonColors(latest);
}

function withTint(map: Record<string, TintName>, key: string, tint: TintName | null): Record<string, TintName> {
  const next = { ...map };
  if (tint) next[key] = tint;
  else delete next[key];
  return next;
}

export const useTintStore = create<TintState>()(persist(
  (set) => ({
    threads: {},
    roots: {},
    daemonSynced: false,
    setThreadTint: (key, tint) => {
      set((state) => ({ threads: withTint(state.threads, key, tint) }));
      saveTint("thread", key, tint);
    },
    setRootTint: (root, tint) => {
      set((state) => ({ roots: withTint(state.roots, root, tint) }));
      saveTint("root", root, tint);
    },
    applyDaemonColors: (colors) => set({ ...tintsFromDaemon(colors), daemonSynced: true }),
  }),
  {
    name: "zorai-tints",
    storage: createJSONStorage(() => (typeof localStorage !== "undefined" ? localStorage : { getItem: () => null, setItem: () => undefined, removeItem: () => undefined })),
    partialize: (state) => ({ threads: state.threads, roots: state.roots, daemonSynced: state.daemonSynced }),
  },
));

export function threadTintKey(thread: { id: string; daemonThreadId?: string | null }): string {
  return thread.daemonThreadId ?? thread.id;
}

export function threadTint(tints: Record<string, TintName>, thread: { id: string; daemonThreadId?: string | null }): TintName | undefined {
  return (thread.daemonThreadId ? tints[thread.daemonThreadId] : undefined) ?? tints[thread.id];
}

export function tintColor(tint: TintName | null | undefined): string | null {
  return tint && tint in TINT_COLORS ? TINT_COLORS[tint] : null;
}

export function tintMenuItem(current: TintName | null | undefined, onPick: (tint: TintName | null) => void, separatorBefore = false): CodeToolbarMenuItem {
  return {
    key: "tint",
    label: "Colour",
    separatorBefore,
    children: [
      { key: "tint-none", label: "None", checked: !current, onSelect: () => onPick(null) },
      ...(Object.keys(TINT_COLORS) as TintName[]).map((name) => ({
        key: `tint-${name}`,
        label: name[0].toUpperCase() + name.slice(1),
        checked: current === name,
        swatch: TINT_COLORS[name],
        onSelect: () => onPick(name),
      })),
    ],
  };
}

export function tintStyle(tint: TintName | null | undefined): Record<string, string> | undefined {
  const color = tintColor(tint);
  return color ? { "--zorai-tint": color } : undefined;
}
