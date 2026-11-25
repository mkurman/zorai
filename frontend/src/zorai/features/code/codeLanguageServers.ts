import { create } from "zustand";
import { getBridge } from "@/lib/bridge";

type LanguageServerState = {
  servers: ZoraiLanguageServerStatus[];
  installing: Record<string, boolean>;
  errors: Record<string, string>;
  revision: number;
  refresh: () => Promise<void>;
  install: (id: string) => Promise<boolean>;
};

export const useLanguageServerStore = create<LanguageServerState>((set, get) => ({
  servers: [],
  installing: {},
  errors: {},
  revision: 0,
  refresh: async () => {
    const servers = await getBridge()?.workspaceLspServers?.().catch(() => null);
    if (servers) set({ servers });
  },
  install: async (id) => {
    const bridge = getBridge();
    if (!bridge?.workspaceLspInstall || get().installing[id]) return false;
    set((state) => ({ installing: { ...state.installing, [id]: true }, errors: { ...state.errors, [id]: "" } }));
    const result = await bridge.workspaceLspInstall(id).catch((reason) => ({ ok: false, output: reason instanceof Error ? reason.message : String(reason) }));
    set((state) => ({
      installing: { ...state.installing, [id]: false },
      errors: { ...state.errors, [id]: result.ok ? "" : result.output.trim().split(/\r?\n/).slice(-6).join("\n") || "Installation failed." },
      revision: result.ok ? state.revision + 1 : state.revision,
    }));
    await get().refresh();
    return result.ok;
  },
}));

const PROMPTED_KEY = "zorai.code.lspInstallPrompted";

export function claimLanguageServerPrompt(serverId: string): boolean {
  try {
    const prompted: string[] = JSON.parse(localStorage.getItem(PROMPTED_KEY) ?? "[]");
    if (prompted.includes(serverId)) return false;
    localStorage.setItem(PROMPTED_KEY, JSON.stringify([...prompted, serverId]));
    return true;
  } catch {
    return false;
  }
}
