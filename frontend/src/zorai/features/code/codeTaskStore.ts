import { create } from "zustand";
import { getBridge } from "@/lib/bridge";

export type TaskRun = { runId: string; label: string; command: string; output: string; status: "running" | "succeeded" | "failed"; code: number | null };

type TaskState = {
  root: string | null;
  tasks: ZoraiTask[];
  runs: TaskRun[];
  selectedRunId: string | null;
  lastTaskLabel: string | null;
  problems: Record<string, ZoraiTaskProblem[]>;
  discover: (root: string) => Promise<ZoraiTask[]>;
  run: (root: string, task: ZoraiTask) => Promise<TaskRun | null>;
  stop: (runId: string) => void;
  select: (runId: string) => void;
  clearProblems: () => void;
};

const MAX_RUNS = 20;
const MAX_OUTPUT = 500_000;
let unsubscribe: (() => void) | null = null;
const waiters = new Map<string, (run: TaskRun) => void>();

export function defaultBuildTask(tasks: ZoraiTask[]): ZoraiTask | null {
  const build = tasks.filter((task) => task.group === "build");
  return build.find((task) => task.isDefault) ?? (build.length === 1 ? build[0] : null);
}

export function taskOrder(tasks: ZoraiTask[], target: ZoraiTask): ZoraiTask[] {
  const byLabel = new Map(tasks.map((task) => [task.label, task]));
  const ordered: ZoraiTask[] = [];
  const visiting = new Set<string>();
  const visit = (task: ZoraiTask) => {
    if (ordered.includes(task) || visiting.has(task.label)) return;
    visiting.add(task.label);
    for (const label of task.dependsOn) {
      const dependency = byLabel.get(label);
      if (dependency) visit(dependency);
    }
    ordered.push(task);
  };
  visit(target);
  return ordered;
}

export const useCodeTaskStore = create<TaskState>((set, get) => {
  const handle = (payload: Parameters<Parameters<NonNullable<NonNullable<ReturnType<typeof getBridge>>["onTaskEvent"]>>[0]>[0]) => {
    if (payload.type === "start") return;
    set((state) => ({
      runs: state.runs.map((run) => {
        if (run.runId !== payload.runId) return run;
        if (payload.type === "output") return { ...run, output: `${run.output}${payload.text ?? ""}`.slice(-MAX_OUTPUT) };
        const finished: TaskRun = { ...run, status: payload.code === 0 ? "succeeded" : "failed", code: payload.code ?? null, output: payload.error ? `${run.output}${payload.error}\n` : run.output };
        queueMicrotask(() => {
          waiters.get(run.runId)?.(finished);
          waiters.delete(run.runId);
        });
        return finished;
      }),
      problems: payload.type === "exit" ? { ...state.problems, [state.runs.find((run) => run.runId === payload.runId)?.label ?? payload.runId]: payload.problems ?? [] } : state.problems,
    }));
  };
  const runOne = async (root: string, task: ZoraiTask): Promise<TaskRun | null> => {
    const bridge = getBridge();
    if (!bridge?.tasksRun) return null;
    unsubscribe ??= (bridge.onTaskEvent?.(handle) as (() => void) | undefined) ?? null;
    const { runId } = await bridge.tasksRun(root, task);
    const run: TaskRun = { runId, label: task.label, command: task.command, output: "", status: "running", code: null };
    const finished = new Promise<TaskRun>((resolve) => waiters.set(runId, resolve));
    set((state) => ({ runs: [run, ...state.runs].slice(0, MAX_RUNS), selectedRunId: runId, lastTaskLabel: task.label }));
    return finished;
  };
  return {
    root: null,
    tasks: [],
    runs: [],
    selectedRunId: null,
    lastTaskLabel: null,
    problems: {},
    discover: async (root) => {
      const tasks = (await getBridge()?.tasksDiscover?.(root).catch(() => [])) ?? [];
      set({ root, tasks });
      return tasks;
    },
    run: async (root, task) => {
      let last: TaskRun | null = null;
      for (const step of taskOrder(get().tasks.length > 0 ? get().tasks : [task], task)) {
        last = await runOne(root, step);
        if (!last || last.status !== "succeeded") return last;
      }
      return last;
    },
    stop: (runId) => { void getBridge()?.tasksStop?.(runId); },
    select: (runId) => set({ selectedRunId: runId }),
    clearProblems: () => set({ problems: {} }),
  };
});
