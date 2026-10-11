const test = require("node:test");
const assert = require("node:assert/strict");
const path = require("node:path");

const runtime = require(path.join(__dirname, "agent-query-runtime.cjs"));

test("gateway chat list replies resolve the pending GUI query instead of timing out", () => {
  let resolved = null;
  const bridge = {
    pending: new Map([
      ["req-1", { responseType: ["gateway-chats", "error"], ts: 1, resolve: (data) => { resolved = data; }, reject: () => {} }],
    ]),
  };
  const chats = [{ channel_key: "Discord:1", thread_id: null }];

  assert.equal(runtime.resolvePendingAgentQueryEvent(bridge, { type: "gateway-chats", data: chats }), true);
  assert.deepEqual(resolved, chats);
  assert.equal(bridge.pending.size, 0);
});
