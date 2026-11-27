const { spawn } = require('child_process');
const path = require('path');
const { pathToFileURL, fileURLToPath } = require('url');
const { isWithinRoot, resolveWorkspacePath } = require('./workspace-service.cjs');

const isNotebookShadowPath = (relativePath) => /\.ipynb\.zorai-notebook\.py$/.test(String(relativePath || ''));

const MAX_LSP_MESSAGE_BYTES = 16 * 1024 * 1024;
const LSP_RETRYABLE_CODES = new Set([-32801, -32802]);
const LSP_METHOD_NOT_FOUND = -32601;
const LSP_CAPABILITY_FOR_METHOD = {
    hover: 'hoverProvider',
    definition: 'definitionProvider',
    references: 'referencesProvider',
    completion: 'completionProvider',
    rename: 'renameProvider',
    codeAction: 'codeActionProvider',
    formatting: 'documentFormattingProvider',
    documentSymbol: 'documentSymbolProvider',
    workspaceSymbol: 'workspaceSymbolProvider',
    completionResolve: 'completionProvider',
    signatureHelp: 'signatureHelpProvider',
    inlayHint: 'inlayHintProvider',
    semanticTokens: 'semanticTokensProvider',
    typeDefinition: 'typeDefinitionProvider',
    implementation: 'implementationProvider',
    prepareRename: 'renameProvider',
    rangeFormatting: 'documentRangeFormattingProvider',
    serverCapabilities: null,
    foldingRange: 'foldingRangeProvider',
    linkedEditingRange: 'linkedEditingRangeProvider',
    onTypeFormatting: 'documentOnTypeFormattingProvider',
    codeLens: 'codeLensProvider',
    codeLensResolve: 'codeLensProvider',
    executeCommand: 'executeCommandProvider',
    prepareCallHierarchy: 'callHierarchyProvider',
    incomingCalls: 'callHierarchyProvider',
    outgoingCalls: 'callHierarchyProvider',
};
const SEMANTIC_TOKEN_TYPES = ['namespace', 'type', 'class', 'enum', 'interface', 'struct', 'typeParameter', 'parameter', 'variable', 'property', 'enumMember', 'event', 'function', 'method', 'macro', 'keyword', 'modifier', 'comment', 'string', 'number', 'regexp', 'operator', 'decorator'];
const SEMANTIC_TOKEN_MODIFIERS = ['declaration', 'definition', 'readonly', 'static', 'deprecated', 'abstract', 'async', 'modification', 'documentation', 'defaultLibrary'];

function serverSupports(capabilities, method) {
    if (!capabilities || LSP_CAPABILITY_FOR_METHOD[method] === null) return true;
    if (method === 'completionResolve') return capabilities.completionProvider?.resolveProvider === true;
    if (method === 'codeLensResolve') return capabilities.codeLensProvider?.resolveProvider === true;
    const value = capabilities[LSP_CAPABILITY_FOR_METHOD[method]];
    return value !== undefined && value !== null && value !== false;
}
const LSP_RETRY_ATTEMPTS = 5;
const LSP_RETRY_DELAY_MS = 300;

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

function syncKind(capabilities) {
    const sync = capabilities?.textDocumentSync;
    return typeof sync === 'number' ? sync : Number(sync?.change ?? 1);
}

function positionAt(text, offset) {
    let line = 0;
    let lineStart = 0;
    for (let index = text.indexOf('\n'); index !== -1 && index < offset; index = text.indexOf('\n', index + 1)) {
        line += 1;
        lineStart = index + 1;
    }
    return { line, character: offset - lineStart };
}

function incrementalChange(previous, next) {
    let start = 0;
    const limit = Math.min(previous.length, next.length);
    while (start < limit && previous.charCodeAt(start) === next.charCodeAt(start)) start += 1;
    if (start > 0 && start < limit && previous.charCodeAt(start - 1) >= 0xd800 && previous.charCodeAt(start - 1) <= 0xdbff) start -= 1;
    let previousEnd = previous.length;
    let nextEnd = next.length;
    while (previousEnd > start && nextEnd > start && previous.charCodeAt(previousEnd - 1) === next.charCodeAt(nextEnd - 1)) {
        previousEnd -= 1;
        nextEnd -= 1;
    }
    return { range: { start: positionAt(previous, start), end: positionAt(previous, previousEnd) }, text: next.slice(start, nextEnd) };
}

