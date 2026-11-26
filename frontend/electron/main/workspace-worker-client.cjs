const path = require('path');

function rebuildError(payload) {
    const error = new Error(payload?.message || 'Workspace worker request failed.');
    if (payload?.fields && typeof payload.fields === 'object') Object.assign(error, payload.fields);
    return error;
}

function createWorkspaceWorkerClient({ fork, entryPath = path.join(__dirname, 'workspace-worker.cjs'), askpassEnv = () => null, onWatchBatch = () => {}, onSearchBatch = () => {}, log = () => {} }) {
    let child = null;
    let nextId = 1;
    const pending = new Map();

    function failPending(message) {
        for (const { reject } of pending.values()) reject(new Error(message));
        pending.clear();
    }

    function ensureChild() {
        if (child) return child;
        const spawned = fork(entryPath, [], { serviceName: 'zorai-workspace' });
        child = spawned;
        spawned.on('message', (message) => {
            if (message?.type === 'search-batch') {
                onSearchBatch(message);
                return;
            }
            if (message?.type === 'watch-batch') {
                onWatchBatch(message.batch);
                return;
            }
            if (message?.type !== 'result') return;
            const entry = pending.get(message.id);
            if (!entry) return;
            pending.delete(message.id);
            if (message.ok) entry.resolve(message.result);
            else entry.reject(rebuildError(message.error));
        });
        spawned.on('exit', (code) => {
            if (child === spawned) child = null;
            log('workspace worker exited', { code });
            failPending('Workspace worker stopped; try again.');
        });
        const env = askpassEnv();
        if (env) post(spawned, 'setAskpassEnv', [env]).catch(() => {});
        return spawned;
    }

    function post(target, method, args) {
        const id = nextId++;
        return new Promise((resolve, reject) => {
            pending.set(id, { resolve, reject });
            target.postMessage({ id, method, args });
        });
    }

    return {
        call(method, ...args) {
            return post(ensureChild(), method, args);
        },
        stop() {
            if (!child) return;
            const current = child;
            child = null;
            failPending('Workspace worker stopped.');
            current.kill();
        },
    };
}

function workspaceServiceProxy(client, methodNames) {
    return Object.fromEntries(methodNames.map((name) => [name, (...args) => client.call(name, ...args)]));
}

module.exports = { createWorkspaceWorkerClient, workspaceServiceProxy };
