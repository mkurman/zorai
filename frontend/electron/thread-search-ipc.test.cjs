const test = require("node:test");
const assert = require("node:assert/strict");
const path = require("node:path");

const runtime = require(path.join(__dirname, "agent-query-runtime.cjs"));

test("thread search replies resolve the pending GUI query instead of timing out", () => {
  let resolved = null;
  const bridge = {
    pending: new Map([
      ["req-1", { responseType: ["thread-search", "error"], ts: 1, resolve: (data) => { resolved = data; }, reject: () => {} }],
    ]),
  };
  const payload = { query: "lora", semantic: false, semantic_error: null, results: [] };

  assert.equal(runtime.resolvePendingAgentQueryEvent(bridge, { type: "thread-search", data: payload }), true);
  assert.deepEqual(resolved, payload);
  assert.equal(bridge.pending.size, 0);
});
