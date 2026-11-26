const crypto = require('crypto');
const fs = require('fs');
const path = require('path');

const MAX_ENTRIES_PER_FILE = 50;
const MAX_ENTRY_BYTES = 2 * 1024 * 1024;

function createLocalHistory(baseDir, { now = () => Date.now() } = {}) {
    const fileKey = (root, relativePath) => crypto.createHash('sha256').update(`${root}\0${relativePath}`).digest('hex').slice(0, 32);
    const dirFor = (root, relativePath) => path.join(baseDir, fileKey(root, relativePath));
    const indexPath = (dir) => path.join(dir, 'index.json');

    async function readIndex(dir) {
        try {
            const value = JSON.parse(await fs.promises.readFile(indexPath(dir), 'utf8'));
            return Array.isArray(value?.entries) ? value : { entries: [] };
        } catch {
            return { entries: [] };
        }
    }

    return {
        async record(root, relativePath, content) {
            const text = String(content ?? '');
            const bytes = Buffer.byteLength(text, 'utf8');
            if (!root || !relativePath || bytes > MAX_ENTRY_BYTES) return null;
            const dir = dirFor(root, relativePath);
            await fs.promises.mkdir(dir, { recursive: true });
            const index = await readIndex(dir);
            const hash = crypto.createHash('sha256').update(text).digest('hex');
            const latest = index.entries[index.entries.length - 1];
            if (latest?.hash === hash) return latest;
            const entry = { id: `${now()}-${hash.slice(0, 8)}`, at: now(), size: bytes, hash };
            await fs.promises.writeFile(path.join(dir, `${entry.id}.txt`), text, 'utf8');
            const entries = [...index.entries, entry];
            const dropped = entries.splice(0, Math.max(0, entries.length - MAX_ENTRIES_PER_FILE));
            await Promise.all(dropped.map((old) => fs.promises.rm(path.join(dir, `${old.id}.txt`), { force: true })));
            await fs.promises.writeFile(indexPath(dir), JSON.stringify({ root, path: relativePath, entries }), 'utf8');
            return entry;
        },
        async list(root, relativePath) {
            const index = await readIndex(dirFor(root, relativePath));
            return [...index.entries].reverse().map(({ id, at, size }) => ({ id, at, size }));
        },
        async read(root, relativePath, id) {
            if (!/^[\w-]+$/.test(String(id))) throw new Error('Invalid local history entry.');
            return fs.promises.readFile(path.join(dirFor(root, relativePath), `${id}.txt`), 'utf8');
        },
    };
}

module.exports = { createLocalHistory, MAX_ENTRIES_PER_FILE };
