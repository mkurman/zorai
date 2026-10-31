"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");

const electronDir = __dirname;
const handlers = fs.readFileSync(path.join(electronDir, "main", "agent-ipc-handlers.cjs"), "utf8");
const preload = fs.readFileSync(path.join(electronDir, "preload.cjs"), "utf8");
const queryRuntime = fs.readFileSync(path.join(electronDir, "agent-query-runtime.cjs"), "utf8");

test("Electron registers prompt queue query handlers", () => {
    for (const channel of [
        "agent-enqueue-prompt",
        "agent-list-prompt-queue",
        "agent-update-queued-prompt",
        "agent-cancel-queued-prompt",
        "agent-send-queued-prompt-now",
        "agent-set-prompt-queue-hold",
    ]) {
        assert.match(handlers, new RegExp(`ipcMain\\.handle\\('${channel}'`));
        assert.match(preload, new RegExp(`ipcRenderer\\.invoke\\('${channel}'`));
    }
});

test("prompt-queue replies are recognized query response types", () => {
    assert.ok(queryRuntime.includes("'prompt-queue'"));
});
