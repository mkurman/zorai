function createTerminalBridgeRuntime(options) {
    const {
        cloneSessionPrefix,
        fs,
        getCliPath,
        getChildProcessEnv = () => process.env,
        getMainWindow,
        logToFile,
        maxReattachHistoryBytes,
        maxTerminalHistoryBytes,
        path,
        spawn,
        spawnDaemon,
    } = options;

    const terminalBridges = new Map();
    const paneSessionHints = new Map();
    function emitTerminalEvent(paneId, event) {
        if (event.type === 'error' || event.type === 'session-exited' || event.type === 'ready') {
            logToFile('info', 'terminal event', { paneId, event });
        }
        getMainWindow()?.webContents.send('terminal-event', { paneId, ...event });
    }
    function rememberTerminalOutput(bridge, base64Chunk) {
        const size = Buffer.byteLength(base64Chunk, 'base64');
        bridge.outputHistory.push(base64Chunk);
        bridge.outputHistoryBytes += size;

        while (bridge.outputHistoryBytes > maxTerminalHistoryBytes && bridge.outputHistory.length > 1) {
            const removed = bridge.outputHistory.shift();
            if (!removed) break;
            bridge.outputHistoryBytes -= Buffer.byteLength(removed, 'base64');
        }
    }
    function getReplayHistory(bridge, maxBytes = maxReattachHistoryBytes) {
        if (!bridge || !Array.isArray(bridge.outputHistory) || bridge.outputHistory.length === 0) {
            return [];
        }

        const replay = [];
        let totalBytes = 0;
        for (let index = bridge.outputHistory.length - 1; index >= 0; index -= 1) {
            const chunk = bridge.outputHistory[index];
            const chunkBytes = Buffer.byteLength(chunk, 'base64');
            if (replay.length > 0 && totalBytes + chunkBytes > maxBytes) {
                break;
            }
            replay.unshift(chunk);
            totalBytes += chunkBytes;
        }
        return replay;
    }
    function parseCloneSessionToken(value) {
        if (typeof value !== 'string') return null;
        let trimmed = value.trim();
        if (!trimmed.startsWith(cloneSessionPrefix)) return null;
        for (let depth = 0; depth < 4; depth += 1) {
            if (!trimmed.startsWith(cloneSessionPrefix)) break;
            trimmed = trimmed.slice(cloneSessionPrefix.length).trim();
            if (!trimmed) return null;
        }
        return trimmed || null;
    }
    function sendBridgeCommand(bridge, command) {
        if (!bridge || bridge.process.killed || !bridge.process.stdin.writable) return;
        bridge.process.stdin.write(`${JSON.stringify(command)}\n`);
    }
    function getBridgeForPane(paneId) {
        const bridge = terminalBridges.get(paneId);
        if (!bridge) {
            throw new Error(`terminal bridge not found for pane ${paneId}`);
        }
        return bridge;
    }
    function getBridgeForSnapshotAction(paneId) {
        const requested = paneId ? terminalBridges.get(paneId) : null;
        if (requested && !requested.process.killed && requested.process.stdin.writable) {
            return requested;
        }

        for (const bridge of terminalBridges.values()) {
            if (bridge && !bridge.process.killed && bridge.process.stdin.writable) {
                return bridge;
            }
        }

        throw new Error(`terminal bridge not found for pane ${paneId}`);
    }
    function closeBridgeStdin(bridge) {
        if (!bridge || bridge.process.killed || bridge.process.stdin.destroyed) return;
        if (bridge.process.stdin.writableEnded) return;
        bridge.process.stdin.end();
    }
    function forceKillBridgeProcess(bridge) {
        if (!bridge || bridge.process.killed) return;
        try {
            bridge.process.kill();
        } catch (error) {
            logToFile('error', 'failed to kill bridge process', { paneId: bridge.paneId, message: error.message });
        }
    }
    function stopTerminalBridge(paneId, killSession = false, force = false) {
        const bridge = terminalBridges.get(paneId);
        if (!bridge) return false;

        bridge.closing = true;
        if (!bridge.process.killed) {
            if (force) {
                forceKillBridgeProcess(bridge);
            } else {
                sendBridgeCommand(bridge, { type: killSession ? 'kill-session' : 'shutdown' });
                closeBridgeStdin(bridge);
                setTimeout(() => {
                    forceKillBridgeProcess(bridge);
                }, 1000).unref?.();
            }
        }

        if (bridge.process.killed) {
            terminalBridges.delete(paneId);
        }
        if (killSession) {
            paneSessionHints.delete(paneId);
        }
        return true;
    }
    function busyTerminalPaneIds() {
        const paneIds = [];
        for (const [paneId, bridge] of terminalBridges) {
            if (bridge?.commandActive) paneIds.push(paneId);
        }
        return paneIds;
    }
    function stopAllTerminalBridges(killSessions = false, force = false) {
        for (const paneId of [...terminalBridges.keys()]) {
            stopTerminalBridge(paneId, killSessions, force);
        }
    }
    function forwardBridgeEvent(paneId, bridge, event) {
        const sessionId = event.session_id;
        switch (event.type) {
            case 'ready':
                bridge.ready = true;
                bridge.sessionId = sessionId;
                paneSessionHints.set(paneId, sessionId);
                emitTerminalEvent(paneId, { type: 'ready', sessionId });
                return;
            case 'output':
                rememberTerminalOutput(bridge, event.data);
                emitTerminalEvent(paneId, { type: 'output', sessionId, data: event.data });
                return;
            case 'session-exited':
                bridge.commandActive = false;
                emitTerminalEvent(paneId, { type: 'session-exited', sessionId, exitCode: event.exit_code });
                paneSessionHints.delete(paneId);
                terminalBridges.delete(paneId);
                return;
            case 'command-finished':
                bridge.commandActive = false;
                emitTerminalEvent(paneId, { type: 'command-finished', sessionId, exitCode: event.exit_code });
                return;
            case 'command-started':
                bridge.commandActive = true;
                emitTerminalEvent(paneId, { type: 'command-started', sessionId, commandB64: event.command_b64 });
                return;
            case 'cwd-changed':
                emitTerminalEvent(paneId, { type: 'cwd-changed', sessionId, cwd: event.cwd });
                return;
            case 'managed-queued':
                bridge.commandActive = true;
                emitTerminalEvent(paneId, {
                    type: 'managed-queued',
                    sessionId,
                    executionId: event.execution_id,
                    position: event.position,
                    snapshot: event.snapshot ?? null,
                });
                return;
            case 'approval-required':
                bridge.commandActive = true;
                emitTerminalEvent(paneId, { type: 'approval-required', sessionId, approval: event.approval });
                return;
            case 'approval-resolved':
                emitTerminalEvent(paneId, {
                    type: 'approval-resolved',
                    sessionId,
                    approvalId: event.approval_id,
                    decision: event.decision,
                });
                return;
            case 'managed-started':
                bridge.commandActive = true;
                emitTerminalEvent(paneId, {
                    type: 'managed-started',
                    sessionId,
                    executionId: event.execution_id,
                    command: event.command,
                    source: event.source,
                });
                return;
            case 'managed-finished':
                bridge.commandActive = false;
                emitTerminalEvent(paneId, {
                    type: 'managed-finished',
                    sessionId,
                    executionId: event.execution_id,
                    command: event.command,
                    exitCode: event.exit_code,
                    durationMs: event.duration_ms,
                    snapshot: event.snapshot ?? null,
                });
                return;
            case 'managed-rejected':
                bridge.commandActive = false;
                emitTerminalEvent(paneId, {
                    type: 'managed-rejected',
                    sessionId,
                    executionId: event.execution_id,
                    message: event.message,
                });
                return;
            case 'history-search-result':
                emitTerminalEvent(paneId, { type: 'history-search-result', query: event.query, summary: event.summary, hits: event.hits });
                return;
            case 'skill-generated':
                emitTerminalEvent(paneId, { type: 'skill-generated', title: event.title, path: event.path });
                return;
            case 'symbol-search-result':
                emitTerminalEvent(paneId, { type: 'symbol-search-result', symbol: event.symbol, matches: event.matches });
                return;
            case 'snapshot-list':
                emitTerminalEvent(paneId, { type: 'snapshot-list', snapshots: event.snapshots });
                return;
            case 'snapshot-restored':
                emitTerminalEvent(paneId, { type: 'snapshot-restored', snapshotId: event.snapshot_id, ok: event.ok, message: event.message });
                return;
            case 'osc-notification':
                emitTerminalEvent(paneId, { type: 'osc-notification', sessionId, notification: event.notification });
                return;
            case 'error':
                emitTerminalEvent(paneId, { type: 'error', message: event.message });
                return;
            default:
                return;
        }
    }
    async function cloneTerminalSession(_event, payload = {}) {
        const sourcePaneId = typeof payload.sourcePaneId === 'string' ? payload.sourcePaneId.trim() : '';
        const requestedRaw = typeof payload.sourceSessionId === 'string' ? payload.sourceSessionId.trim() : '';
        const requestedSourceSessionId = parseCloneSessionToken(requestedRaw) || requestedRaw;
        let sourceSessionId = requestedSourceSessionId;

        if (sourcePaneId) {
            const bridge = terminalBridges.get(sourcePaneId);
            if (bridge?.sessionId?.trim()) {
                sourceSessionId = bridge.sessionId.trim();
            }
            if (!sourceSessionId) {
                const hinted = paneSessionHints.get(sourcePaneId);
                if (typeof hinted === 'string' && hinted.trim()) {
                    sourceSessionId = hinted.trim();
                }
            }
        }

        logToFile('info', 'clone terminal request', {
            sourcePaneId: sourcePaneId || null,
            requestedSourceSessionId: requestedSourceSessionId || null,
            resolvedSourceSessionId: sourceSessionId || null,
            hasLiveBridge: sourcePaneId ? terminalBridges.has(sourcePaneId) : false,
            hasHint: sourcePaneId ? paneSessionHints.has(sourcePaneId) : false,
        });

        if (!sourceSessionId) {
            throw new Error('sourceSessionId is required (and no live source pane session was found)');
        }
        if (!await spawnDaemon()) {
            throw new Error('daemon is not reachable');
        }

        const cliPath = getCliPath();
        if (!fs.existsSync(cliPath)) {
            throw new Error(`zorai CLI not found at ${cliPath}`);
        }

        const args = ['clone', '--source', sourceSessionId];
        if (typeof payload.workspaceId === 'string' && payload.workspaceId.trim()) args.push('--workspace', payload.workspaceId.trim());
        if (Number.isFinite(payload.cols)) args.push('--cols', String(Math.max(2, Math.trunc(payload.cols))));
        if (Number.isFinite(payload.rows)) args.push('--rows', String(Math.max(2, Math.trunc(payload.rows))));
        if (typeof payload.cwd === 'string' && payload.cwd.trim()) args.push('--cwd', payload.cwd.trim());

        try {
            const result = await new Promise((resolve, reject) => {
                const child = spawn(cliPath, args, {
                    cwd: path.dirname(cliPath),
                    env: getChildProcessEnv(),
                    windowsHide: true,
                    stdio: ['ignore', 'pipe', 'pipe'],
                });

                let stdout = '';
                let stderr = '';
                child.stdout.on('data', (chunk) => { stdout += chunk.toString('utf8'); });
                child.stderr.on('data', (chunk) => { stderr += chunk.toString('utf8'); });
                child.on('error', reject);
                child.on('exit', (code) => {
                    if (code !== 0) {
                        reject(new Error((stderr || stdout || `zorai clone exited with code ${code}`).trim()));
                        return;
                    }
                    const lines = stdout.split(/\r?\n/).map((line) => line.trim()).filter(Boolean);
                    const sessionId = lines[0] ?? '';
                    if (!sessionId) {
                        reject(new Error('zorai clone did not return a session id'));
                        return;
                    }
                    const cmdLine = lines.find((line) => line.startsWith('active_command:'));
                    resolve({ sessionId, activeCommand: cmdLine ? cmdLine.slice('active_command:'.length) : null });
                });
            });

            logToFile('info', 'cloned terminal session', {
                sourceSessionId,
                clonedSessionId: result.sessionId,
                activeCommand: result.activeCommand ?? null,
            });
            return result;
        } catch (error) {
            logToFile('error', 'failed to clone terminal session', {
                sourceSessionId,
                message: error?.message ?? String(error),
            });
            throw error;
        }
    }
    async function startTerminalBridge(_event, options = {}) {
        const paneId = typeof options.paneId === 'string' ? options.paneId : '';
        if (!paneId) throw new Error('paneId is required');

        logToFile('info', 'starting terminal bridge', { paneId, options });
        const existing = terminalBridges.get(paneId);
        if (existing) {
            return {
                sessionId: existing.sessionId,
                initialOutput: getReplayHistory(existing),
                state: existing.ready ? 'reachable' : 'checking',
            };
        }

        if (!await spawnDaemon()) {
            throw new Error('daemon is not reachable');
        }

        const cliPath = getCliPath();
        if (!fs.existsSync(cliPath)) {
            logToFile('error', 'cli binary missing', { cliPath });
            throw new Error(`zorai CLI not found at ${cliPath}`);
        }

        const cols = Number.isFinite(options.cols) ? Math.max(2, Math.trunc(options.cols)) : 80;
        const rows = Number.isFinite(options.rows) ? Math.max(2, Math.trunc(options.rows)) : 24;
        const args = ['bridge', '--cols', String(cols), '--rows', String(rows)];
        let requestedSessionId = typeof options.sessionId === 'string' ? options.sessionId.trim() : '';
        const cloneFromSessionId = parseCloneSessionToken(requestedSessionId);
        if (cloneFromSessionId) {
            const cloned = await cloneTerminalSession(null, {
                sourcePaneId: typeof options.sourcePaneId === 'string' ? options.sourcePaneId : null,
                sourceSessionId: cloneFromSessionId,
                workspaceId: typeof options.workspaceId === 'string' ? options.workspaceId : null,
                cols,
                rows,
            });
            requestedSessionId = typeof cloned?.sessionId === 'string' ? cloned.sessionId.trim() : '';
        }

        if (requestedSessionId) args.push('--session', requestedSessionId);
        if (typeof options.shell === 'string' && options.shell) args.push('--shell', options.shell);
        if (typeof options.cwd === 'string' && options.cwd) args.push('--cwd', options.cwd);
        if (typeof options.workspaceId === 'string' && options.workspaceId) args.push('--workspace', options.workspaceId);

        const bridgeProcess = spawn(cliPath, args, {
            cwd: path.dirname(cliPath),
            env: getChildProcessEnv(),
            windowsHide: true,
            stdio: ['pipe', 'pipe', 'pipe'],
        });

        const bridge = {
            process: bridgeProcess,
            paneId,
            sessionId: requestedSessionId || null,
            ready: false,
            closing: false,
            commandActive: false,
            outputHistory: [],
            outputHistoryBytes: 0,
            stdoutBuffer: '',
            stderrBuffer: '',
        };
        if (typeof bridge.sessionId === 'string' && bridge.sessionId) {
            paneSessionHints.set(paneId, bridge.sessionId);
        }
        terminalBridges.set(paneId, bridge);

        bridgeProcess.stdout.on('data', (chunk) => {
            bridge.stdoutBuffer += chunk.toString('utf8');
            const lines = bridge.stdoutBuffer.split(/\r?\n/);
            bridge.stdoutBuffer = lines.pop() ?? '';
            for (const line of lines) {
                if (!line.trim()) continue;
                try {
                    forwardBridgeEvent(paneId, bridge, JSON.parse(line));
                } catch (error) {
                    emitTerminalEvent(paneId, { type: 'error', message: `invalid bridge output: ${error.message}` });
                }
            }
        });

        bridgeProcess.stderr.on('data', (chunk) => {
            bridge.stderrBuffer += chunk.toString('utf8');
            const message = bridge.stderrBuffer.trim();
            if (!message) return;
            logToFile('error', 'bridge stderr', { paneId, message });
            emitTerminalEvent(paneId, { type: 'error', message });
            bridge.stderrBuffer = '';
        });

        bridgeProcess.on('error', (error) => {
            logToFile('error', 'bridge process error', { paneId, message: error.message });
            emitTerminalEvent(paneId, { type: 'error', message: error.message });
            terminalBridges.delete(paneId);
        });

        bridgeProcess.on('exit', (code, signal) => {
            logToFile('info', 'bridge process exit', { paneId, code, signal, closing: bridge.closing });
            if (!bridge.closing && code !== 0) {
                emitTerminalEvent(paneId, { type: 'error', message: `terminal bridge exited with ${signal ?? code}` });
            }
            if (terminalBridges.get(paneId) === bridge) {
                terminalBridges.delete(paneId);
            }
        });

        return { sessionId: bridge.sessionId, initialOutput: [], state: 'checking' };
    }
    function sendTerminalInput(_event, paneId, data) {
        const bridge = terminalBridges.get(paneId);
        if (!bridge || typeof data !== 'string' || !data) return false;
        sendBridgeCommand(bridge, { type: 'input', data });
        return true;
    }
    function resizeTerminalSession(_event, paneId, cols, rows) {
        const bridge = terminalBridges.get(paneId);
        if (!bridge) return false;
        sendBridgeCommand(bridge, { type: 'resize', cols: Math.max(2, Math.trunc(cols)), rows: Math.max(2, Math.trunc(rows)) });
        return true;
    }
    function executeManagedCommand(_event, paneId, payload = {}) {
        sendBridgeCommand(getBridgeForPane(paneId), {
            type: 'execute-managed',
            command: typeof payload.command === 'string' ? payload.command : '',
            rationale: typeof payload.rationale === 'string' ? payload.rationale : 'Operator requested managed execution',
            allow_network: Boolean(payload.allowNetwork),
            sandbox_enabled: Boolean(payload.sandboxEnabled),
            security_level: typeof payload.securityLevel === 'string' ? payload.securityLevel : 'moderate',
            cwd: typeof payload.cwd === 'string' ? payload.cwd : null,
            language_hint: typeof payload.languageHint === 'string' ? payload.languageHint : null,
            source: typeof payload.source === 'string' ? payload.source : 'agent',
        });
        return true;
    }
    function resolveManagedApproval(_event, paneId, approvalId, decision) {
        const bridge = terminalBridges.get(paneId);
        if (!bridge || bridge.process.killed || !bridge.process.stdin.writable) {
            return false;
        }
        sendBridgeCommand(bridge, { type: 'approval-decision', approval_id: approvalId, decision });
        return true;
    }
    function searchManagedHistory(_event, paneId, query, limit) {
        sendBridgeCommand(getBridgeForPane(paneId), { type: 'search-history', query, limit });
        return true;
    }
    function generateManagedSkill(_event, paneId, query, title) {
        sendBridgeCommand(getBridgeForPane(paneId), { type: 'generate-skill', query, title });
        return true;
    }
    function findManagedSymbol(_event, paneId, workspaceRoot, symbol, limit) {
        sendBridgeCommand(getBridgeForPane(paneId), { type: 'find-symbol', workspace_root: workspaceRoot, symbol, limit });
        return true;
    }
    function listSnapshots(_event, paneId, workspaceId) {
        sendBridgeCommand(getBridgeForSnapshotAction(paneId), { type: 'list-snapshots', workspace_id: workspaceId ?? null });
        return true;
    }
    function restoreSnapshot(_event, paneId, snapshotId) {
        sendBridgeCommand(getBridgeForSnapshotAction(paneId), { type: 'restore-snapshot', snapshot_id: snapshotId });
        return true;
    }
    return {
        busyTerminalPaneIds,
        cloneTerminalSession,
        executeManagedCommand,
        findManagedSymbol,
        generateManagedSkill,
        listSnapshots,
        resolveManagedApproval,
        resizeTerminalSession,
        restoreSnapshot,
        searchManagedHistory,
        sendTerminalInput,
        startTerminalBridge,
        stopAllTerminalBridges,
        stopTerminalBridge,
    };
}
module.exports = { createTerminalBridgeRuntime };
