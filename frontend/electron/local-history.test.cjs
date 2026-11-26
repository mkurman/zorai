const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { createLocalHistory, MAX_ENTRIES_PER_FILE } = require('./main/local-history.cjs');

test('local history keeps every distinct save newest-first and skips identical re-saves', async () => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'zorai-history-'));
    let clock = 1000;
    const history = createLocalHistory(dir, { now: () => (clock += 1) });
    await history.record('/repo', 'a.ts', 'one');
    await history.record('/repo', 'a.ts', 'one');
    await history.record('/repo', 'a.ts', 'two');
    const entries = await history.list('/repo', 'a.ts');
    assert.equal(entries.length, 2);
    assert.equal(await history.read('/repo', 'a.ts', entries[0].id), 'two');
    assert.equal(await history.read('/repo', 'a.ts', entries[1].id), 'one');
    assert.deepEqual(await history.list('/repo', 'b.ts'), []);
    await assert.rejects(history.read('/repo', 'a.ts', '../index'), /Invalid/);
    fs.rmSync(dir, { recursive: true, force: true });
});

test('local history is a ring buffer so a file saved all day never fills the disk', async () => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'zorai-history-'));
    let clock = 0;
    const history = createLocalHistory(dir, { now: () => (clock += 1) });
    for (let index = 0; index < MAX_ENTRIES_PER_FILE + 5; index += 1) await history.record('/repo', 'a.ts', `v${index}`);
    const entries = await history.list('/repo', 'a.ts');
    assert.equal(entries.length, MAX_ENTRIES_PER_FILE);
    assert.equal(await history.read('/repo', 'a.ts', entries[entries.length - 1].id), 'v5');
    const files = fs.readdirSync(fs.readdirSync(dir).map((name) => path.join(dir, name))[0]).filter((name) => name.endsWith('.txt'));
    assert.equal(files.length, MAX_ENTRIES_PER_FILE);
    fs.rmSync(dir, { recursive: true, force: true });
});
