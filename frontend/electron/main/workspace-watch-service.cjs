const fs = require('fs');
const path = require('path');
const { randomUUID } = require('crypto');
const { canonicalWorkspaceRoot, isWithinRoot } = require('./workspace-service.cjs');

const DEFAULT_DEBOUNCE_MS = 100;
const DEFAULT_MAX_DIRECTORIES = 2048;
const IGNORED_DIRECTORY_NAMES = new Set([
    '.git', 'node_modules', 'target', 'dist', 'build', '.venv', '__pycache__', '.next', '.cache',
]);

function createWorkspaceWatcher(rootPath, onBatch, options = {}) {
    if (typeof onBatch !== 'function') throw new Error('Workspace watcher callback is required.');
    const root = canonicalWorkspaceRoot(rootPath);
    const subscriptionId = randomUUID();
    const debounceMs = Math.max(25, Math.min(Number(options.debounceMs) || DEFAULT_DEBOUNCE_MS, 2000));
    const maxDirectories = Math.max(1, Math.min(Number(options.maxDirectories) || DEFAULT_MAX_DIRECTORIES, 10000));
    const watchers = new Map();
    const pending = new Map();
    let closed = false;
    let flushTimer = null;

    function flush() {
        flushTimer = null;
        if (closed || pending.size === 0) return;
        const changes = [...pending.values()]
            .sort((left, right) => left.path.localeCompare(right.path));
        pending.clear();
        onBatch({ subscriptionId, root, changes, emittedAt: Date.now() });
    }

    function enqueue(absolutePath, eventType) {
        const resolved = path.resolve(absolutePath);
        if (!isWithinRoot(root, resolved)) return;
        const relativePath = path.relative(root, resolved);
        if (!relativePath || relativePath.split(path.sep).some((part) => IGNORED_DIRECTORY_NAMES.has(part))) return;
        pending.set(relativePath, {
            path: relativePath,
            eventType: eventType === 'rename' ? 'rename' : 'change',
            observedAt: Date.now(),
        });
        if (flushTimer !== null) clearTimeout(flushTimer);
        flushTimer = setTimeout(flush, debounceMs);
    }

    function watchDirectory(directory) {
        if (closed || watchers.has(directory) || watchers.size >= maxDirectories) return;
        let watcher;
        try {
            watcher = fs.watch(directory, { persistent: false }, (eventType, filename) => {
                if (closed || !filename) return;
                const absolutePath = path.join(directory, filename.toString());
                enqueue(absolutePath, eventType);
                if (eventType === 'rename') {
                    setTimeout(() => discoverDirectories(absolutePath), debounceMs);
                }
            });
        } catch {
            return;
        }
        watcher.on('error', () => {
            watcher.close();
            watchers.delete(directory);
        });
        watchers.set(directory, watcher);
    }

    async function discoverDirectories(startPath) {
        const queue = [startPath];
        while (queue.length > 0) {
            if (closed || watchers.size >= maxDirectories) return;
            const next = queue.shift();
            let stats;
            try { stats = await fs.promises.stat(next); } catch { continue; }
            if (!stats.isDirectory()) continue;
            let canonical;
            try { canonical = await fs.promises.realpath(next); } catch { continue; }
            if (!isWithinRoot(root, canonical) || IGNORED_DIRECTORY_NAMES.has(path.basename(canonical))) continue;
            watchDirectory(canonical);
            let entries;
            try { entries = await fs.promises.readdir(canonical, { withFileTypes: true }); } catch { continue; }
            for (const entry of entries) {
                if (!entry.isDirectory() || entry.isSymbolicLink() || IGNORED_DIRECTORY_NAMES.has(entry.name)) continue;
                queue.push(path.join(canonical, entry.name));
            }
        }
    }

    const discovered = discoverDirectories(root);

    return {
        subscriptionId,
        root,
        ready: discovered,
        get watchedDirectoryCount() { return watchers.size; },
        isWatching(relativeDirectory) { return !closed && watchers.has(relativeDirectory ? path.join(root, relativeDirectory) : root); },
        close() {
            if (closed) return;
            closed = true;
            if (flushTimer !== null) clearTimeout(flushTimer);
            flushTimer = null;
            pending.clear();
            for (const watcher of watchers.values()) watcher.close();
            watchers.clear();
        },
    };
}

module.exports = { createWorkspaceWatcher };
