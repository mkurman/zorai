import { create } from "zustand";
import { getBridge } from "@/lib/bridge";

export type Breakpoint = { line: number; condition?: string; logMessage?: string; hitCondition?: string; enabled: boolean };
export type DebugFrame = { id: number; name: string; line: number; column: number; path: string | null; absolutePath: string | null };
export type DebugVariable = { name: string; value: string; type?: string; variablesReference: number };
export type DebugScope = { name: string; variablesReference: number; expensive?: boolean };
export type ConsoleLine = { category: "stdout" | "stderr" | "console" | "input" | "result" | "error"; text: string };
export type DebugStatus = "idle" | "starting" | "running" | "stopped" | "ended";

type Session = { id: string; root: string; file: string | null; config: ZoraiDebugConfiguration };

type DebugState = {
  breakpoints: Record<string, Record<string, Breakpoint[]>>;
  session: Session | null;
  companions: Session[];
  status: DebugStatus;
  stopReason: string | null;
  threadId: number | null;
  frames: DebugFrame[];
  frameId: number | null;
  scopes: DebugScope[];
  variables: Record<number, DebugVariable[]>;
  console: ConsoleLine[];
  watches: string[];
  watchValues: Record<string, string>;
  exceptionFilters: Array<{ filter: string; label: string; default?: boolean }>;
  enabledExceptionFilters: string[] | null;
  setExceptionFilter: (filter: string, enabled: boolean) => void;
  toggleBreakpoint: (root: string, path: string, line: number) => void;
  updateBreakpoint: (root: string, path: string, line: number, patch: Partial<Breakpoint> | null) => void;
  start: (root: string, file: string | null, config: ZoraiDebugConfiguration) => Promise<void>;
  startCompound: (root: string, file: string | null, configs: ZoraiDebugConfiguration[]) => Promise<void>;
  command: (command: "continue" | "next" | "stepIn" | "stepOut" | "pause") => Promise<void>;
  stop: () => Promise<void>;
  restart: () => Promise<void>;
  selectFrame: (frameId: number) => Promise<void>;
  loadVariables: (reference: number) => Promise<void>;
  evaluate: (expression: string, context?: "repl" | "watch" | "hover") => Promise<string | null>;
  addWatch: (expression: string) => void;
  removeWatch: (expression: string) => void;
};

const STORAGE_KEY = "zorai.code.breakpoints";
const WATCH_KEY = "zorai.code.debugWatches";
const EXCEPTION_KEY = "zorai.code.debugExceptionFilters";
const MAX_CONSOLE_LINES = 2000;

function readJson<T>(key: string, fallback: T): T {
  try {
    return JSON.parse(localStorage.getItem(key) ?? "null") ?? fallback;
  } catch {
    return fallback;
  }
}

function writeJson(key: string, value: unknown) {
  try { localStorage.setItem(key, JSON.stringify(value)); } catch { return; }
}

export function joinPath(root: string, path: string) {
  return `${root.replace(/[\\/]+$/, "")}/${path}`;
}

export function relativeToRoot(root: string, absolute: string | null | undefined): string | null {
  if (!absolute) return null;
  const normalizedRoot = root.replace(/\\/g, "/").replace(/\/+$/, "");
  const normalized = absolute.replace(/\\/g, "/");
  return normalized.startsWith(`${normalizedRoot}/`) ? normalized.slice(normalizedRoot.length + 1) : null;
}

export function toggleBreakpointIn(list: Breakpoint[], line: number): Breakpoint[] {
  return list.some((entry) => entry.line === line) ? list.filter((entry) => entry.line !== line) : [...list, { line, enabled: true }].sort((left, right) => left.line - right.line);
}

export function compoundMembers(compound: ZoraiDebugConfiguration, configs: ZoraiDebugConfiguration[]): ZoraiDebugConfiguration[] {
  const names = Array.isArray(compound.configurations) ? compound.configurations.filter((name): name is string => typeof name === "string") : [];
  const missing = names.filter((name) => !configs.some((config) => config.name === name && config.type !== "compound"));
  if (missing.length > 0) throw new Error(`The compound "${compound.name}" names configurations that do not exist: ${missing.join(", ")}.`);
  return names.map((name) => configs.find((config) => config.name === name && config.type !== "compound")!);
}

