const { canonicalWorkspaceRoot } = require('./workspace-service.cjs');

const LSP_LOG_LIMIT = 1000;
const { installLanguageServer, languageServerSpec, languageServerStatuses, resolveLanguageServer, typeScriptLanguageId, usableLanguageServerCommand } = require('./lsp-registry.cjs');
const { LSP_METHOD_NOT_FOUND, LspMessageReader, LspSession, encodeLspMessage, normalizeDiagnostic, serverSupports } = require('./lsp-session.cjs');

const LSP_REQUEST_METHODS = ['hover', 'definition', 'references', 'completion', 'completionResolve', 'rename', 'prepareRename', 'codeAction', 'formatting', 'rangeFormatting', 'documentSymbol', 'workspaceSymbol', 'signatureHelp', 'inlayHint', 'semanticTokens', 'typeDefinition', 'implementation', 'serverCapabilities', 'prepareCallHierarchy', 'incomingCalls', 'outgoingCalls', 'foldingRange', 'linkedEditingRange', 'onTypeFormatting', 'codeLens', 'codeLensResolve', 'executeCommand'];

function createLspRuntime() {
    const sessions = new Map();
    const logEntries = [];
    const appendLog = (entry) => {
        logEntries.push(entry);
        if (logEntries.length > LSP_LOG_LIMIT) logEntries.splice(0, logEntries.length - LSP_LOG_LIMIT);
    };

    function key(root, serverId) { return `${root}\0${serverId}`; }
    function emitDiagnostics(session, payload) {
        for (const webContents of session.subscribers) {
            if (!webContents.isDestroyed()) webContents.send('workspace-lsp-diagnostics', payload);
        }
    }
    async function getSession(webContents, rootPath, language) {
        const root = canonicalWorkspaceRoot(rootPath);
        const server = resolveLanguageServer(language);
        if (!server) return { available: false, reason: `No LSP integration is configured for ${language}.` };
        if (!server.command) return { available: false, server: server.installId ?? server.id, missing: true, reason: `${server.candidates.map((entry) => entry.command).join(' or ')} is not installed.` };
        const sessionKey = key(root, server.id);
        let session = sessions.get(sessionKey);
        if (!session) {
            session = new LspSession(root, server, emitDiagnostics, () => sessions.delete(sessionKey), appendLog);
            sessions.set(sessionKey, session);
        }
        session.subscribers.add(webContents);
        await session.start();
        return { available: true, root, server: server.id, command: server.command, session };
    }
    return {
        async status(rootPath, language) {
            const root = canonicalWorkspaceRoot(rootPath);
            const server = resolveLanguageServer(language);
            return { root, language, configured: Boolean(server), available: Boolean(server?.command), command: server?.command ?? null, server: server?.id ?? null };
        },
        async open(webContents, rootPath, relativePath, language, content, version) {
            const resolved = await getSession(webContents, rootPath, language);
            if (!resolved.available) return resolved;
            return { ...resolved, session: undefined, document: await resolved.session.openDocument(relativePath, String(content ?? ''), version) };
        },
        async change(webContents, rootPath, relativePath, language, content, version) {
            return this.open(webContents, rootPath, relativePath, language, content, version);
        },
        async request(webContents, rootPath, relativePath, language, method, position, extra) {
            const resolved = await getSession(webContents, rootPath, language);
            if (!resolved.available) return resolved;
            if (!LSP_REQUEST_METHODS.includes(method)) throw new Error(`Unsupported LSP request: ${method}`);
            if (!serverSupports(resolved.session.capabilities, method)) return { available: true, root: resolved.root, server: resolved.server, result: null, unsupported: true };
            let result;
            const startedAt = Date.now();
            try {
                result = await resolved.session[method](relativePath, {
                    line: Math.max(0, Number(position?.line) || 0),
                    character: Math.max(0, Number(position?.character) || 0),
                }, extra);
            } catch (error) {
                if (error?.code !== LSP_METHOD_NOT_FOUND) throw error;
                return { available: true, root: resolved.root, server: resolved.server, result: null, unsupported: true };
            }
            const elapsed = Date.now() - startedAt;
            const size = elapsed > 1000 || method === 'semanticTokens' || method === 'documentSymbol' ? JSON.stringify(result ?? null).length : 0;
            if (elapsed > 1000 || size > 1024 * 1024) appendLog({ at: Date.now(), server: resolved.server, root: resolved.root, level: 'warning', text: `Slow ${method} for ${relativePath}: ${elapsed} ms, ${Math.round(size / 1024)} KB` });
            return { available: true, root: resolved.root, server: resolved.server, result };
        },
        save(rootPath, relativePath, language) {
            const root = canonicalWorkspaceRoot(rootPath);
            const server = resolveLanguageServer(language);
            const session = server ? sessions.get(key(root, server.id)) : null;
            return session ? session.saveDocument(relativePath) : false;
        },
        filesChanged(rootPath, changes) {
            const root = canonicalWorkspaceRoot(rootPath);
            for (const session of sessions.values()) {
                if (session.root === root) session.filesChanged(changes);
            }
        },
        close(rootPath, relativePath, language) {
            const root = canonicalWorkspaceRoot(rootPath);
            const server = resolveLanguageServer(language);
            const session = server ? sessions.get(key(root, server.id)) : null;
            return session ? session.closeDocument(relativePath) : false;
        },
        async unsubscribe(webContents, rootPath, language) {
            const root = canonicalWorkspaceRoot(rootPath);
            const server = resolveLanguageServer(language);
            const session = server ? sessions.get(key(root, server.id)) : null;
            if (!session) return false;
            session.subscribers.delete(webContents);
            if (session.subscribers.size === 0) {
                await session.stop();
                sessions.delete(key(root, server.id));
            }
            return true;
        },
        logs() {
            return logEntries.slice();
        },
        servers() {
            return languageServerStatuses();
        },
        install(id) {
            return installLanguageServer(String(id));
        },
        async stopAll() {
            await Promise.all([...sessions.values()].map((session) => session.stop()));
            sessions.clear();
        },
    };
}

module.exports = { serverSupports, typeScriptLanguageId, LspMessageReader, createLspRuntime, encodeLspMessage, languageServerSpec, normalizeDiagnostic, resolveLanguageServer, usableLanguageServerCommand };
