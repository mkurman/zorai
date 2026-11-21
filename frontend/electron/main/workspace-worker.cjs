const workspaceService = require('./workspace-service.cjs');
const { setGitAskpassEnv } = require('./git-askpass.cjs');
const { createWorkspaceWatcher } = require('./workspace-watch-service.cjs');
const { discoverWorkspaceTests } = require('./test-runtime.cjs');
const { createWorkspaceListCache } = require('./workspace-list-cache.cjs');

const MUTATING_METHODS = new Set([
    'createWorkspaceDirectory', 'deleteWorkspacePath', 'renameWorkspacePath', 'writeWorkspaceFile',
    'workspaceGitAction', 'workspaceGitApplyHunk', 'workspaceGitCheckout', 'workspaceGitCommit', 'workspaceGitCreateWorktree',
    'workspaceGitDiscard', 'workspaceGitIntegrateWorktree', 'workspaceGitRemoveWorktree', 'workspaceGitStage', 'workspaceGitStageMany',
    'workspaceGitUnstage', 'workspaceGitUnstageMany',
]);

const listCache = createWorkspaceListCache({
    canonicalRoot: workspaceService.canonicalWorkspaceRoot,
    list: workspaceService.listWorkspaceDirectory,
});

const watchers = new Map();

function post(message) {
    process.parentPort.postMessage(message);
}

const handlers = {
    ...workspaceService,
    listWorkspaceDirectory: listCache.listDirectory,
    discoverTests: discoverWorkspaceTests,
    setAskpassEnv(env) {
        setGitAskpassEnv(env);
        return true;
    },
    watchStart(rootPath, options) {
        const watcher = createWorkspaceWatcher(rootPath, (batch) => {
            listCache.applyChanges(batch.root, batch.changes);
            post({ type: 'watch-batch', batch });
        }, options);
        watchers.set(watcher.subscriptionId, watcher);
        void watcher.ready.then(() => {
            if (watchers.get(watcher.subscriptionId) === watcher && watcher.watchedDirectoryCount > 0) {
                watcher.countedForCache = true;
                listCache.watchStarted(watcher.root);
            }
        });
        return { subscriptionId: watcher.subscriptionId, root: watcher.root, watchedDirectoryCount: watcher.watchedDirectoryCount };
    },
    watchStop(subscriptionId) {
        const watcher = watchers.get(subscriptionId);
        if (!watcher) return false;
        watcher.close();
        watchers.delete(subscriptionId);
        if (watcher.countedForCache) listCache.watchStopped(watcher.root);
        return true;
    },
};

process.parentPort.on('message', async ({ data }) => {
    const { id, method, args } = data ?? {};
    const handler = handlers[method];
    try {
        if (typeof handler !== 'function') throw new Error(`Unknown workspace worker method: ${method}`);
        const callArgs = Array.isArray(args) ? args : [];
        const result = await handler(...callArgs);
        if (MUTATING_METHODS.has(method)) listCache.invalidateRoot(callArgs[0]);
        post({ type: 'result', id, ok: true, result });
    } catch (error) {
        post({
            type: 'result',
            id,
            ok: false,
            error: { message: error?.message || String(error), fields: error && typeof error === 'object' ? { ...error } : {} },
        });
    }
});
