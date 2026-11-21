const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { EventEmitter } = require('node:events');
const { createWorkspaceWorkerClient, workspaceServiceProxy } = require('./main/workspace-worker-client.cjs');

function fakeParentPort() {
    const port = new EventEmitter();
    port.sent = [];
    port.postMessage = (message) => port.sent.push(message);
    return port;
}

function loadWorker(port) {
    process.parentPort = port;
    const workerPath = require.resolve('./main/workspace-worker.cjs');
    delete require.cache[workerPath];
    require(workerPath);
}

function workerFork(port) {
    return () => {
        const child = new EventEmitter();
        port.postMessage = (message) => child.emit('message', message);
        child.postMessage = (message) => port.emit('message', { data: message });
        child.kill = () => child.emit('exit', 0);
        return child;
    };
}

test('workspace file reads run in the worker and come back to the main process unchanged', async () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'zorai-worker-'));
    fs.writeFileSync(path.join(root, 'notes.md'), '# hello\n');
    const port = fakeParentPort();
    loadWorker(port);
    const client = createWorkspaceWorkerClient({ fork: workerFork(port) });
    const service = workspaceServiceProxy(client, ['readWorkspaceFile', 'listWorkspaceDirectory']);
    const file = await service.readWorkspaceFile(root, 'notes.md');
    assert.equal(file.content, '# hello\n');
    const entries = await service.listWorkspaceDirectory(root, '');
    assert.deepEqual(entries.map((entry) => entry.name), ['notes.md']);
    client.stop();
    fs.rmSync(root, { recursive: true, force: true });
});

test('worker errors keep their message and fields so the editor can still detect too-large and binary files', async () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'zorai-worker-'));
    fs.writeFileSync(path.join(root, 'big.txt'), 'x'.repeat(64));
    const port = fakeParentPort();
    loadWorker(port);
    const client = createWorkspaceWorkerClient({ fork: workerFork(port) });
    await assert.rejects(
        client.call('readWorkspaceFile', root, 'big.txt', { maxBytes: 8 }),
        (error) => /File is 64 bytes; limit is 8/.test(error.message) && error.code === 'WORKSPACE_FILE_TOO_LARGE' && error.sizeBytes === 64,
    );
    client.stop();
    fs.rmSync(root, { recursive: true, force: true });
});

test('a crashed worker fails in-flight calls instead of hanging them, and the next call starts a fresh worker', async () => {
    let forks = 0;
    let current = null;
    const client = createWorkspaceWorkerClient({
        fork: () => {
            forks += 1;
            current = new EventEmitter();
            current.postMessage = () => {};
            current.kill = () => current.emit('exit', 0);
            return current;
        },
    });
    const inFlight = client.call('readWorkspaceFile', '/x', 'y');
    current.emit('exit', 1);
    await assert.rejects(inFlight, /Workspace worker stopped/);
    void client.call('readWorkspaceFile', '/x', 'y').catch(() => {});
    assert.equal(forks, 2);
    client.stop();
});

const { createWorkspaceListCache } = require('./main/workspace-list-cache.cjs');

test('folder listings are served from memory while a watcher covers the workspace, so slow disks are read once', async () => {
    let reads = 0;
    const cache = createWorkspaceListCache({ canonicalRoot: (root) => root, list: async () => { reads += 1; return [{ name: `v${reads}` }]; } });
    cache.watchStarted('/w');
    await cache.listDirectory('/w', 'src');
    const again = await cache.listDirectory('/w', 'src');
    assert.equal(reads, 1);
    assert.equal(again[0].name, 'v1');
});

test('a watcher change or a workspace mutation drops only the affected listings', async () => {
    let reads = 0;
    const cache = createWorkspaceListCache({ canonicalRoot: (root) => root, list: async (_root, rel) => { reads += 1; return [{ name: `${rel}:${reads}` }]; } });
    cache.watchStarted('/w');
    await cache.listDirectory('/w', 'src');
    await cache.listDirectory('/w', 'docs');
    cache.applyChanges('/w', [{ path: 'src/new.ts', eventType: 'rename' }]);
    await cache.listDirectory('/w', 'src');
    await cache.listDirectory('/w', 'docs');
    assert.equal(reads, 3);
    cache.invalidateRoot('/w');
    await cache.listDirectory('/w', 'docs');
    assert.equal(reads, 4);
});

test('without a watcher, cached listings expire quickly so nothing stays stale', async () => {
    let clock = 0;
    let reads = 0;
    const cache = createWorkspaceListCache({ canonicalRoot: (root) => root, list: async () => { reads += 1; return []; }, now: () => clock });
    await cache.listDirectory('/w', '');
    clock = 1000;
    await cache.listDirectory('/w', '');
    clock = 5000;
    await cache.listDirectory('/w', '');
    assert.equal(reads, 2);
});

test('an explicit refresh drops every cached listing for the workspace so Refresh always shows the disk', async () => {
    let reads = 0;
    const cache = createWorkspaceListCache({ canonicalRoot: (root) => root, list: async () => { reads += 1; return []; } });
    cache.watchStarted('/w');
    await cache.listDirectory('/w', 'src');
    await cache.listDirectory('/w', '', { fresh: true });
    await cache.listDirectory('/w', 'src');
    assert.equal(reads, 3);
});
