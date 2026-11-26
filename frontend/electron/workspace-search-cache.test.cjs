const test = require('node:test');
const assert = require('node:assert/strict');
const ws = require('./main/workspace-service.cjs');
const { createWorkspaceSearchCache } = require('./main/workspace-search-cache.cjs');

function fakeWorkspace(initial) {
    const files = new Map(Object.entries(initial).map(([name, text]) => [name, { text, mtimeMs: 1 }]));
    const counts = { reads: 0, stats: 0, lists: 0 };
    const fsPromises = {
        async stat(absolutePath) {
            counts.stats += 1;
            const file = files.get(absolutePath.replace(/^\/w\//, ''));
            if (!file) throw new Error('ENOENT');
            return { size: Buffer.byteLength(file.text), mtimeMs: file.mtimeMs, isFile: () => true };
        },
        async readFile(absolutePath) {
            counts.reads += 1;
            return Buffer.from(files.get(absolutePath.replace(/^\/w\//, '')).text);
        },
    };
    return {
        files,
        counts,
        write(name, text) { files.set(name, { text, mtimeMs: (files.get(name)?.mtimeMs ?? 0) + 1 }); },
        cache(options = {}) {
            return createWorkspaceSearchCache({
                canonicalRoot: (root) => root,
                listFiles: async () => { counts.lists += 1; return { files: [...files.keys()] }; },
                collectMatches: ws.collectWorkspaceTextMatches,
                searchPattern: ws.workspaceSearchPattern,
                globMatcher: ws.workspaceGlobMatcher,
                isBinary: ws.isProbablyBinary,
                fsPromises,
                ...options,
            });
        },
    };
}

test('a repeated search in a watched folder reads nothing from disk, which is the whole point of the cache', async () => {
    const workspace = fakeWorkspace({ 'a.ts': 'const needle = 1;', 'b.ts': 'nothing here' });
    const cache = workspace.cache({ isTrusted: () => true });
    assert.equal((await cache.search('/w', 'needle')).length, 1);
    const before = { ...workspace.counts };
    assert.equal((await cache.search('/w', 'needle')).length, 1);
    assert.deepEqual(workspace.counts, before);
});

test('outside watched folders an edited file is noticed by size/mtime, so results are never stale', async () => {
    const workspace = fakeWorkspace({ 'a.ts': 'old text' });
    const cache = workspace.cache({ isTrusted: () => false });
    assert.equal((await cache.search('/w', 'needle')).length, 0);
    workspace.write('a.ts', 'now has needle');
    assert.equal((await cache.search('/w', 'needle')).length, 1);
});

test('a watcher change event drops the cached text even in trusted folders', async () => {
    const workspace = fakeWorkspace({ 'src/a.ts': 'old text' });
    const cache = workspace.cache({ isTrusted: () => true });
    await cache.search('/w', 'needle');
    workspace.write('src/a.ts', 'needle arrived');
    assert.equal((await cache.search('/w', 'needle')).length, 0);
    cache.applyChanges('/w', [{ path: 'src/a.ts', eventType: 'change' }]);
    assert.equal((await cache.search('/w', 'needle')).length, 1);
});

test('refresh (fresh) rescans the file list and text, picking up files the list did not know about', async () => {
    const workspace = fakeWorkspace({ 'a.ts': 'x' });
    const cache = workspace.cache({ isTrusted: () => true });
    await cache.search('/w', 'needle');
    workspace.write('new.ts', 'needle');
    assert.equal((await cache.search('/w', 'needle')).length, 0);
    assert.equal((await cache.search('/w', 'needle', { fresh: true })).length, 1);
});

test('excluding a folder name hides everything inside it, as the folder walk did', async () => {
    const workspace = fakeWorkspace({ 'docs/guide.md': 'needle', 'src/a.ts': 'needle' });
    const results = await workspace.cache().search('/w', 'needle', { exclude: 'docs' });
    assert.deepEqual(results.map((hit) => hit.path), ['src/a.ts']);
});

test('results follow file-list order and stop at maxResults even though files are read in parallel', async () => {
    const names = Array.from({ length: 50 }, (_, index) => `f${String(index).padStart(2, '0')}.ts`);
    const workspace = fakeWorkspace(Object.fromEntries(names.map((name) => [name, 'needle needle'])));
    const results = await workspace.cache().search('/w', 'needle', { maxResults: 5 });
    assert.deepEqual(results.map((hit) => `${hit.path}:${hit.column}`), ['f00.ts:1', 'f00.ts:8', 'f01.ts:1', 'f01.ts:8', 'f02.ts:1']);
});

test('the cache never holds more text than its cap, but still searches the overflow', async () => {
    const workspace = fakeWorkspace({ 'a.ts': 'needle aaaaaaaaaa', 'b.ts': 'needle bbbbbbbbbb' });
    const cache = workspace.cache({ isTrusted: () => true, maxCachedChars: 20 });
    assert.equal((await cache.search('/w', 'needle')).length, 2);
    assert.ok(cache.stats().cachedChars <= 20);
});

test('warming in the background makes even the first search a cache hit', async () => {
    const workspace = fakeWorkspace({ 'a.ts': 'needle', 'b.ts': 'other' });
    const cache = workspace.cache({ isTrusted: () => true });
    assert.equal(await cache.warm('/w'), 2);
    const readsAfterWarm = workspace.counts.reads;
    assert.equal((await cache.search('/w', 'needle')).length, 1);
    assert.equal(workspace.counts.reads, readsAfterWarm);
});
