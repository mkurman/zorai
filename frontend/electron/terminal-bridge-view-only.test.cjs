const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { isViewOnlyTerminalPane } = require('./main/terminal-bridge-runtime.cjs');

test('viewing an agent terminal attaches only, so a dead session never spawns a stray replacement shell', () => {
    assert.equal(isViewOnlyTerminalPane('thread-terminal-view:7f3c'), true);
    assert.equal(isViewOnlyTerminalPane('thread-terminal:code:/repo:1'), false);
    assert.equal(isViewOnlyTerminalPane(undefined), false);
});

test('the renderer still names agent view panes with the prefix Electron treats as view-only', () => {
    const store = fs.readFileSync(path.join(__dirname, '..', 'src', 'zorai', 'features', 'terminal', 'threadTerminalStore.ts'), 'utf8');
    assert.match(store, /`thread-terminal-view:\$\{agentSessionId\}`/);
});
