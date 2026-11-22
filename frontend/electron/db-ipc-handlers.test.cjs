const assert = require("node:assert/strict");
const test = require("node:test");

const { registerDbIpcHandlers } = require("./main/db-ipc-handlers.cjs");

function createHarness(extraRuntime = {}) {
  const handlers = new Map();
  const ackCommands = [];
  const queryCommands = [];
  let queryResult = null;
  const ipcMain = {
    handle(channel, handler) {
      handlers.set(channel, handler);
    },
  };

  registerDbIpcHandlers(ipcMain, {
    async sendDbAckCommand(command) {
      ackCommands.push(command);
    },
    async sendDbQuery(command) {
      queryCommands.push(command);
      return queryResult;
    },
    ...extraRuntime,
  });

  return {
    ackCommands,
    handlers,
    queryCommands,
    setQueryResult(value) {
      queryResult = value;
    },
  };
}

test("db-upsert-transcript-index serializes renderer transcript entries for the daemon protocol", async () => {
  const { ackCommands, handlers } = createHarness();
  const handler = handlers.get("db-upsert-transcript-index");

  const ok = await handler({}, {
    id: "tx_1",
    filename: "2026-04-29/test.log",
    filePath: "transcripts/2026-04-29/test.log",
    reason: "manual",
    workspaceId: "workspace-1",
    surfaceId: "surface-1",
    paneId: "pane-1",
    capturedAt: 1_777_474_400_123,
    sizeBytes: 42,
    preview: "hello",
    content: "hello world",
  });

  assert.equal(ok, true);
  assert.equal(ackCommands.length, 1);
  assert.equal(ackCommands[0].type, "upsert-transcript-index");
  assert.deepEqual(JSON.parse(ackCommands[0].entry_json), {
    id: "tx_1",
    pane_id: "pane-1",
    workspace_id: "workspace-1",
    surface_id: "surface-1",
    filename: "2026-04-29/test.log",
    reason: "manual",
    captured_at: 1_777_474_400_123,
    size_bytes: 42,
    preview: "hello",
  });
});

test("db-fork-thread forwards fork-agent-thread with the given thread and message id", async () => {
  const { handlers, queryCommands, setQueryResult } = createHarness();
  setQueryResult({ thread_id: "fork-daemon-1-1", title: "Fork: hello" });
  const handler = handlers.get("db-fork-thread");

  const result = await handler({}, "daemon-thread-1", "message-9");

  assert.deepEqual(queryCommands, [{
    type: "fork-agent-thread",
    thread_id: "daemon-thread-1",
    message_id: "message-9",
  }]);
  assert.deepEqual(result, {
    ok: true,
    thread_id: "fork-daemon-1-1",
    title: "Fork: hello",
  });
});

test("db-delete-message forwards delete-agent-messages with the given thread id", async () => {
  const { ackCommands, handlers } = createHarness();
  const handler = handlers.get("db-delete-message");

  const ok = await handler({}, "daemon-thread-1", "message-9");

  assert.equal(ok, true);
  assert.deepEqual(ackCommands, [{
    type: "delete-agent-messages",
    thread_id: "daemon-thread-1",
    message_ids: ["message-9"],
  }]);
});

test("db-list-transcript-index converts daemon transcript entries for renderer stores", async () => {
  const { handlers, queryCommands, setQueryResult } = createHarness();
  setQueryResult([{
    id: "tx_1",
    pane_id: "pane-1",
    workspace_id: "workspace-1",
    surface_id: "surface-1",
    filename: "2026-04-29/test.log",
    reason: "manual",
    captured_at: 1_777_474_400_123,
    size_bytes: 42,
    preview: "hello",
  }]);

  const handler = handlers.get("db-list-transcript-index");
  const entries = await handler({}, "workspace-1");

  assert.deepEqual(queryCommands, [{
    type: "list-transcript-index",
    workspace_id: "workspace-1",
  }]);
  assert.deepEqual(entries, [{
    id: "tx_1",
    filename: "2026-04-29/test.log",
    filePath: "transcripts/2026-04-29/test.log",
    reason: "manual",
    workspaceId: "workspace-1",
    surfaceId: "surface-1",
    paneId: "pane-1",
    cwd: null,
    capturedAt: 1_777_474_400_123,
    sizeBytes: 42,
    preview: "hello",
    content: "",
  }]);
});

test("db-export-thread hands the daemon's export to the user's chosen location instead of leaving it hidden in the thread folder", async () => {
  const saves = [];
  const { handlers, setQueryResult } = createHarness({
    async saveExportedFile(_event, exportedPath, suggestedName) {
      saves.push({ exportedPath, suggestedName });
      return "/home/me/Downloads/My thread.json";
    },
  });
  setQueryResult({ file_path: "/data/threads/t1/inventory/thread-export-1.json" });
  const result = await handlers.get("db-export-thread")({}, "t1", "m1", "My thread");
  assert.deepEqual(saves, [{ exportedPath: "/data/threads/t1/inventory/thread-export-1.json", suggestedName: "My thread" }]);
  assert.deepEqual(result, { ok: true, file_path: "/home/me/Downloads/My thread.json" });
});

test("db-export-thread reports a cancelled save dialog so the UI stays quiet", async () => {
  const { handlers, setQueryResult } = createHarness({ async saveExportedFile() { return null; } });
  setQueryResult({ file_path: "/data/export.json" });
  const result = await handlers.get("db-export-thread")({}, "t1", "m1", "x");
  assert.deepEqual(result, { ok: true, file_path: "/data/export.json", canceled: true });
});

test("db-list-database-tables reports a failed listing instead of claiming the database is empty", async () => {
  const timeouts = [];
  const { handlers } = createHarness({
    async sendDbQuery(_command, _responseType, timeoutMs) {
      timeouts.push(timeoutMs);
      throw new Error("DB query timeout: database-tables");
    },
  });

  const result = await handlers.get("db-list-database-tables")({});

  assert.deepEqual(result, { error: "DB query timeout: database-tables" });
  assert.ok(timeouts[0] >= 30000, "a cold history database needs longer than the default 5s to count every table");
});
