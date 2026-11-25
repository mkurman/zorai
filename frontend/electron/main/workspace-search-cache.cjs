const fs = require('fs');
const path = require('path');

const MAX_SEARCH_FILE_BYTES = 1024 * 1024;
const MAX_CACHED_TEXT_CHARS = 256 * 1024 * 1024;
const FILE_LIST_TTL_MS = 30000;
const READ_CONCURRENCY = 32;

function createWorkspaceSearchCache({
    canonicalRoot,
    listFiles,
    collectMatches,
    searchPattern,
    globMatcher,
    isBinary,
    isTrusted = () => false,
    fsPromises = fs.promises,
    now = () => Date.now(),
    maxCachedChars = MAX_CACHED_TEXT_CHARS,
}) {
    const roots = new Map();
    let cachedChars = 0;

    const stateFor = (root) => {
        let state = roots.get(root);
        if (!state) {
            state = { texts: new Map(), lists: new Map() };
            roots.set(root, state);
        }
        return state;
    };

    const forget = (state, relativePath) => {
        const entry = state.texts.get(relativePath);
        if (!entry) return;
        cachedChars -= entry.text ? entry.text.length : 0;
        state.texts.delete(relativePath);
    };

    const forgetAll = (state) => {
        for (const relativePath of [...state.texts.keys()]) forget(state, relativePath);
        state.lists.clear();
    };

    async function textFor(root, state, relativePath) {
        const cached = state.texts.get(relativePath);
        if (cached && isTrusted(root, path.posix.dirname(relativePath) === '.' ? '' : path.posix.dirname(relativePath))) return cached.text;
        const absolutePath = path.join(root, relativePath);
        let stats;
        try { stats = await fsPromises.stat(absolutePath); } catch { forget(state, relativePath); return null; }
        if (!stats.isFile()) { forget(state, relativePath); return null; }
        if (cached && cached.size === stats.size && cached.mtimeMs === stats.mtimeMs) return cached.text;
        forget(state, relativePath);
        let text = null;
        if (stats.size <= MAX_SEARCH_FILE_BYTES) {
            try {
                const buffer = await fsPromises.readFile(absolutePath);
                if (!isBinary(buffer)) text = buffer.toString('utf8');
            } catch {}
        }
        if (text && cachedChars + text.length > maxCachedChars) return text;
        state.texts.set(relativePath, { size: stats.size, mtimeMs: stats.mtimeMs, text });
        cachedChars += text ? text.length : 0;
        return text;
    }

    async function filesFor(root, state, useIgnoreFiles) {
        const key = useIgnoreFiles ? 'ignore' : 'all';
        const cached = state.lists.get(key);
        if (cached && now() - cached.at < FILE_LIST_TTL_MS) return cached.files;
        const listing = await listFiles(root, { useIgnoreFiles });
        state.lists.set(key, { files: listing.files, at: now() });
        return listing.files;
    }

    return {
        async search(rootPath, query, options = {}) {
            const root = canonicalRoot(rootPath);
            const needle = typeof query === 'string' ? query : '';
            if (!needle.trim()) return [];
            const state = stateFor(root);
            if (options.fresh === true) forgetAll(state);
            const pattern = searchPattern(needle, options);
            const included = globMatcher(options.include);
            const excluded = globMatcher(options.exclude);
            const maxResults = Math.max(1, Math.min(Number(options.maxResults) || 100, 5000));
            const maxFiles = Math.max(1, Math.min(Number(options.maxFiles) || 200000, 200000));
            const isExcluded = (relativePath) => {
                if (!excluded) return false;
                const parts = relativePath.split('/');
                return parts.some((_, index) => excluded(parts.slice(0, index + 1).join('/')));
            };
            const files = (await filesFor(root, state, options.useIgnoreFiles !== false))
                .filter((relativePath) => !isExcluded(relativePath) && (!included || included(relativePath)))
                .slice(0, maxFiles);

            const perFile = new Array(files.length);
            let next = 0;
            let found = 0;
            const worker = async () => {
                while (next < files.length && found < maxResults) {
                    const index = next;
                    next += 1;
                    const text = await textFor(root, state, files[index]);
                    if (!text) continue;
                    const matches = collectMatches(files[index], text, new RegExp(pattern.source, pattern.flags), [], maxResults);
                    perFile[index] = matches;
                    found += matches.length;
                }
            };
            await Promise.all(Array.from({ length: Math.min(READ_CONCURRENCY, files.length) }, worker));
            const results = [];
            for (let index = 0; index < next && results.length < maxResults; index += 1) {
                for (const match of perFile[index] ?? []) {
                    if (results.length >= maxResults) break;
                    results.push(match);
                }
            }
            return results;
        },
        async warm(rootPath) {
            const root = canonicalRoot(rootPath);
            const state = stateFor(root);
            const files = await filesFor(root, state, true);
            let next = 0;
            const worker = async () => {
                while (next < files.length) {
                    const index = next;
                    next += 1;
                    await textFor(root, state, files[index]);
                }
            };
            await Promise.all(Array.from({ length: Math.min(4, files.length) }, worker));
            return files.length;
        },
        applyChanges(rootPath, changes) {
            let root;
            try { root = canonicalRoot(rootPath); } catch { return; }
            const state = roots.get(root);
            if (!state) return;
            for (const change of changes ?? []) {
                const relativePath = String(change?.path ?? '').replace(/\\/g, '/').replace(/^\.\/?/, '').replace(/\/+$/, '');
                forget(state, relativePath);
                const prefix = `${relativePath}/`;
                for (const key of [...state.texts.keys()]) if (key.startsWith(prefix)) forget(state, key);
                if (change?.eventType === 'rename') state.lists.clear();
            }
        },
        forgetPaths(rootPath, relativePaths) {
            this.applyChanges(rootPath, (relativePaths ?? []).map((relativePath) => ({ path: relativePath, eventType: 'change' })));
        },
        invalidateRoot(rootPath) {
            let root;
            try { root = canonicalRoot(rootPath); } catch { return; }
            const state = roots.get(root);
            if (state) forgetAll(state);
        },
        stats() {
            let files = 0;
            for (const state of roots.values()) files += state.texts.size;
            return { files, cachedChars };
        },
    };
}

module.exports = { createWorkspaceSearchCache, MAX_SEARCH_FILE_BYTES };
