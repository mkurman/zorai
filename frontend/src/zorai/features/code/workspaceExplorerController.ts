export type WorkspaceExplorerSnapshot = {
  expandedPaths: ReadonlySet<string>;
  childrenByPath: ReadonlyMap<string, ZoraiWorkspaceEntry[]>;
  loadingPaths: ReadonlySet<string>;
  errorByPath: ReadonlyMap<string, string>;
};

export type WorkspaceExplorerController = {
  getSnapshot: () => WorkspaceExplorerSnapshot;
  subscribe: (listener: () => void) => () => void;
  toggle: (path: string) => Promise<void>;
  refreshExpanded: () => Promise<void>;
};

export function createWorkspaceExplorerLoader(
  bridge: Pick<ZoraiBridge, "workspaceListDirectory"> | null | undefined,
  root: string,
): (path: string) => Promise<ZoraiWorkspaceEntry[]> {
  return async (path) => {
    if (!bridge?.workspaceListDirectory) return [];
    return bridge.workspaceListDirectory(root, path);
  };
}

export function createWorkspaceExplorerController(
  loadDirectory: (path: string) => Promise<ZoraiWorkspaceEntry[]>,
): WorkspaceExplorerController {
  let snapshot: WorkspaceExplorerSnapshot = {
    expandedPaths: new Set(),
    childrenByPath: new Map(),
    loadingPaths: new Set(),
    errorByPath: new Map(),
  };
  const listeners = new Set<() => void>();
  const publish = (next: WorkspaceExplorerSnapshot) => {
    snapshot = next;
    listeners.forEach((listener) => listener());
  };
  const pendingLoads = new Map<string, Promise<void>>();
  const loadGenerations = new Map<string, number>();
  const load = (path: string, quiet = false): Promise<void> => {
    const existing = pendingLoads.get(path);
    if (existing) return existing;
    const generation = (loadGenerations.get(path) ?? 0) + 1;
    loadGenerations.set(path, generation);
    const task = (async () => {
      if (!quiet) publish({ ...snapshot, loadingPaths: new Set([...snapshot.loadingPaths, path]), errorByPath: new Map([...snapshot.errorByPath].filter(([key]) => key !== path)) });
      try {
        const children = await loadDirectory(path);
        if (loadGenerations.get(path) !== generation) return;
        publish({ ...snapshot, childrenByPath: new Map(snapshot.childrenByPath).set(path, children) });
      } catch (reason: any) {
        if (loadGenerations.get(path) !== generation) return;
        publish({ ...snapshot, errorByPath: new Map(snapshot.errorByPath).set(path, reason?.message ?? String(reason)) });
      } finally {
        if (loadGenerations.get(path) === generation && snapshot.loadingPaths.has(path)) {
          const loadingPaths = new Set(snapshot.loadingPaths);
          loadingPaths.delete(path);
          publish({ ...snapshot, loadingPaths });
        }
      }
    })();
    pendingLoads.set(path, task);
    void task.finally(() => {
      if (pendingLoads.get(path) === task) pendingLoads.delete(path);
    });
    return task;
  };

  return {
    getSnapshot: () => snapshot,
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    async toggle(path) {
      if (snapshot.expandedPaths.has(path)) {
        const expandedPaths = new Set(snapshot.expandedPaths);
        expandedPaths.delete(path);
        publish({ ...snapshot, expandedPaths });
        return;
      }
      const previousChildren = snapshot.childrenByPath.get(path);
      const expandedBefore = new Set(snapshot.expandedPaths);
      await load(path);
      if (!snapshot.childrenByPath.has(path) && previousChildren === undefined && snapshot.errorByPath.has(path)) return;
      if (expandedBefore.has(path)) return;
      if (snapshot.expandedPaths.has(path)) return;
      publish({ ...snapshot, expandedPaths: new Set([...snapshot.expandedPaths, path]) });
    },
    async refreshExpanded() {
      const expanded = [...snapshot.expandedPaths];
      await Promise.all(expanded.map((path) => load(path, true)));
    },
  };
}

const explorerControllers = new Map<string, { bridge: unknown; controller: ReturnType<typeof createWorkspaceExplorerController> }>();

export function workspaceExplorerControllerFor(bridge: Parameters<typeof createWorkspaceExplorerLoader>[0], root: string) {
  const cached = explorerControllers.get(root);
  if (cached && cached.bridge === bridge) return cached.controller;
  const controller = createWorkspaceExplorerController(createWorkspaceExplorerLoader(bridge, root));
  explorerControllers.set(root, { bridge, controller });
  return controller;
}
