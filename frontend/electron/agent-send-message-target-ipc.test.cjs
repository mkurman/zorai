const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");

const preloadPath = path.join(__dirname, "preload.cjs");
const handlerPath = path.join(__dirname, "main", "agent-ipc-handlers.cjs");
const preloadSrc = fs.readFileSync(preloadPath, "utf8");
const { registerAgentIpcHandlers } = require(handlerPath);

function createHandlerHarness(queryImpl = async () => ({ ok: true })) {
  const handlers = new Map();
  const commands = [];
  const ipcMain = {
    handle(name, handler) {
      handlers.set(name, handler);
    },
  };

  registerAgentIpcHandlers(
    ipcMain,
    {
      sendAgentCommand: (command) => commands.push(command),
      sendAgentQuery: queryImpl,
    },
    {
      logToFile: () => {},
      openAICodexAuthHandlers: {
        status: async () => ({ available: false }),
        login: async () => ({ available: false }),
        logout: async () => ({ ok: true }),
      },
    },
  );

  return { handlers, commands };
}

test("preload forwards an optional target agent when sending a message", () => {
  assert.match(
    preloadSrc,
    /agentSendMessage:\s*\(threadId, content, sessionId, contextMessages, contentBlocksJson, targetAgentId, workspaceContext\)\s*=>\s*ipcRenderer\.invoke\('agent-send-message', threadId, content, sessionId, contextMessages, contentBlocksJson, targetAgentId, workspaceContext\)/,
  );
});

test("preload exposes typed subagent delegation", () => {
  assert.match(preloadSrc, /agentSpawnSubagent:\s*\(threadId, request\)\s*=>\s*ipcRenderer\.invoke\('agent-spawn-subagent', threadId, request\)/);
});

test("agent spawn-subagent IPC preserves parent thread and request", async () => {
  const queries = [];
  const { handlers } = createHandlerHarness(async (command, responseType) => {
    queries.push({ command, responseType });
    return { result: { ok: true, content: "spawned" } };
  });
  const request = { title: "Reviewer", description: "Review this", cwd: "/repo" };

  const result = await handlers.get("agent-spawn-subagent")(null, "daemon-thread-1", request);

  assert.deepEqual(queries, [{
    command: { type: "spawn-subagent", thread_id: "daemon-thread-1", args: request },
    responseType: "subagent-spawned",
  }]);
  assert.deepEqual(result, { ok: true, content: "spawned" });
});

test("agent send-message IPC forwards image content blocks to the bridge", async () => {
  const { handlers, commands } = createHandlerHarness();
  const contentBlocksJson = JSON.stringify([{
    type: "image",
    data_url: "data:image/png;base64,iVBORw0KGgo=",
    mime_type: "image/png",
  }]);

  await handlers.get("agent-send-message")(
    null,
    "local-thread-image",
    "What is in this image?",
    null,
    null,
    contentBlocksJson,
    null,
  );

  assert.deepEqual(commands, [{
    type: "send-message",
    thread_id: "local-thread-image",
    content: "What is in this image?",
    session_id: null,
    target_agent_id: null,
    content_blocks_json: contentBlocksJson,
  }]);
});

test("agent send-message IPC forwards the selected agent to the daemon", async () => {
  const { handlers, commands } = createHandlerHarness();

  await handlers.get("agent-send-message")(
    null,
    "local-thread-1",
    "Review this",
    null,
    null,
    null,
    "reviewer",
  );

  assert.deepEqual(commands, [{
    type: "send-message",
    thread_id: "local-thread-1",
    content: "Review this",
    session_id: null,
    target_agent_id: "reviewer",
  }]);
});

test("agent send-message IPC forwards the pinned workspace so the daemon has it before the first prompt", async () => {
  const { handlers, commands } = createHandlerHarness();
  const workspaceContext = { root: "/repo", active_file: "src/lib.rs", isolate_agent_tasks: true };

  await handlers.get("agent-send-message")(
    null,
    null,
    "First message",
    null,
    null,
    null,
    null,
    workspaceContext,
  );

  assert.deepEqual(commands, [{
    type: "send-message",
    thread_id: null,
    content: "First message",
    session_id: null,
    target_agent_id: null,
    workspace_context: workspaceContext,
  }]);
});