export function breakpointSpecs(list: Breakpoint[]): ZoraiDebugBreakpointSpec[] {
  return list.filter((entry) => entry.enabled).map(({ line, condition, logMessage, hitCondition }) => ({ line, condition, logMessage, hitCondition }));
}

let unsubscribe: (() => void) | null = null;

export const useDebugStore = create<DebugState>((set, get) => {
  const appendConsole = (lines: ConsoleLine[]) => set((state) => ({ console: [...state.console, ...lines].slice(-MAX_CONSOLE_LINES) }));
  const request = (command: string, args?: Record<string, unknown>) => {
    const session = get().session;
    const bridge = getBridge();
    if (!session || !bridge?.debugRequest) return Promise.reject(new Error("No debug session is running."));
    return bridge.debugRequest(session.id, command, args);
  };
  const syncBreakpoints = (root: string, path: string) => {
    const session = get().session;
    if (!session || get().status === "ended") return;
    for (const target of [session, ...get().companions]) {
      if (target.root !== root) continue;
      void getBridge()?.debugSetBreakpoints?.(target.id, joinPath(root, path), breakpointSpecs(get().breakpoints[root]?.[path] ?? []))?.catch(() => null);
    }
  };
  const breakpointSpecsFor = (root: string) => Object.fromEntries(Object.entries(get().breakpoints[root] ?? {}).map(([path, list]) => [path, breakpointSpecs(list)]).filter(([, list]) => list.length > 0));
  const refreshWatches = async () => {
    const values: Record<string, string> = {};
    for (const expression of get().watches) values[expression] = (await get().evaluate(expression, "watch")) ?? "not available";
    set({ watchValues: values });
  };
  const onStopped = async (body: { threadId?: number; reason?: string; description?: string }) => {
    let threadId = body.threadId ?? get().threadId;
    if (!threadId) threadId = (await request("threads").catch(() => null))?.threads?.[0]?.id ?? null;
    set({ status: "stopped", stopReason: body.description ?? body.reason ?? null, threadId, variables: {} });
    if (!threadId) return;
    const root = get().session?.root ?? "";
    const trace = await request("stackTrace", { threadId, startFrame: 0, levels: 50 }).catch(() => null);
    const frames: DebugFrame[] = (trace?.stackFrames ?? []).map((frame: any) => ({
      id: frame.id,
      name: String(frame.name ?? "?"),
      line: Number(frame.line) || 0,
      column: Number(frame.column) || 1,
      absolutePath: frame.source?.path ?? null,
      path: relativeToRoot(root, frame.source?.path),
    }));
    set({ frames });
    const top = frames.find((frame) => frame.path) ?? frames[0];
    if (top) await get().selectFrame(top.id);
  };
  const handleCompanionEvent = (companion: Session, event: string, body: any) => {
    if (event === "output" && typeof body?.output === "string" && body.category !== "telemetry") appendConsole([{ category: body.category === "stderr" ? "stderr" : "stdout", text: `[${companion.config.name}] ${body.output}` }]);
    else if (event === "stopped") {
      const focused = get().session;
      set({ session: companion, companions: [...get().companions.filter((entry) => entry.id !== companion.id), ...(focused && get().status !== "ended" ? [focused] : [])], frames: [], scopes: [], frameId: null, threadId: null });
      void onStopped(body ?? {});
    } else if (event === "terminated" || event === "sessionEnded") {
      set({ companions: get().companions.filter((entry) => entry.id !== companion.id) });
      if (event === "terminated") appendConsole([{ category: "console", text: `${companion.config.name} ended\n` }]);
    }
  };
  const handleEvent = (payload: { sessionId: string; event: string; body: any }) => {
    const companion = get().companions.find((entry) => entry.id === payload.sessionId);
    if (companion) {
      handleCompanionEvent(companion, payload.event, payload.body);
      return;
    }
    if (payload.sessionId !== get().session?.id) return;
    const { event, body } = payload;
    if (event === "capabilities") set({ exceptionFilters: Array.isArray(body?.exceptionBreakpointFilters) ? body.exceptionBreakpointFilters.map((entry: any) => ({ filter: String(entry.filter), label: String(entry.label ?? entry.filter), default: entry.default === true })) : [] });
    else if (event === "stopped") void onStopped(body ?? {});
    else if (event === "continued") set({ status: "running", frames: [], scopes: [], frameId: null, stopReason: null });
    else if (event === "output" && typeof body?.output === "string" && body.category !== "telemetry") appendConsole([{ category: body.category === "stderr" ? "stderr" : body.category === "stdout" ? "stdout" : "console", text: body.output }]);
    else if (event === "terminated" || event === "exited" || event === "sessionEnded") {
      if (event === "exited" && typeof body?.exitCode === "number") appendConsole([{ category: "console", text: `Process exited with code ${body.exitCode}\n` }]);
      const [next, ...rest] = get().companions;
      if (next) {
        if (event !== "exited") set({ session: next, companions: rest, status: "running", frames: [], scopes: [], frameId: null, threadId: null, stopReason: null });
        return;
      }
      set({ status: "ended", frames: [], scopes: [], frameId: null, threadId: null });
    }
  };

  return {
    breakpoints: readJson(STORAGE_KEY, {}),
    session: null,
    companions: [],
    status: "idle",
    stopReason: null,
    threadId: null,
    frames: [],
    frameId: null,
    scopes: [],
    variables: {},
    console: [],
    watches: readJson<string[]>(WATCH_KEY, []),
    watchValues: {},
    exceptionFilters: [],
    enabledExceptionFilters: readJson<string[] | null>(EXCEPTION_KEY, null),
    setExceptionFilter: (filter, enabled) => {
      const current = get().enabledExceptionFilters ?? get().exceptionFilters.filter((entry) => entry.default).map((entry) => entry.filter);
      const next = enabled ? [...new Set([...current, filter])] : current.filter((entry) => entry !== filter);
      writeJson(EXCEPTION_KEY, next);
      set({ enabledExceptionFilters: next });
      if (get().session && get().status !== "ended") void request("setExceptionBreakpoints", { filters: next }).catch(() => null);
    },
    toggleBreakpoint: (root, path, line) => {
      set((state) => {
        const byPath = { ...(state.breakpoints[root] ?? {}), [path]: toggleBreakpointIn(state.breakpoints[root]?.[path] ?? [], line) };
        const breakpoints = { ...state.breakpoints, [root]: byPath };
        writeJson(STORAGE_KEY, breakpoints);
        return { breakpoints };
      });
      syncBreakpoints(root, path);
    },
    updateBreakpoint: (root, path, line, patch) => {
      set((state) => {
        const current = state.breakpoints[root]?.[path] ?? [];
        const next = patch === null ? current.filter((entry) => entry.line !== line) : current.map((entry) => (entry.line === line ? { ...entry, ...patch } : entry));
        const breakpoints = { ...state.breakpoints, [root]: { ...(state.breakpoints[root] ?? {}), [path]: next } };
        writeJson(STORAGE_KEY, breakpoints);
        return { breakpoints };
      });
      syncBreakpoints(root, path);
    },
    start: async (root, file, config) => {
      const bridge = getBridge();
      if (!bridge?.debugStart) throw new Error("Debugging needs the desktop app.");
      if (get().session && get().status !== "ended") await get().stop();
      unsubscribe ??= (bridge.onDebugEvent?.(handleEvent) as (() => void) | undefined) ?? null;
      const specs = breakpointSpecsFor(root);
      set({ companions: [], status: "starting", console: [{ category: "console", text: `Starting ${config.name}…\n` }], frames: [], scopes: [], variables: {}, frameId: null, threadId: null, stopReason: null });
      try {
        const { sessionId } = await bridge.debugStart(root, file, config, specs, get().enabledExceptionFilters);
        set({ session: { id: sessionId, root, file, config }, status: get().status === "starting" ? "running" : get().status });
      } catch (reason) {
        appendConsole([{ category: "error", text: `${reason instanceof Error ? reason.message : String(reason)}\n` }]);
        set({ status: "ended" });
        throw reason;
      }
    },
    startCompound: async (root, file, configs) => {
      const bridge = getBridge();
      const [first, ...rest] = configs;
      if (!first) return;
      await get().start(root, file, first);
      if (!bridge?.debugStart) return;
      for (const config of rest) {
        appendConsole([{ category: "console", text: `Starting ${config.name}…\n` }]);
        try {
          const { sessionId } = await bridge.debugStart(root, file, config, breakpointSpecsFor(root), get().enabledExceptionFilters);
          set({ companions: [...get().companions, { id: sessionId, root, file, config }] });
        } catch (reason) {
          appendConsole([{ category: "error", text: `${config.name}: ${reason instanceof Error ? reason.message : String(reason)}\n` }]);
        }
      }
    },
    command: async (command) => {
      const threadId = get().threadId;
      if (command !== "pause") set({ status: "running" });
      await request(command, { threadId: threadId ?? 1 }).catch((reason) => appendConsole([{ category: "error", text: `${reason.message}\n` }]));
    },
    stop: async () => {
      const sessions = [get().session, ...get().companions].filter((entry): entry is Session => Boolean(entry));
      set({ companions: [] });
      await Promise.all(sessions.map((entry) => getBridge()?.debugStop?.(entry.id).catch(() => false)));
      set({ status: "ended", frames: [], scopes: [], frameId: null, threadId: null });
    },
    restart: async () => {
      const session = get().session;
      if (!session) return;
      const configs = [session.config, ...get().companions.map((entry) => entry.config)];
      await get().stop();
      if (configs.length > 1) await get().startCompound(session.root, session.file, configs);
      else await get().start(session.root, session.file, session.config);
    },
    selectFrame: async (frameId) => {
      set({ frameId });
      const scopes: DebugScope[] = ((await request("scopes", { frameId }).catch(() => null))?.scopes ?? []).map((scope: any) => ({ name: String(scope.name), variablesReference: Number(scope.variablesReference) || 0, expensive: scope.expensive === true }));
      set({ scopes });
      const first = scopes.find((scope) => !scope.expensive && scope.variablesReference > 0);
      if (first) await get().loadVariables(first.variablesReference);
      await refreshWatches();
    },
    loadVariables: async (reference) => {
      const variables: DebugVariable[] = ((await request("variables", { variablesReference: reference }).catch(() => null))?.variables ?? []).map((variable: any) => ({
        name: String(variable.name),
        value: String(variable.value ?? ""),
        type: typeof variable.type === "string" ? variable.type : undefined,
        variablesReference: Number(variable.variablesReference) || 0,
      }));
      set((state) => ({ variables: { ...state.variables, [reference]: variables } }));
    },
    evaluate: async (expression, context = "repl") => {
      if (context === "repl") appendConsole([{ category: "input", text: `${expression}\n` }]);
      try {
        const result = await request("evaluate", { expression, frameId: get().frameId ?? undefined, context });
        const text = String(result?.result ?? "");
        if (context === "repl") appendConsole([{ category: "result", text: `${text}\n` }]);
        return text;
      } catch (reason) {
        if (context === "repl") appendConsole([{ category: "error", text: `${reason instanceof Error ? reason.message : String(reason)}\n` }]);
        return null;
      }
    },
    addWatch: (expression) => {
      const trimmed = expression.trim();
      if (!trimmed || get().watches.includes(trimmed)) return;
      const watches = [...get().watches, trimmed];
      writeJson(WATCH_KEY, watches);
      set({ watches });
      if (get().status === "stopped") void refreshWatches();
    },
    removeWatch: (expression) => {
      const watches = get().watches.filter((entry) => entry !== expression);
      writeJson(WATCH_KEY, watches);
      set({ watches });
    },
  };
});
