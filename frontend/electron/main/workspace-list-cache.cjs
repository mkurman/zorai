const path = require('path');

const UNWATCHED_TTL_MS = 2000;
const WATCHED_TTL_MS = 30000;

function normalizeRelative(relativePath) {
    const normalized = path.normalize(String(relativePath || '')).replace(/\\/g, '/').replace(/^\.\/?/, '').replace(/\/+$/, '');
    return normalized === '.' ? '' : normalized;
}

function createWorkspaceListCache({ canonicalRoot, list, now = () => Date.now(), unwatchedTtlMs = UNWATCHED_TTL_MS, watchedTtlMs = WATCHED_TTL_MS }) {
    const entries = new Map();
    const watchedRoots = new Map();

    const keyFor = (root, relativePath, includeIgnored) => `${root}\0${normalizeRelative(relativePath)}\0${includeIgnored ? 1 : 0}`;

    function invalidateDirectory(root, relativePath) {
        const prefix = `${root}\0${normalizeRelative(relativePath)}\0`;
        for (const key of entries.keys()) if (key.startsWith(prefix)) entries.delete(key);
    }

    return {
        async listDirectory(rootPath, relativePath = '', options = {}) {
            const root = canonicalRoot(rootPath);
            if (options.fresh === true) {
                const prefix = `${root}\0`;
                for (const key of entries.keys()) if (key.startsWith(prefix)) entries.delete(key);
            }
            const key = keyFor(root, relativePath, options.includeIgnored === true);
            const cached = entries.get(key);
            if (cached && now() - cached.at < (watchedRoots.has(root) ? watchedTtlMs : unwatchedTtlMs)) return cached.value;
            const value = await list(rootPath, relativePath, options);
            entries.set(key, { value, at: now() });
            return value;
        },
        watchStarted(root) {
            watchedRoots.set(root, (watchedRoots.get(root) ?? 0) + 1);
        },
        watchStopped(root) {
            const count = (watchedRoots.get(root) ?? 0) - 1;
            if (count > 0) watchedRoots.set(root, count);
            else watchedRoots.delete(root);
        },
        applyChanges(root, changes) {
            for (const change of changes ?? []) {
                const relative = normalizeRelative(change.path);
                invalidateDirectory(root, path.posix.dirname(relative) === '.' ? '' : path.posix.dirname(relative));
                invalidateDirectory(root, relative);
            }
        },
        invalidateRoot(rootPath) {
            let root;
            try { root = canonicalRoot(rootPath); } catch { return; }
            const prefix = `${root}\0`;
            for (const key of entries.keys()) if (key.startsWith(prefix)) entries.delete(key);
        },
    };
}

module.exports = { createWorkspaceListCache, normalizeRelative };
