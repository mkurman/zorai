const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { createWorkspaceWatcher } = require('./main/workspace-watch-service.cjs');

function waitForBatch(root, mutate) {
    return new Promise((resolve, reject) => {
        const timeout = setTimeout(() => reject(new Error('workspace watcher timed out')), 3000);
        const watcher = createWorkspaceWatcher(root, (batch) => {
            clearTimeout(timeout);
            watcher.close();
            resolve(batch);
        }, { debounceMs: 30 });
        watcher.ready.then(mutate, reject);
    });
}

test('workspace watcher reports root-relative coalesced changes', async () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'zorai-watch-'));
    fs.mkdirSync(path.join(root, 'src'));
    const file = path.join(root, 'src', 'main.ts');
    fs.writeFileSync(file, 'first');
    const batchPromise = waitForBatch(root, () => {
        fs.writeFileSync(file, 'second');
        fs.writeFileSync(file, 'third');
    });
    const batch = await batchPromise;
    assert.equal(batch.root, fs.realpathSync.native(root));
    assert.equal(batch.changes.filter((change) => change.path === path.join('src', 'main.ts')).length, 1);
    fs.rmSync(root, { recursive: true, force: true });
});

test('workspace watcher ignores heavy directories', async () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'zorai-watch-'));
    fs.mkdirSync(path.join(root, 'node_modules'));
    const batches = [];
    const watcher = createWorkspaceWatcher(root, (batch) => batches.push(batch), { debounceMs: 25 });
    await watcher.ready;
    fs.writeFileSync(path.join(root, 'node_modules', 'ignored.js'), 'ignored');
    await new Promise((resolve) => setTimeout(resolve, 150));
    watcher.close();
    assert.equal(batches.length, 0);
    fs.rmSync(root, { recursive: true, force: true });
});

test('closing a workspace watcher releases directory watchers', async () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'zorai-watch-'));
    fs.mkdirSync(path.join(root, 'src'));
    const watcher = createWorkspaceWatcher(root, () => {});
    await watcher.ready;
    assert.ok(watcher.watchedDirectoryCount >= 2);
    watcher.close();
    assert.equal(watcher.watchedDirectoryCount, 0);
    fs.rmSync(root, { recursive: true, force: true });
});

test('creating a workspace watcher returns before the directory walk finishes, so the main process never blocks on it', () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'zorai-watch-'));
    fs.mkdirSync(path.join(root, 'src'));
    const watcher = createWorkspaceWatcher(root, () => {});
    assert.equal(watcher.watchedDirectoryCount, 0);
    watcher.close();
    fs.rmSync(root, { recursive: true, force: true });
});
