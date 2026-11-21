import { create } from "zustand";

export type ToastKind = "error" | "info" | "success";

export interface ToastAction {
  label: string;
  run: () => void;
}

export interface ToastOptions {
  actions?: ToastAction[];
  durationMs?: number;
}

export interface ToastItem {
  id: string;
  kind: ToastKind;
  message: string;
  createdAt: number;
  actions?: ToastAction[];
}

interface ToastStoreState {
  toasts: ToastItem[];
  pushToast: (message: string, kind?: ToastKind, options?: ToastOptions) => void;
  dismissToast: (id: string) => void;
}

const TOAST_AUTO_DISMISS_MS = 6000;
let toastSerial = 0;

export const useToastStore = create<ToastStoreState>((set, get) => ({
  toasts: [],
  pushToast: (message, kind = "error", options) => {
    const trimmed = message.trim();
    if (!trimmed) return;
    toastSerial += 1;
    const id = "toast_" + String(Date.now()) + "_" + String(toastSerial);
    set((state) => ({
      toasts: [...state.toasts.slice(-4), { id, kind, message: trimmed, createdAt: Date.now(), actions: options?.actions }],
    }));
    setTimeout(() => {
      get().dismissToast(id);
    }, options?.durationMs ?? TOAST_AUTO_DISMISS_MS);
  },
  dismissToast: (id) => {
    set((state) => ({ toasts: state.toasts.filter((toast) => toast.id !== id) }));
  },
}));

/** Convenience helper for non-React call sites. */
export function pushToast(message: string, kind: ToastKind = "error", options?: ToastOptions): void {
  useToastStore.getState().pushToast(message, kind, options);
}
