const { spawn } = require('child_process');
const net = require('net');
const path = require('path');
const { LspMessageReader, encodeLspMessage } = require('./lsp-session.cjs');
const { adapterForType, adapterStatuses, cargoTestBinary, defaultConfigurations, readLaunchConfigurations, substitute } = require('./dap-adapters.cjs');

const REQUEST_TIMEOUT_MS = 30000;
const LISTEN_PATTERN = /listening (?:at|on)[^\d[]*\[?([0-9a-f.:]+?)\]?:(\d+)/i;

class DapSession {
    constructor(id, adapter, emit) {
        this.id = id;
        this.adapter = adapter;
        this.emit = emit;
        this.seq = 1;
        this.pending = new Map();
        this.reader = new LspMessageReader((message) => this.handle(message));
        this.process = null;
        this.socket = null;
        this.capabilities = {};
        this.initialized = null;
        this.closed = false;
    }

    async connect() {
        const child = spawn(this.adapter.command, this.adapter.args, { windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'] });
        this.process = child;
        child.once('exit', (code) => this.close(`Debug adapter exited (${code ?? 'signal'})`));
        child.stderr.on('data', (chunk) => this.emit('output', { category: 'stderr', output: chunk.toString('utf8') }));
        if (!this.adapter.listen && !this.adapter.tcp) {
            child.stdout.on('data', (chunk) => this.push(chunk));
            this.write = (buffer) => child.stdin.write(buffer);
            return;
        }
        const address = await new Promise((resolve, reject) => {
            let text = '';
            const timer = setTimeout(() => reject(new Error('The debug adapter did not report a port.')), 10000);
            child.stdout.on('data', (chunk) => {
                text += chunk.toString('utf8');
                const match = LISTEN_PATTERN.exec(text);
                if (match) {
                    clearTimeout(timer);
                    resolve({ host: match[1] === '::' || match[1] === '::1' ? '127.0.0.1' : match[1], port: Number(match[2]) });
                }
            });
        });
        this.socket = await new Promise((resolve, reject) => {
            const socket = net.connect(address.port, address.host, () => resolve(socket));
            socket.once('error', reject);
        });
        this.socket.on('data', (chunk) => this.push(chunk));
        this.socket.once('close', () => this.close('Debug adapter disconnected'));
        this.write = (buffer) => this.socket.write(buffer);
    }

    push(chunk) {
        try { this.reader.push(chunk); } catch (error) { this.close(error.message); }
    }

    send(message) {
        if (this.closed) throw new Error('The debug session has ended.');
        this.write(encodeLspMessage(message));
    }

    request(command, args) {
        const seq = this.seq++;
        return new Promise((resolve, reject) => {
            const timer = setTimeout(() => {
                this.pending.delete(seq);
                reject(new Error(`${command} timed out`));
            }, REQUEST_TIMEOUT_MS);
            this.pending.set(seq, { resolve, reject, timer });
            this.send({ seq, type: 'request', command, arguments: args ?? {} });
        });
    }

    handle(message) {
        if (message?.type === 'response') {
            const pending = this.pending.get(message.request_seq);
            if (!pending) return;
            this.pending.delete(message.request_seq);
            clearTimeout(pending.timer);
            if (message.success) pending.resolve(message.body ?? {});
            else pending.reject(new Error(message.message || message.body?.error?.format || `${message.command} failed`));
            return;
        }
        if (message?.type === 'event') {
            if (message.event === 'initialized') this.initializedResolve?.();
            this.emit(message.event, message.body ?? {});
            return;
        }
        if (message?.type === 'request') {
            const reply = (success, body, errorMessage) => this.send({ seq: this.seq++, type: 'response', request_seq: message.seq, command: message.command, success, body, message: errorMessage });
            if (message.command === 'runInTerminal') {
                const args = message.arguments ?? {};
                const [command, ...rest] = args.args ?? [];
                if (!command) return reply(false, undefined, 'No command to run.');
                const child = spawn(command, rest, { cwd: args.cwd || undefined, env: { ...process.env, ...(args.env ?? {}) }, windowsHide: true });
                child.stdout.on('data', (chunk) => this.emit('output', { category: 'stdout', output: chunk.toString('utf8') }));
                child.stderr.on('data', (chunk) => this.emit('output', { category: 'stderr', output: chunk.toString('utf8') }));
                return reply(true, { processId: child.pid });
            }
            if (message.command === 'startDebugging') return reply(false, undefined, 'Child debug sessions are not supported yet.');
            return reply(false, undefined, `Unsupported reverse request: ${message.command}`);
        }
    }

    async start(config, breakpoints, exceptionFilters) {
        await this.connect();
        this.initialized = new Promise((resolve) => { this.initializedResolve = resolve; });
        this.capabilities = await this.request('initialize', {
            clientID: 'zorai', clientName: 'Zorai', adapterID: config.type, pathFormat: 'path', linesStartAt1: true, columnsStartAt1: true,
            supportsVariableType: true, supportsVariablePaging: false, supportsRunInTerminalRequest: true, locale: 'en',
        });
        this.emit('capabilities', this.capabilities);
        const launched = this.request(config.request === 'attach' ? 'attach' : 'launch', config);
        await Promise.race([this.initialized, launched]);
        await this.initialized;
        for (const [file, lines] of Object.entries(breakpoints ?? {})) await this.setBreakpoints(file, lines).catch(() => null);
        if (this.capabilities.exceptionBreakpointFilters) {
            const filters = Array.isArray(exceptionFilters) ? exceptionFilters : this.capabilities.exceptionBreakpointFilters.filter((entry) => entry.default).map((entry) => entry.filter);
            await this.request('setExceptionBreakpoints', { filters }).catch(() => null);
        }
        if (this.capabilities.supportsConfigurationDoneRequest) await this.request('configurationDone', {});
        await launched;
    }

    async setBreakpoints(file, breakpoints) {
        const body = await this.request('setBreakpoints', {
            source: { path: file, name: path.basename(file) },
            breakpoints: breakpoints.map((breakpoint) => ({
                line: breakpoint.line,
                ...(breakpoint.condition ? { condition: breakpoint.condition } : {}),
                ...(breakpoint.hitCondition ? { hitCondition: breakpoint.hitCondition } : {}),
                ...(breakpoint.logMessage ? { logMessage: breakpoint.logMessage } : {}),
            })),
            lines: breakpoints.map((breakpoint) => breakpoint.line),
        });
        this.emit('breakpointsSet', { file, breakpoints: body.breakpoints ?? [] });
        return body;
    }

    close(reason) {
        if (this.closed) return;
        this.closed = true;
        for (const pending of this.pending.values()) {
            clearTimeout(pending.timer);
            pending.reject(new Error(reason || 'The debug session has ended.'));
        }
        this.pending.clear();
        this.emit('sessionEnded', { reason: reason ?? null });
        try { this.socket?.destroy(); } catch {}
        setTimeout(() => { if (this.process && this.process.exitCode === null) this.process.kill(); }, 1000);
    }

    async stop() {
        if (this.closed) return;
        await Promise.race([this.request('disconnect', { terminateDebuggee: true }).catch(() => null), new Promise((resolve) => setTimeout(resolve, 1500))]);
        this.close('Stopped');
    }
}

function createDapRuntime() {
    const sessions = new Map();
    let nextId = 1;
    return {
        configurations(rootPath, file) {
            const configured = readLaunchConfigurations(rootPath);
            return { configurations: configured.length > 0 ? configured : defaultConfigurations(file), adapters: adapterStatuses() };
        },
        async start(webContents, rootPath, file, requested, breakpoints, exceptionFilters) {
            let config = requested;
            if (config?.type === 'rust-test') {
                const { program, cwd } = await cargoTestBinary(rootPath, config.testFile ?? file);
                const native = adapterForType('lldb').resolve() ? 'lldb' : 'gdb';
                config = { name: config.name, type: native, request: 'launch', program, cwd, args: Array.isArray(config.args) ? config.args : [] };
            }
            const adapterSpec = adapterForType(config?.type);
            if (!adapterSpec) throw new Error(`No debug adapter is known for "${config?.type}".`);
            const adapter = adapterSpec.resolve();
            if (!adapter) throw new Error(`${adapterSpec.label} is not installed. ${adapterSpec.hint}`);
            const resolved = substitute({ cwd: rootPath, ...config }, { root: rootPath, file });
            const id = `debug-${nextId++}`;
            const session = new DapSession(id, adapter, (event, body) => {
                if (!webContents.isDestroyed()) webContents.send('debug-event', { sessionId: id, event, body });
                if (event === 'sessionEnded') sessions.delete(id);
                if (event === 'terminated') void session.stop();
            });
            sessions.set(id, session);
            const absolute = Object.fromEntries(Object.entries(breakpoints ?? {}).map(([relative, entries]) => [path.join(rootPath, relative), entries]));
            session.start(resolved, absolute, exceptionFilters).catch((error) => {
                session.emit('output', { category: 'stderr', output: `${error.message}\n` });
                session.close(error.message);
            });
            return { sessionId: id };
        },
        async request(sessionId, command, args) {
            const session = sessions.get(sessionId);
            if (!session) throw new Error('That debug session has ended.');
            return session.request(command, args);
        },
        async setBreakpoints(sessionId, absolutePath, breakpoints) {
            const session = sessions.get(sessionId);
            return session ? session.setBreakpoints(absolutePath, breakpoints) : null;
        },
        async stop(sessionId) {
            const session = sessions.get(sessionId);
            sessions.delete(sessionId);
            if (session) await session.stop();
            return true;
        },
        async stopAll() {
            await Promise.all([...sessions.values()].map((session) => session.stop()));
            sessions.clear();
        },
    };
}

module.exports = { DapSession, LISTEN_PATTERN, createDapRuntime };
