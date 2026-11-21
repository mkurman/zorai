const { spawn, spawnSync } = require('child_process');
const path = require('path');
const { pathToFileURL, fileURLToPath } = require('url');
const { canonicalWorkspaceRoot, isWithinRoot, resolveWorkspacePath } = require('./workspace-service.cjs');

const MAX_LSP_MESSAGE_BYTES = 16 * 1024 * 1024;

class LspMessageReader {
    constructor(onMessage) {
        this.buffer = Buffer.alloc(0);
        this.onMessage = onMessage;
    }

    push(chunk) {
        this.buffer = Buffer.concat([this.buffer, Buffer.from(chunk)]);
        while (this.buffer.length > 0) {
            const headerEnd = this.buffer.indexOf('\r\n\r\n');
            if (headerEnd < 0) return;
            const header = this.buffer.subarray(0, headerEnd).toString('ascii');
            const match = header.match(/(?:^|\r\n)Content-Length:\s*(\d+)/i);
            if (!match) throw new Error('LSP frame is missing Content-Length.');
            const length = Number(match[1]);
            if (!Number.isFinite(length) || length < 0 || length > MAX_LSP_MESSAGE_BYTES) {
                throw new Error(`Invalid LSP Content-Length: ${match[1]}`);
            }
            const bodyStart = headerEnd + 4;
            if (this.buffer.length < bodyStart + length) return;
            const body = this.buffer.subarray(bodyStart, bodyStart + length).toString('utf8');
            this.buffer = this.buffer.subarray(bodyStart + length);
            this.onMessage(JSON.parse(body));
        }
    }
}

function encodeLspMessage(message) {
    const body = Buffer.from(JSON.stringify(message), 'utf8');
    return Buffer.concat([Buffer.from(`Content-Length: ${body.length}\r\n\r\n`, 'ascii'), body]);
}

function resolveCommandPath(command) {
    const checker = process.platform === 'win32' ? 'where' : 'which';
    const result = spawnSync(checker, [command], { encoding: 'utf8' });
    if (result.status !== 0) return null;
    return String(result.stdout || '').split(/\r?\n/).map((value) => value.trim()).find(Boolean) ?? null;
}

function usableLanguageServerCommand(command) {
    const resolved = resolveCommandPath(command);
    if (!resolved) return null;
    // The Ubuntu/Snap rust-analyzer shim recursively invokes itself when the
    // active rustup toolchain does not contain the component.
    if (command === 'rust-analyzer' && resolved.replace(/\\/g, '/') === '/snap/bin/rust-analyzer') {
        const rustup = resolveCommandPath('rustup');
        if (!rustup) return null;
        const toolchainBinary = spawnSync(rustup, ['which', 'rust-analyzer'], { encoding: 'utf8', timeout: 5000 });
        if (toolchainBinary.status !== 0) return null;
        return String(toolchainBinary.stdout || '').trim() || null;
    }
    return resolved;
}

function languageServerSpec(language) {
    const normalized = String(language || '').toLowerCase();
    if (normalized === 'rust') return { id: 'rust', languageId: 'rust', candidates: [{ command: 'rust-analyzer', args: [] }] };
    if (normalized === 'python') return {
        id: 'python', languageId: 'python', candidates: [
            { command: 'basedpyright-langserver', args: ['--stdio'] },
            { command: 'pyright-langserver', args: ['--stdio'] },
        ],
    };
    if (normalized === 'go') return { id: 'go', languageId: 'go', candidates: [{ command: 'gopls', args: [] }] };
    if (['c', 'cpp', 'objective-c'].includes(normalized)) return { id: 'cpp', languageId: normalized === 'c' ? 'c' : 'cpp', candidates: [{ command: 'clangd', args: ['--background-index'] }] };
    return null;
}

const LANGUAGE_SERVER_CACHE_TTL_MS = 60000;
const languageServerCache = new Map();

function resolveLanguageServer(language) {
    const cacheKey = String(language || '').toLowerCase();
    const cached = languageServerCache.get(cacheKey);
    if (cached && Date.now() - cached.at < LANGUAGE_SERVER_CACHE_TTL_MS) return cached.server;
    const server = resolveLanguageServerUncached(language);
    languageServerCache.set(cacheKey, { server, at: Date.now() });
    return server;
}

