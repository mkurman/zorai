import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";
import type { CodeToolbarMenuItem } from "@/zorai/features/code/CodeToolbarMenu";

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
  setThreadTint: (key: string, tint: TintName | null) => void;
  setRootTint: (root: string, tint: TintName | null) => void;
};

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
    setThreadTint: (key, tint) => set((state) => ({ threads: withTint(state.threads, key, tint) })),
    setRootTint: (root, tint) => set((state) => ({ roots: withTint(state.roots, root, tint) })),
  }),
  {
    name: "zorai-tints",
    storage: createJSONStorage(() => (typeof localStorage !== "undefined" ? localStorage : { getItem: () => null, setItem: () => undefined, removeItem: () => undefined })),
    partialize: (state) => ({ threads: state.threads, roots: state.roots }),
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
