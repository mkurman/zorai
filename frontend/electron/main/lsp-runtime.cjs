const { canonicalWorkspaceRoot } = require('./workspace-service.cjs');

const LSP_LOG_LIMIT = 1000;
const { installLanguageServer, languageServerSpec, languageServerStatuses, resolveLanguageServer, typeScriptLanguageId, usableLanguageServerCommand } = require('./lsp-registry.cjs');
const { LSP_METHOD_NOT_FOUND, LspMessageReader, LspSession, encodeLspMessage, normalizeDiagnostic, serverSupports } = require('./lsp-session.cjs');

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
            if (!['hover', 'definition', 'references', 'completion', 'rename', 'codeAction', 'formatting', 'documentSymbol', 'workspaceSymbol'].includes(method)) throw new Error(`Unsupported LSP request: ${method}`);
            if (!serverSupports(resolved.session.capabilities, method)) return { available: true, root: resolved.root, server: resolved.server, result: null, unsupported: true };
            let result;
            try {
                result = await resolved.session[method](relativePath, {
                    line: Math.max(0, Number(position?.line) || 0),
                    character: Math.max(0, Number(position?.character) || 0),
                }, extra);
            } catch (error) {
                if (error?.code !== LSP_METHOD_NOT_FOUND) throw error;
                return { available: true, root: resolved.root, server: resolved.server, result: null, unsupported: true };
            }
            return { available: true, root: resolved.root, server: resolved.server, result };
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