function resolveLanguageServerUncached(language) {
    const spec = languageServerSpec(language);
    if (!spec) return null;
    const candidate = spec.candidates
        .map((entry) => ({ ...entry, resolvedCommand: usableLanguageServerCommand(entry.command) }))
        .find((entry) => entry.resolvedCommand);
    return candidate
        ? { ...spec, command: candidate.resolvedCommand, displayCommand: candidate.command, args: candidate.args }
        : { ...spec, command: null, displayCommand: null, args: [] };
}

function normalizeDiagnostic(diagnostic) {
    const range = diagnostic?.range ?? {};
    const start = range.start ?? {};
    const end = range.end ?? start;
    return {
        message: String(diagnostic?.message ?? 'Language server diagnostic'),
        severity: Number(diagnostic?.severity) || 3,
        source: typeof diagnostic?.source === 'string' ? diagnostic.source : null,
        code: diagnostic?.code === undefined || diagnostic?.code === null ? null : String(diagnostic.code),
        startLine: Math.max(1, Number(start.line) + 1 || 1),
        startColumn: Math.max(1, Number(start.character) + 1 || 1),
        endLine: Math.max(1, Number(end.line) + 1 || 1),
        endColumn: Math.max(1, Number(end.character) + 1 || 1),
    };
}

class LspSession {
    constructor(root, server, emitDiagnostics, removeSession) {
        this.root = root;
        this.server = server;
        this.emitDiagnostics = emitDiagnostics;
        this.removeSession = removeSession;
        this.process = null;
        this.reader = new LspMessageReader((message) => this.handleMessage(message));
        this.nextRequestId = 1;
        this.pending = new Map();
        this.documents = new Map();
        this.subscribers = new Set();
        this.ready = null;
        this.closed = false;
    }

    start() {
        if (this.ready) return this.ready;
        this.ready = new Promise((resolve, reject) => {
            const child = spawn(this.server.command, this.server.args, {
                cwd: this.root,
                windowsHide: true,
                stdio: ['pipe', 'pipe', 'pipe'],
            });
            this.process = child;
            this.stderrBuffer = '';
            child.stdout.on('data', (chunk) => {
                try { this.reader.push(chunk); } catch (error) { this.fail(error); }
            });
            child.stderr.on('data', (chunk) => {
                this.stderrBuffer = `${this.stderrBuffer}${chunk.toString('utf8')}`.slice(-8000);
            });
            child.once('error', (error) => { reject(error); this.fail(error); });
            child.once('exit', (code, signal) => {
                const details = this.stderrBuffer?.trim();
                if (!this.closed) this.fail(new Error(`${this.server.command} exited (${code ?? signal ?? 'unknown'})${details ? `: ${details}` : ''}`));
                this.removeSession();
            });
            this.request('initialize', {
                processId: process.pid,
                clientInfo: { name: 'zorai', version: '0.9.45' },
                rootUri: pathToFileURL(this.root).href,
                workspaceFolders: [{ uri: pathToFileURL(this.root).href, name: path.basename(this.root) }],
                capabilities: {
                    workspace: { workspaceFolders: true, configuration: true },
                    textDocument: { publishDiagnostics: { relatedInformation: true, versionSupport: true } },
                },
                initializationOptions: {},
            }).then((result) => {
                this.notify('initialized', {});
                resolve(result);
            }).catch(reject);
        });
        return this.ready;
    }

    send(message) {
        if (!this.process?.stdin?.writable) throw new Error('Language server stdin is unavailable.');
        this.process.stdin.write(encodeLspMessage(message));
    }

    request(method, params) {
        const id = this.nextRequestId++;
        return new Promise((resolve, reject) => {
            this.pending.set(id, { resolve, reject });
            this.send({ jsonrpc: '2.0', id, method, params });
        });
    }

    notify(method, params) {
        this.send({ jsonrpc: '2.0', method, params });
    }

    handleMessage(message) {
        if (message?.id !== undefined && this.pending.has(message.id)) {
            const pending = this.pending.get(message.id);
            this.pending.delete(message.id);
            if (message.error) pending.reject(new Error(message.error.message || 'Language server request failed.'));
            else pending.resolve(message.result);
            return;
        }
        if (message?.method === 'textDocument/publishDiagnostics') {
            let absolutePath;
            try { absolutePath = fileURLToPath(message.params.uri); } catch { return; }
            if (!isWithinRoot(this.root, absolutePath)) return;
            this.emitDiagnostics(this, {
                root: this.root,
                language: this.server.id,
                path: path.relative(this.root, absolutePath),
                version: message.params.version ?? null,
                diagnostics: Array.isArray(message.params.diagnostics) ? message.params.diagnostics.map(normalizeDiagnostic) : [],
            });
            return;
        }
        if (message?.method === 'workspace/configuration' && message.id !== undefined) {
            this.send({ jsonrpc: '2.0', id: message.id, result: (message.params?.items ?? []).map(() => null) });
            return;
        }
        if (message?.id !== undefined && [
            'client/registerCapability',
            'client/unregisterCapability',
            'window/workDoneProgress/create',
        ].includes(message.method)) {
            this.send({ jsonrpc: '2.0', id: message.id, result: null });
            return;
        }
        if (message?.id !== undefined && message?.method === 'workspace/applyEdit') {
            this.send({ jsonrpc: '2.0', id: message.id, result: { applied: false, failureReason: 'Workspace edits require explicit operator review in zorai.' } });
        }
    }