class LspSession {
    constructor(root, server, emitDiagnostics, removeSession, log = () => {}) {
        this.log = (level, text) => log({ at: Date.now(), server: server.id, root, level, text: String(text) });
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
            this.log('info', `Starting ${this.server.command} ${this.server.args.join(' ')}`.trim());
            child.stdout.on('data', (chunk) => {
                try { this.reader.push(chunk); } catch (error) { this.fail(error); }
            });
            child.stderr.on('data', (chunk) => {
                const text = chunk.toString('utf8');
                this.stderrBuffer = `${this.stderrBuffer}${text}`.slice(-8000);
                for (const line of text.split(/\r?\n/)) if (line.trim()) this.log('stderr', line);
            });
            child.once('error', (error) => { this.log('error', error.message); reject(error); this.fail(error); });
            child.once('exit', (code, signal) => {
                this.log(code === 0 || this.closed ? 'info' : 'error', `Exited (${code ?? signal ?? 'unknown'})`);
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
                    workspace: { workspaceFolders: true, configuration: true, symbol: {}, didChangeWatchedFiles: { dynamicRegistration: true }, workspaceEdit: { documentChanges: true, resourceOperations: ['create', 'rename', 'delete'] } },
                    textDocument: {
                        publishDiagnostics: { relatedInformation: true, versionSupport: true },
                        synchronization: { didSave: true, dynamicRegistration: false },
                        completion: {
                            completionItem: {
                                snippetSupport: true,
                                insertReplaceSupport: false,
                                documentationFormat: ['markdown', 'plaintext'],
                                resolveSupport: { properties: ['documentation', 'detail', 'additionalTextEdits'] },
                                labelDetailsSupport: true,
                            },
                            contextSupport: true,
                        },
                        signatureHelp: { signatureInformation: { documentationFormat: ['markdown', 'plaintext'], parameterInformation: { labelOffsetSupport: true }, activeParameterSupport: true }, contextSupport: true },
                        inlayHint: { dynamicRegistration: false },
                        semanticTokens: {
                            requests: { full: true, range: false },
                            tokenTypes: SEMANTIC_TOKEN_TYPES,
                            tokenModifiers: SEMANTIC_TOKEN_MODIFIERS,
                            formats: ['relative'],
                            multilineTokenSupport: false,
                            overlappingTokenSupport: false,
                        },
                        typeDefinition: { linkSupport: true },
                        implementation: { linkSupport: true },
                        definition: { linkSupport: true },
                        rangeFormatting: {},
                        callHierarchy: { dynamicRegistration: false },
                        foldingRange: { lineFoldingOnly: true },
                        linkedEditingRange: {},
                        onTypeFormatting: {},
                        codeLens: {},
                        rename: { prepareSupport: true },
                        codeAction: { codeActionLiteralSupport: { codeActionKind: { valueSet: ['', 'quickfix', 'refactor', 'refactor.extract', 'refactor.inline', 'refactor.rewrite', 'source', 'source.organizeImports'] } } },
                        formatting: {},
                        documentSymbol: { hierarchicalDocumentSymbolSupport: true },
                    },
                },
                initializationOptions: this.server.initializationOptions ?? {},
            }).then((result) => {
                this.capabilities = result?.capabilities ?? {};
                this.log('info', `Ready${result?.serverInfo?.name ? `: ${result.serverInfo.name}${result.serverInfo.version ? ` ${result.serverInfo.version}` : ''}` : ''}`);
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
            if (message.error) pending.reject(Object.assign(new Error(message.error.message || 'Language server request failed.'), { code: message.error.code }));
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
        if (message?.method === 'window/logMessage' || message?.method === 'window/showMessage') {
            const type = Number(message.params?.type);
            this.log(type === 1 ? 'error' : type === 2 ? 'warning' : 'info', message.params?.message ?? '');
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
        const resolved = resolveWorkspacePath(this.root, relativePath, { allowMissing: isNotebookShadowPath(relativePath) });
        const uri = pathToFileURL(resolved.absolutePath).href;
        const existing = this.documents.get(resolved.relativePath);
        if (!existing) {
            this.documents.set(resolved.relativePath, { uri, version, text: content });
            this.notify('textDocument/didOpen', { textDocument: { uri, languageId: this.server.languageIdFor?.(resolved.relativePath) ?? this.server.languageId, version, text: content } });
        } else if (existing.text !== content) {
            existing.version = Math.max(existing.version + 1, version);
            const change = syncKind(this.capabilities) === 2 ? incrementalChange(existing.text, content) : { text: content };
            existing.text = content;
            this.notify('textDocument/didChange', { textDocument: { uri, version: existing.version }, contentChanges: [change] });
        }
        return { path: resolved.relativePath, version: this.documents.get(resolved.relativePath).version };
    }

    async changeDocument(relativePath, content, version) {
        return this.openDocument(relativePath, content, version);
    }

    async requestDocument(relativePath, method, params) {
        await this.start();
        const resolved = resolveWorkspacePath(this.root, relativePath, { allowMissing: isNotebookShadowPath(relativePath) });
        const document = this.documents.get(resolved.relativePath);
        const textDocument = { uri: document?.uri ?? pathToFileURL(resolved.absolutePath).href };
        for (let attempt = 1; ; attempt += 1) {
            try {
                return await this.request(method, { textDocument, ...params });
            } catch (error) {
                if (attempt >= LSP_RETRY_ATTEMPTS || !LSP_RETRYABLE_CODES.has(error?.code)) throw error;
                await new Promise((resolve) => setTimeout(resolve, LSP_RETRY_DELAY_MS));
            }
        }
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

    async completion(relativePath, position, extra) {
        const triggerCharacter = typeof extra?.triggerCharacter === 'string' ? extra.triggerCharacter : undefined;
        return this.requestDocument(relativePath, 'textDocument/completion', { position, context: triggerCharacter ? { triggerKind: 2, triggerCharacter } : { triggerKind: 1 } });
    }

    async completionResolve(_relativePath, _position, extra) {
        await this.start();
        return this.request('completionItem/resolve', extra?.item ?? {});
    }

    async signatureHelp(relativePath, position, extra) {
        return this.requestDocument(relativePath, 'textDocument/signatureHelp', { position, context: extra?.context ?? { triggerKind: 1, isRetrigger: false } });
    }

    async inlayHint(relativePath, _position, extra) {
        return this.requestDocument(relativePath, 'textDocument/inlayHint', { range: extra?.range });
    }

    async semanticTokens(relativePath) {
        return this.requestDocument(relativePath, 'textDocument/semanticTokens/full', {});
    }

    async typeDefinition(relativePath, position) {
        return this.requestDocument(relativePath, 'textDocument/typeDefinition', { position });
    }

    async implementation(relativePath, position) {
        return this.requestDocument(relativePath, 'textDocument/implementation', { position });
    }

    async prepareRename(relativePath, position) {
        if (!this.capabilities?.renameProvider?.prepareProvider) return { defaultBehavior: true };
        return this.requestDocument(relativePath, 'textDocument/prepareRename', { position });
    }

    async rangeFormatting(relativePath, _position, extra) {
        return this.requestDocument(relativePath, 'textDocument/rangeFormatting', { range: extra?.range, options: { tabSize: Number(extra?.tabSize) || 4, insertSpaces: extra?.insertSpaces !== false } });
    }

    async foldingRange(relativePath) {
        return this.requestDocument(relativePath, 'textDocument/foldingRange', {});
    }

    async linkedEditingRange(relativePath, position) {
        return this.requestDocument(relativePath, 'textDocument/linkedEditingRange', { position });
    }

    async onTypeFormatting(relativePath, position, extra) {
        return this.requestDocument(relativePath, 'textDocument/onTypeFormatting', { position, ch: String(extra?.ch ?? ''), options: { tabSize: Number(extra?.tabSize) || 4, insertSpaces: extra?.insertSpaces !== false } });
    }

    async codeLens(relativePath) {
        return this.requestDocument(relativePath, 'textDocument/codeLens', {});
    }

    async codeLensResolve(_relativePath, _position, extra) {
        await this.start();
        return this.request('codeLens/resolve', extra?.lens ?? {});
    }

    async executeCommand(_relativePath, _position, extra) {
        await this.start();
        return this.request('workspace/executeCommand', { command: String(extra?.command ?? ''), arguments: Array.isArray(extra?.arguments) ? extra.arguments : [] });
    }

    async prepareCallHierarchy(relativePath, position) {
        return this.requestDocument(relativePath, 'textDocument/prepareCallHierarchy', { position });
    }

    async incomingCalls(_relativePath, _position, extra) {
        await this.start();
        return this.request('callHierarchy/incomingCalls', { item: extra?.item });
    }

    async outgoingCalls(_relativePath, _position, extra) {
        await this.start();
        return this.request('callHierarchy/outgoingCalls', { item: extra?.item });
    }

    async serverCapabilities() {
        await this.start();
        return this.capabilities ?? {};
    }

    async saveDocument(relativePath) {
        await this.start();
        const resolved = resolveWorkspacePath(this.root, relativePath, { allowMissing: isNotebookShadowPath(relativePath) });
        const document = this.documents.get(resolved.relativePath);
        if (!document) return false;
        this.notify('textDocument/didSave', { textDocument: { uri: document.uri } });
        return true;
    }

    filesChanged(changes) {
        if (!this.capabilities || changes.length === 0) return;
        this.notify('workspace/didChangeWatchedFiles', { changes });
    }

    async rename(relativePath, position, extra) {
        return this.requestDocument(relativePath, 'textDocument/rename', { position, newName: String(extra?.newName ?? '') });
    }

    async codeAction(relativePath, _position, extra) {
        return this.requestDocument(relativePath, 'textDocument/codeAction', { range: extra?.range, context: { diagnostics: Array.isArray(extra?.diagnostics) ? extra.diagnostics : [] } });
    }

    async formatting(relativePath, _position, extra) {
        return this.requestDocument(relativePath, 'textDocument/formatting', { options: { tabSize: Number(extra?.tabSize) || 4, insertSpaces: extra?.insertSpaces !== false } });
    }

    async workspaceSymbol(_relativePath, _position, extra) {
        await this.start();
        return this.request('workspace/symbol', { query: String(extra?.query ?? '') });
    }

    async documentSymbol(relativePath) {
        return this.requestDocument(relativePath, 'textDocument/documentSymbol', {});
    }

    closeDocument(relativePath) {
        const resolved = resolveWorkspacePath(this.root, relativePath, { allowMissing: isNotebookShadowPath(relativePath) });
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

module.exports = { LSP_METHOD_NOT_FOUND, LspMessageReader, LspSession, encodeLspMessage, incrementalChange, normalizeDiagnostic, serverSupports };
