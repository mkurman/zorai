import { describe, expect, it } from "vitest";
import type { StateStorage } from "zustand/middleware";
import {
  isCodeProjectThread,
  CODE_WORKSPACE_BINDING_STORE_NAME,
  CODE_WORKSPACE_BINDING_STORE_VERSION,
  CODE_PROJECT_THREADS_PER_ROOT_MAX,
  createCodeWorkspaceBindingStore,
} from "./codeWorkspaceBindingStore";

function createMemoryStorage(): StateStorage {
  const values = new Map<string, string>();
  return {
    getItem: (name) => values.get(name) ?? null,
    setItem: (name, value) => {
      values.set(name, value);
    },
    removeItem: (name) => {
      values.delete(name);
    },
  };
}

function readPersistedJson(storage: StateStorage) {
  const raw = storage.getItem(CODE_WORKSPACE_BINDING_STORE_NAME);
  return raw ? JSON.parse(raw) : null;
}

describe("codeWorkspaceBindingStore", () => {
  it("records project threads newest-first, dedupes, and promotes re-selected threads", () => {
    const store = createCodeWorkspaceBindingStore(createMemoryStorage());

    store.getState().recordProjectThread("/work/a", "thread-1");
    store.getState().recordProjectThread("/work/a", "thread-2");
    store.getState().recordProjectThread("/work/a", "thread-1");
    store.getState().recordProjectThread("/work/b", "thread-3");

    expect(store.getState().threadsByRoot).toEqual({
      "/work/a": ["thread-1", "thread-2"],
      "/work/b": ["thread-3"],
    });
    expect(store.getState().threadForRoot("/work/a")).toBe("thread-1");
    expect(store.getState().projectThreadIdsForRoot("/work/a")).toEqual(["thread-1", "thread-2"]);
    expect(store.getState().threadForRoot("/work/missing")).toBeNull();
  });

  it("does not rewrite the binding map when the active project thread is already first", () => {
    const store = createCodeWorkspaceBindingStore(createMemoryStorage());
    store.getState().recordProjectThread("/work/a", "thread-1");
    const firstMap = store.getState().threadsByRoot;
    store.getState().recordProjectThread("/work/a", "thread-1");
    expect(store.getState().threadsByRoot).toBe(firstMap);
  });

  it("caps remembered project threads per root", () => {
    const store = createCodeWorkspaceBindingStore(createMemoryStorage());

    for (let index = 0; index < CODE_PROJECT_THREADS_PER_ROOT_MAX + 10; index += 1) {
      store.getState().recordProjectThread("/work/a", `thread-${index}`);
    }

    expect(store.getState().threadsByRoot["/work/a"]).toHaveLength(CODE_PROJECT_THREADS_PER_ROOT_MAX);
    expect(store.getState().threadsByRoot["/work/a"]?.[0]).toBe(`thread-${CODE_PROJECT_THREADS_PER_ROOT_MAX + 9}`);
  });

  it("persists the v2 list shape with version 2 and restores it into a fresh store", async () => {
    const storage = createMemoryStorage();
    const first = createCodeWorkspaceBindingStore(storage);

    first.getState().recordProjectThread("/work/a", "thread-1");
    first.getState().recordProjectThread("/work/a", "thread-2");
    first.getState().setLastRoot("/work/a");

    const persisted = readPersistedJson(storage);
    expect(persisted).toEqual({
      state: {
        lastRoot: "/work/a",
        threadsByRoot: { "/work/a": ["thread-2", "thread-1"] },
      },
      version: 2,
    });
    expect(CODE_WORKSPACE_BINDING_STORE_VERSION).toBe(2);

    const second = createCodeWorkspaceBindingStore(storage);
    await second.persist.rehydrate();
    expect(second.getState().lastRoot).toBe("/work/a");
    expect(second.getState().projectThreadIdsForRoot("/work/a")).toEqual(["thread-2", "thread-1"]);
  });

  it("migrates v1 single-thread mappings into the project-thread list", async () => {
    const storage = createMemoryStorage();
    storage.setItem(
      CODE_WORKSPACE_BINDING_STORE_NAME,
      JSON.stringify({
        state: {
          lastRoot: "/work/a",
          threadByRoot: { "/work/a": "thread-1" },
        },
        version: 1,
      }),
    );

    const store = createCodeWorkspaceBindingStore(storage);
    await store.persist.rehydrate();

    expect(store.getState().lastRoot).toBe("/work/a");
    expect(store.getState().threadsByRoot).toEqual({ "/work/a": ["thread-1"] });

    const persisted = readPersistedJson(storage);
    expect(persisted.version).toBe(CODE_WORKSPACE_BINDING_STORE_VERSION);
    expect(persisted.state).toEqual({
      lastRoot: "/work/a",
      threadsByRoot: { "/work/a": ["thread-1"] },
    });
  });

  it("forgets one project thread without dropping the rest of the list", () => {
    const store = createCodeWorkspaceBindingStore(createMemoryStorage());

    store.getState().recordProjectThread("/work/a", "thread-1");
    store.getState().recordProjectThread("/work/a", "thread-2");
    store.getState().forgetProjectThread("/work/a", "thread-1");

    expect(store.getState().threadsByRoot).toEqual({ "/work/a": ["thread-2"] });
    expect(store.getState().lastRoot).toBeNull();

    store.getState().forgetProjectThread("/work/a", "thread-2");
    expect(store.getState().threadsByRoot).toEqual({});
  });

  it("removes stale mappings without deleting the referenced thread", () => {
    const store = createCodeWorkspaceBindingStore(createMemoryStorage());

    store.getState().recordProjectThread("/work/a", "thread-1");
    store.getState().recordProjectThread("/work/b", "thread-2");
    store.getState().removeRootBinding("/work/a");

    expect(store.getState().threadsByRoot).toEqual({ "/work/b": ["thread-2"] });
    expect(store.getState().threadForRoot("/work/a")).toBeNull();
    expect(store.getState().lastRoot).toBeNull();
  });

  it("keeps lastRoot untouched when removing a non-stale binding", () => {
    const store = createCodeWorkspaceBindingStore(createMemoryStorage());

    store.getState().setLastRoot("/work/a");
    store.getState().recordProjectThread("/work/b", "thread-2");
    store.getState().removeRootBinding("/work/b");

    expect(store.getState().threadsByRoot).toEqual({});
    expect(store.getState().lastRoot).toBe("/work/a");
  });

  it("closes a root by dropping its list and clearing a matching lastRoot", () => {
    const store = createCodeWorkspaceBindingStore(createMemoryStorage());

    store.getState().setLastRoot("/work/a");
    store.getState().recordProjectThread("/work/a", "thread-1");
    store.getState().closeRoot("/work/a");

    expect(store.getState().threadsByRoot).toEqual({});
    expect(store.getState().lastRoot).toBeNull();
  });

  it("closing a root that is not lastRoot only removes its list", () => {
    const store = createCodeWorkspaceBindingStore(createMemoryStorage());

    store.getState().setLastRoot("/work/a");
    store.getState().recordProjectThread("/work/b", "thread-2");
    store.getState().closeRoot("/work/b");

    expect(store.getState().threadsByRoot).toEqual({});
    expect(store.getState().lastRoot).toBe("/work/a");
  });

  it("hydrates with explicit bindings and ignores malformed entries", () => {
    const store = createCodeWorkspaceBindingStore(createMemoryStorage());

    store.getState().hydrate({
      lastRoot: "  /work/a  ",
      threadsByRoot: {
        "/work/a": ["thread-1", "   ", 42 as unknown as string, "thread-1"],
        "/work/empty": [],
        "/work/bad": "nope" as unknown as string[],
      },
    });

    expect(store.getState().lastRoot).toBe("/work/a");
    expect(store.getState().threadsByRoot).toEqual({ "/work/a": ["thread-1"] });
  });

  it("ignores empty roots and thread ids in record actions", () => {
    const store = createCodeWorkspaceBindingStore(createMemoryStorage());

    store.getState().recordProjectThread("   ", "thread-1");
    store.getState().recordProjectThread("/work/a", "   ");

    expect(store.getState().threadsByRoot).toEqual({});
  });

  it("isolates the factory storage when no explicit storage is given", async () => {
    const first = createCodeWorkspaceBindingStore();
    first.getState().setLastRoot("/work/a");

    // A second factory instance must not see the first instance's state:
    // without explicit storage each factory gets its own in-memory storage
    // instead of falling through to renderer localStorage.
    const second = createCodeWorkspaceBindingStore();
    await second.persist.rehydrate();

    expect(second.getState().lastRoot).toBeNull();
    expect(second.getState().threadsByRoot).toEqual({});
  });

  it("normalizes duplicate and oversized persisted lists on rehydrate", async () => {
    const storage = createMemoryStorage();
    storage.setItem(
      CODE_WORKSPACE_BINDING_STORE_NAME,
      JSON.stringify({
        state: {
          lastRoot: "  /work/a  ",
          threadsByRoot: {
            " /work/a ": ["thread-1", "thread-1", "   ", "thread-2"],
            "/work/legacy": "ignored" as unknown as string[],
            "/work/empty": [],
          },
          extraFutureField: "ignored",
        },
        version: CODE_WORKSPACE_BINDING_STORE_VERSION,
      }),
    );

    const store = createCodeWorkspaceBindingStore(storage);
    await store.persist.rehydrate();

    expect(store.getState().lastRoot).toBe("/work/a");
    expect(store.getState().threadsByRoot).toEqual({ "/work/a": ["thread-1", "thread-2"] });
  });
});

describe("isCodeProjectThread", () => {
  it("lets the Code tab claim only its own project threads, never a thread opened from the Threads tab", () => {
    const projectThreadIds = ["daemon-code-1", "local-draft"];
    expect(isCodeProjectThread(projectThreadIds, { id: "local-1", daemonThreadId: "daemon-code-1" })).toBe(true);
    expect(isCodeProjectThread(projectThreadIds, { id: "local-draft", daemonThreadId: null })).toBe(true);
    expect(isCodeProjectThread(projectThreadIds, { id: "local-2", daemonThreadId: "daemon-research" })).toBe(false);
    expect(isCodeProjectThread(projectThreadIds, undefined)).toBe(false);
  });
});