    async openDocument(relativePath, content, version = 1) {
        await this.start();
        const resolved = resolveWorkspacePath(this.root, relativePath);
        const uri = pathToFileURL(resolved.absolutePath).href;
        const existing = this.documents.get(resolved.relativePath);
        if (!existing) {
            this.documents.set(resolved.relativePath, { uri, version });
            this.notify('textDocument/didOpen', { textDocument: { uri, languageId: this.server.languageId, version, text: content } });
        } else {
            existing.version = Math.max(existing.version + 1, version);
            this.notify('textDocument/didChange', { textDocument: { uri, version: existing.version }, contentChanges: [{ text: content }] });
        }
        return { path: resolved.relativePath, version: this.documents.get(resolved.relativePath).version };
    }

    async changeDocument(relativePath, content, version) {
        return this.openDocument(relativePath, content, version);
    }

    async requestDocument(relativePath, method, params) {
        await this.start();
        const resolved = resolveWorkspacePath(this.root, relativePath);
        const document = this.documents.get(resolved.relativePath);
        const textDocument = { uri: document?.uri ?? pathToFileURL(resolved.absolutePath).href };
        return this.request(method, { textDocument, ...params });
    }

    async hover(relativePath, position) {
        return this.requestDocument(relativePath, 'textDocument/hover', { position });
    }

    async definition(relativePath, position) {
        return this.requestDocument(relativePath, 'textDocument/definition', { position });
    }

    async references(relativePath, position) {
        return this.requestDocument(relativePath, 'textDocument/references', { position, context: { includeDeclaration: true } });
    }

    async completion(relativePath, position) {
        return this.requestDocument(relativePath, 'textDocument/completion', { position, context: { triggerKind: 1 } });
    }

    closeDocument(relativePath) {
        const resolved = resolveWorkspacePath(this.root, relativePath);
        const document = this.documents.get(resolved.relativePath);
        if (!document) return false;
        this.documents.delete(resolved.relativePath);
        this.notify('textDocument/didClose', { textDocument: { uri: document.uri } });
        return true;
    }

    fail(error) {
        for (const pending of this.pending.values()) pending.reject(error);
        this.pending.clear();
    }

    async stop() {
        if (this.closed) return;
        this.closed = true;
        try { await Promise.race([this.request('shutdown', null), new Promise((resolve) => setTimeout(resolve, 500))]); } catch {}
        try { this.notify('exit', null); } catch {}
        setTimeout(() => { if (this.process && !this.process.killed) this.process.kill(); }, 250);
    }
}

function createLspRuntime() {
    const sessions = new Map();

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
        if (!server.command) return { available: false, reason: `${server.candidates.map((entry) => entry.command).join(' or ')} is not installed.` };
        const sessionKey = key(root, server.id);
        let session = sessions.get(sessionKey);
        if (!session) {
            session = new LspSession(root, server, emitDiagnostics, () => sessions.delete(sessionKey));
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
        async request(webContents, rootPath, relativePath, language, method, position) {
            const resolved = await getSession(webContents, rootPath, language);
            if (!resolved.available) return resolved;
            if (!['hover', 'definition', 'references', 'completion'].includes(method)) throw new Error(`Unsupported LSP request: ${method}`);
            const result = await resolved.session[method](relativePath, {
                line: Math.max(0, Number(position?.line) || 0),
                character: Math.max(0, Number(position?.character) || 0),
            });
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
        async stopAll() {
            await Promise.all([...sessions.values()].map((session) => session.stop()));
            sessions.clear();
        },
    };
}

module.exports = { LspMessageReader, createLspRuntime, encodeLspMessage, languageServerSpec, normalizeDiagnostic, resolveLanguageServer, usableLanguageServerCommand };
