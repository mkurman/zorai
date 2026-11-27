const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { createLspRuntime } = require('./main/lsp-runtime.cjs');
const { resolveLanguageServer } = require('./main/lsp-registry.cjs');

test('a notebook shadow document that only exists in memory gets Python completions and diagnostics', { timeout: 90000, skip: !resolveLanguageServer('python')?.command && 'no Python language server installed' }, async () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'zorai-nb-lsp-'));
    fs.mkdirSync(path.join(root, 'notebooks'));
    const runtime = createLspRuntime();
    const diagnostics = [];
    const webContents = { isDestroyed: () => false, send: (_channel, payload) => diagnostics.push(payload) };
    const shadow = 'notebooks/test.ipynb.zorai-notebook.py';
    try {
        const opened = await runtime.open(webContents, root, shadow, 'python', 'import json\n\n\njson.du\nundefined_name\n', 1);
        assert.equal(opened.available, true, JSON.stringify(opened));
        assert.equal(fs.existsSync(path.join(root, shadow)), false);
        const deadline = Date.now() + 60000;
        while (!diagnostics.some((payload) => payload.path.endsWith('zorai-notebook.py') && payload.diagnostics.some((item) => /undefined_name/.test(item.message))) && Date.now() < deadline) {
            await new Promise((resolve) => setTimeout(resolve, 200));
        }
        assert.ok(diagnostics.some((payload) => payload.diagnostics.some((item) => /undefined_name/.test(item.message))), JSON.stringify(diagnostics));
        const response = await runtime.request(webContents, root, shadow, 'python', 'completion', { line: 3, character: 7 });
        const items = Array.isArray(response.result) ? response.result : response.result?.items ?? [];
        assert.ok(items.some((item) => item.label === 'dumps'), JSON.stringify(response).slice(0, 300));
        const hover = await runtime.request(webContents, root, shadow, 'python', 'hover', { line: 0, character: 8 });
        assert.match(JSON.stringify(hover.result), /JSON/);
        assert.throws(() => require('./main/workspace-service.cjs').resolveWorkspacePath(root, 'notebooks/missing.py'), /does not exist/);
    } finally {
        await runtime.stopAll();
        fs.rmSync(root, { recursive: true, force: true });
    }
});
