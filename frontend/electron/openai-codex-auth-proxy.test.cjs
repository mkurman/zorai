const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");

const mainPath = path.join(__dirname, "main.cjs");
const preloadPath = path.join(__dirname, "preload.cjs");
const runtimePath = path.join(__dirname, "agent-query-runtime.cjs");
const mainSrc = fs.readFileSync(mainPath, "utf8");
const preloadSrc = fs.readFileSync(preloadPath, "utf8");

let runtimeModulePromise = null;

async function loadRuntimeModule() {
  if (!runtimeModulePromise) {
    runtimeModulePromise = Promise.resolve(require(runtimePath));
  }
  return runtimeModulePromise;
}

test("main removes local sqlite codex auth ownership", () => {
  assert.doesNotMatch(mainSrc, /require\(['"]node:sqlite['"]\)/);
  assert.doesNotMatch(mainSrc, /DatabaseSync/);
  assert.doesNotMatch(mainSrc, /function getProviderAuthDbPath\(/);
  assert.doesNotMatch(mainSrc, /function withProviderAuthDb\(/);
  assert.doesNotMatch(mainSrc, /function readStoredOpenAICodexAuth\(/);
  assert.doesNotMatch(mainSrc, /function writeStoredOpenAICodexAuth\(/);
  assert.doesNotMatch(mainSrc, /function deleteStoredOpenAICodexAuth\(/);
  assert.doesNotMatch(mainSrc, /function importCodexCliAuthIfPresent\(/);
  assert.doesNotMatch(mainSrc, /async function refreshOpenAICodexAuth\(/);
  assert.doesNotMatch(mainSrc, /async function getOpenAICodexAuthStatus\(/);
  assert.doesNotMatch(mainSrc, /async function exchangeOpenAICodexAuthorizationCode\(/);
  assert.doesNotMatch(mainSrc, /async function loginOpenAICodexInteractive\(/);
});

test("preload keeps desktop codex auth API names stable", () => {
  assert.match(preloadSrc, /openAICodexAuthStatus:\s*\(options\)\s*=>\s*ipcRenderer\.invoke\('openai-codex-auth-status', options\)/);
  assert.match(preloadSrc, /openAICodexAuthLogin:\s*\(\)\s*=>\s*ipcRenderer\.invoke\('openai-codex-auth-login'\)/);
  assert.match(preloadSrc, /openAICodexAuthLogout:\s*\(\)\s*=>\s*ipcRenderer\.invoke\('openai-codex-auth-logout'\)/);
});

test("main wires desktop codex auth IPC handlers through shared runtime helpers", () => {
  assert.match(
    mainSrc,
    /const\s*\{[\s\S]*createOpenAICodexAuthHandlers,[\s\S]*resolvePendingAgentQueryEvent,[\s\S]*\}\s*=\s*require\('\.\/agent-query-runtime\.cjs'\)/
  );
  assert.match(
    mainSrc,
    /const openAICodexAuthHandlers = createOpenAICodexAuthHandlers\(sendAgentQuery\)/
  );
  assert.match(
    mainSrc,
    /ipcMain\.handle\('openai-codex-auth-status', async \(_event, options\) => \{[\s\S]*?openAICodexAuthHandlers\.status\(_event, options\)/
  );
  assert.match(
    mainSrc,
    /ipcMain\.handle\('openai-codex-auth-login', async \(\) => \{[\s\S]*?openAICodexAuthHandlers\.login\(\)/
  );
  assert.match(
    mainSrc,
    /ipcMain\.handle\('openai-codex-auth-logout', async \(\) => \{[\s\S]*?openAICodexAuthHandlers\.logout\(\)/
  );
});

test("main uses shared resolver for daemon codex auth bridge response types", async () => {
  const { AGENT_QUERY_RESPONSE_TYPES } = await loadRuntimeModule();

  assert.match(mainSrc, /resolvePendingAgentQueryEvent\(agentBridge, event\)/);
  assert.ok(AGENT_QUERY_RESPONSE_TYPES.includes("openai-codex-auth-status"));
  assert.ok(AGENT_QUERY_RESPONSE_TYPES.includes("openai-codex-auth-login-result"));
  assert.ok(AGENT_QUERY_RESPONSE_TYPES.includes("openai-codex-auth-logout-result"));
});

test("runtime status handler sends daemon query and passes refresh option through", async () => {
  const { createOpenAICodexAuthHandlers } = await loadRuntimeModule();
  const calls = [];
  const handlers = createOpenAICodexAuthHandlers(async (...args) => {
    calls.push(args);
    return { available: true, authMode: "chatgpt_subscription" };
  });

  const result = await handlers.status(null, { refresh: false });

  assert.deepEqual(result, { available: true, authMode: "chatgpt_subscription" });
  assert.deepEqual(calls, [[
    { type: "openai-codex-auth-status", refresh: false },
    "openai-codex-auth-status",
    30000,
  ]]);
});

test("runtime login and logout handlers use daemon auth query response types", async () => {
  const { createOpenAICodexAuthHandlers } = await loadRuntimeModule();
  const calls = [];
  const handlers = createOpenAICodexAuthHandlers(async (...args) => {
    calls.push(args);
    return { ok: true };
  });

  await handlers.login();
  await handlers.logout();

  assert.deepEqual(calls, [
    [
      { type: "openai-codex-auth-login" },
      "openai-codex-auth-login-result",
      30000,
    ],
    [
      { type: "openai-codex-auth-logout" },
      "openai-codex-auth-logout-result",
      30000,
    ],
  ]);
});

test("runtime auth handlers return existing fallback payloads on query failure", async () => {
  const { createOpenAICodexAuthHandlers } = await loadRuntimeModule();
  const handlers = createOpenAICodexAuthHandlers(async (command) => {
    throw new Error(`failed:${command.type}`);
  });

  assert.deepEqual(await handlers.status(null, { refresh: true }), {
    available: false,
    authMode: "chatgpt_subscription",
    error: "failed:openai-codex-auth-status",
  });
  assert.deepEqual(await handlers.login(), {
    available: false,
    authMode: "chatgpt_subscription",
    error: "failed:openai-codex-auth-login",
  });
  assert.deepEqual(await handlers.logout(), {
    ok: false,
    error: "failed:openai-codex-auth-logout",
  });
});

test("runtime response resolution admits auth response types and resolves oldest matching pending query", async () => {
  const { resolvePendingAgentQueryEvent } = await loadRuntimeModule();
  const resolved = [];
  const rejected = [];
  const pending = new Map([
    ["older", {
      responseType: "openai-codex-auth-login-result",
      ts: 10,
      resolve: (value) => resolved.push(["older", value]),
      reject: (error) => rejected.push(error),
    }],
    ["newer", {
      responseType: "openai-codex-auth-login-result",
      ts: 20,
      resolve: (value) => resolved.push(["newer", value]),
      reject: (error) => rejected.push(error),
    }],
  ]);

  const handled = resolvePendingAgentQueryEvent({ pending }, {
    type: "openai-codex-auth-login-result",
    data: { available: false, status: "pending" },
  });

  assert.equal(handled, true);
  assert.deepEqual(resolved, [["older", { available: false, status: "pending" }]]);
  assert.equal(rejected.length, 0);
  assert.deepEqual([...pending.keys()], ["newer"]);
});

test("runtime response resolution ignores non-allowlisted auth-adjacent events", async () => {
  const { isAgentQueryResponseType, resolvePendingAgentQueryEvent } = await loadRuntimeModule();
  const pending = new Map([
    ["status", {
      responseType: "openai-codex-auth-status",
      ts: 10,
      resolve: () => {
        throw new Error("should not resolve");
      },
      reject: () => {
        throw new Error("should not reject");
      },
    }],
  ]);

  assert.equal(isAgentQueryResponseType("openai-codex-auth-status"), true);
  assert.equal(isAgentQueryResponseType("openai-codex-auth-login-result"), true);
  assert.equal(isAgentQueryResponseType("openai-codex-auth-logout-result"), true);
  assert.equal(isAgentQueryResponseType("openai-codex-auth-login"), false);
  assert.equal(resolvePendingAgentQueryEvent({ pending }, { type: "openai-codex-auth-login" }), false);
  assert.deepEqual([...pending.keys()], ["status"]);
});

test("every reply type an IPC handler awaits is routed back to its pending query", async () => {
  const { isAgentQueryResponseType } = await loadRuntimeModule();
  const dedicatedHandlers = new Set(["error", "whatsapp-link-status"]);
  const mainDir = path.join(__dirname, "main");
  const awaited = new Set();
  for (const file of fs.readdirSync(mainDir).filter((name) => name.endsWith(".cjs") && !name.includes(".test."))) {
    const source = fs.readFileSync(path.join(mainDir, file), "utf8");
    for (const match of source.matchAll(/sendAgentQuery\(\s*\{[^}]*?\}\s*,\s*(\[[^\]]*\]|'[^']+'|"[^"]+")/gs)) {
      for (const type of match[1].matchAll(/['"]([^'"]+)['"]/g)) awaited.add(type[1]);
    }
  }
  assert.ok(awaited.has("data-query-result"));
  const dropped = [...awaited].filter((type) => !dedicatedHandlers.has(type) && !isAgentQueryResponseType(type));
  assert.deepEqual(dropped, []);
});
