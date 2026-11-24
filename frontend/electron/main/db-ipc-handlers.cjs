function finiteIntegerOrNull(value) {
    const number = Number(value);
    return Number.isFinite(number) ? Math.trunc(number) : null;
}

function optionalString(value) {
    return typeof value === 'string' && value.length > 0 ? value : null;
}

function transcriptFilePathFromFilename(filename) {
    if (typeof filename !== 'string' || filename.length === 0) return '';
    return filename.startsWith('transcripts/') ? filename : `transcripts/${filename}`;
}

function normalizeTranscriptIndexForDaemon(entry) {
    const source = entry && typeof entry === 'object' ? entry : {};
    const capturedAt = finiteIntegerOrNull(source.captured_at ?? source.capturedAt);
    if (!optionalString(source.id) || !optionalString(source.filename) || capturedAt === null) {
        return null;
    }
    return {
        id: source.id,
        pane_id: optionalString(source.pane_id ?? source.paneId),
        workspace_id: optionalString(source.workspace_id ?? source.workspaceId),
        surface_id: optionalString(source.surface_id ?? source.surfaceId),
        filename: source.filename,
        reason: optionalString(source.reason),
        captured_at: capturedAt,
        size_bytes: finiteIntegerOrNull(source.size_bytes ?? source.sizeBytes),
        preview: typeof source.preview === 'string' ? source.preview : null,
    };
}

function normalizeTranscriptIndexForRenderer(entry) {
    const source = entry && typeof entry === 'object' ? entry : {};
    const id = optionalString(source.id);
    const filename = optionalString(source.filename);
    const capturedAt = finiteIntegerOrNull(source.captured_at ?? source.capturedAt);
    if (!id || !filename || capturedAt === null) return null;
    return {
        id,
        filename,
        filePath: optionalString(source.filePath) ?? transcriptFilePathFromFilename(filename),
        reason: optionalString(source.reason) ?? 'manual',
        workspaceId: optionalString(source.workspace_id ?? source.workspaceId),
        surfaceId: optionalString(source.surface_id ?? source.surfaceId),
        paneId: optionalString(source.pane_id ?? source.paneId),
        cwd: optionalString(source.cwd),
        capturedAt,
        sizeBytes: finiteIntegerOrNull(source.size_bytes ?? source.sizeBytes) ?? 0,
        preview: typeof source.preview === 'string' ? source.preview : '',
        content: typeof source.content === 'string' ? source.content : '',
    };
}

function registerDbIpcHandlers(ipcMain, runtime) {
    const { sendDbAckCommand, sendDbQuery, saveExportedFile } = runtime;

    ipcMain.handle('db-append-command-log', async (_event, entry) => {
        try {
            await sendDbAckCommand({ type: 'append-command-log', entry_json: JSON.stringify(entry ?? {}) });
            return true;
        } catch {
            return false;
        }
    });
    ipcMain.handle('db-complete-command-log', async (_event, id, exitCode, durationMs) => {
        try {
            await sendDbAckCommand({
                type: 'complete-command-log',
                id,
                exit_code: Number.isFinite(exitCode) ? Math.trunc(exitCode) : null,
                duration_ms: Number.isFinite(durationMs) ? Math.trunc(durationMs) : null,
            });
            return true;
        } catch {
            return false;
        }
    });
    ipcMain.handle('db-query-command-log', async (_event, opts = {}) => {
        try {
            return await sendDbQuery({
                type: 'query-command-log',
                workspace_id: typeof opts.workspaceId === 'string' ? opts.workspaceId : null,
                pane_id: typeof opts.paneId === 'string' ? opts.paneId : null,
                limit: Number.isFinite(opts.limit) ? Math.max(1, Math.trunc(opts.limit)) : null,
            }, 'command-log-entries');
        } catch {
            return [];
        }
    });
    ipcMain.handle('db-clear-command-log', async () => {
        try {
            await sendDbAckCommand({ type: 'clear-command-log' });
            return true;
        } catch {
            return false;
        }
    });
    ipcMain.handle('db-create-thread', async (_event, thread) => {
        try {
            await sendDbAckCommand({ type: 'create-agent-thread', thread_json: JSON.stringify(thread ?? {}) });
            return true;
        } catch {
            return false;
        }
    });
    ipcMain.handle('db-delete-thread', async (_event, threadId) => {
        try {
            await sendDbAckCommand({ type: 'delete-agent-thread', thread_id: threadId });
            return true;
        } catch {
            return false;
        }
    });
    ipcMain.handle('db-list-threads', async () => {
        try {
            return await sendDbQuery({ type: 'list-agent-threads' }, 'agent-thread-list');
        } catch {
            return [];
        }
    });
    ipcMain.handle('db-get-thread', async (_event, threadId) => {
        try {
            return await sendDbQuery({ type: 'get-agent-thread', thread_id: threadId }, 'agent-thread-detail');
        } catch {
            return { thread: null, messages: [] };
        }
    });
    ipcMain.handle('db-add-message', async (_event, message) => {
        try {
            await sendDbAckCommand({ type: 'add-agent-message', message_json: JSON.stringify(message ?? {}) });
            return true;
        } catch {
            return false;
        }
    });
    ipcMain.handle('db-export-thread', async (event, threadId, messageId, suggestedName) => {
        try {
            const result = await sendDbQuery({ type: 'export-agent-thread', thread_id: threadId, message_id: messageId }, 'agent-thread-exported');
            const exported = result?.file_path ?? null;
            if (!exported || !saveExportedFile) return { ok: true, file_path: exported };
            const saved = await saveExportedFile(event, exported, suggestedName);
            return saved ? { ok: true, file_path: saved } : { ok: true, file_path: exported, canceled: true };
        } catch (error) {
            return { ok: false, error: error?.message || String(error) };
        }
    });
    ipcMain.handle('db-fork-thread', async (_event, threadId, messageId) => {
        try {
            const result = await sendDbQuery({ type: 'fork-agent-thread', thread_id: threadId, message_id: messageId }, 'agent-thread-forked');
            return { ok: true, thread_id: result?.thread_id ?? null, title: result?.title ?? null };
        } catch (error) {
            return { ok: false, error: error?.message || String(error) };
        }
    });
    ipcMain.handle('db-delete-message', async (_event, threadId, messageId) => {
        try {
            await sendDbAckCommand({ type: 'delete-agent-messages', thread_id: threadId, message_ids: [messageId] });
            return true;
        } catch {
            return false;
        }
    });
    ipcMain.handle('db-rewind-messages', async (_event, threadId, messageIds) => {
        try {
            await sendDbAckCommand({ type: 'delete-agent-messages', thread_id: threadId, message_ids: messageIds });
            return true;
        } catch {
            return false;
        }
    });
    ipcMain.handle('db-restore-messages', async (_event, threadId, messageIds) => {
        try {
            await sendDbAckCommand({ type: 'restore-agent-messages', thread_id: threadId, message_ids: messageIds });
            return true;
        } catch {
            return false;
        }
    });
    ipcMain.handle('db-list-messages', async (_event, threadId, limit) => {
        try {
            const result = await sendDbQuery({
                type: 'list-agent-messages',
                thread_id: threadId,
                limit: Number.isFinite(limit) ? Math.max(1, Math.trunc(limit)) : null,
            }, 'agent-thread-detail');
            return Array.isArray(result?.messages) ? result.messages : [];
        } catch {
            return [];
        }
    });
    ipcMain.handle('db-upsert-transcript-index', async (_event, entry) => {
        try {
            const normalized = normalizeTranscriptIndexForDaemon(entry);
            if (!normalized) return false;
            await sendDbAckCommand({ type: 'upsert-transcript-index', entry_json: JSON.stringify(normalized) });
            return true;
        } catch {
            return false;
        }
    });
    ipcMain.handle('db-list-transcript-index', async (_event, workspaceId) => {
        try {
            const entries = await sendDbQuery({ type: 'list-transcript-index', workspace_id: typeof workspaceId === 'string' ? workspaceId : null }, 'transcript-index-entries');
            return Array.isArray(entries)
                ? entries.map(normalizeTranscriptIndexForRenderer).filter(Boolean)
                : [];
        } catch {
            return [];
        }
    });
    ipcMain.handle('db-upsert-snapshot-index', async (_event, entry) => {
        try {
            await sendDbAckCommand({ type: 'upsert-snapshot-index', entry_json: JSON.stringify(entry ?? {}) });
            return true;
        } catch {
            return false;
        }
    });
    ipcMain.handle('db-list-snapshot-index', async (_event, workspaceId) => {
        try {
            return await sendDbQuery({ type: 'list-snapshot-index', workspace_id: typeof workspaceId === 'string' ? workspaceId : null }, 'snapshot-index-entries');
        } catch {
            return [];
        }
    });
    ipcMain.handle('db-upsert-agent-event', async (_event, eventRow) => {
        try {
            await sendDbAckCommand({ type: 'upsert-agent-event', event_json: JSON.stringify(eventRow ?? {}) });
            return true;
        } catch {
            return false;
        }
    });
    ipcMain.handle('db-list-agent-events', async (_event, opts = {}) => {
        try {
            return await sendDbQuery({
                type: 'list-agent-events',
                category: typeof opts.category === 'string' ? opts.category : null,
                pane_id: typeof opts.paneId === 'string' ? opts.paneId : null,
                limit: Number.isFinite(opts.limit) ? Math.max(1, Math.trunc(opts.limit)) : null,
            }, 'agent-event-rows');
        } catch {
            return [];
        }
    });
    ipcMain.handle('db-list-database-tables', async () => {
        try {
            return await sendDbQuery({ type: 'list-database-tables' }, 'database-tables', 30000);
        } catch (error) {
            return { error: error instanceof Error ? error.message : String(error) };
        }
    });
    ipcMain.handle('db-query-database-rows', async (_event, opts = {}) => {
        try {
            return await sendDbQuery({
                type: 'query-database-rows',
                table_name: typeof opts.tableName === 'string' ? opts.tableName : '',
                offset: Number.isFinite(opts.offset) ? Math.max(0, Math.trunc(opts.offset)) : 0,
                limit: Number.isFinite(opts.limit) ? Math.max(1, Math.trunc(opts.limit)) : 100,
                sort_column: typeof opts.sortColumn === 'string' ? opts.sortColumn : null,
                sort_direction: opts.sortDirection === 'asc' || opts.sortDirection === 'desc' ? opts.sortDirection : null,
            }, 'database-rows');
        } catch {
            return null;
        }
    });
    ipcMain.handle('db-update-database-rows', async (_event, tableName, updates) => {
        try {
            return await sendDbQuery({
                type: 'update-database-rows',
                table_name: typeof tableName === 'string' ? tableName : '',
                updates_json: JSON.stringify(Array.isArray(updates) ? updates : []),
            }, 'database-update-ack');
        } catch (error) {
            return { updatedRows: 0, error: error?.message || String(error) };
        }
    });
    ipcMain.handle('db-execute-database-sql', async (_event, sql) => {
        try {
            return await sendDbQuery({
                type: 'execute-database-sql',
                sql: typeof sql === 'string' ? sql : '',
            }, 'database-sql-result', 30000);
        } catch (error) {
            return { error: error?.message || String(error) };
        }
    });
    ipcMain.handle('db-queue-semantic-backfill', async (_event, limit = null) => {
        try {
            return await sendDbQuery({
                type: 'queue-semantic-backfill',
                limit: Number.isFinite(limit) ? Math.max(1, Math.trunc(limit)) : null,
            }, 'semantic-backfill-queued', 30000);
        } catch (error) {
            return { messages_queued: 0, tasks_queued: 0, error: error?.message || String(error) };
        }
    });
    ipcMain.handle('db-get-semantic-index-status', async (_event, opts = {}) => {
        try {
            return await sendDbQuery({
                type: 'get-semantic-index-status',
                embedding_model: typeof opts.embeddingModel === 'string' ? opts.embeddingModel : '',
                dimensions: Number.isFinite(opts.dimensions) ? Math.max(1, Math.trunc(opts.dimensions)) : 1536,
            }, 'semantic-index-status');
        } catch (error) {
            return { error: error?.message || String(error) };
        }
    });
    ipcMain.handle('db-get-backend', async () => {
        try {
            return await sendDbQuery({ type: 'database-get-backend' }, 'database-backend-state');
        } catch (error) {
            return { error: error?.message || String(error) };
        }
    });
    ipcMain.handle('db-sync-now', async () => {
        try {
            return await sendDbQuery({ type: 'database-sync-now' }, 'database-sync-result', 30000);
        } catch (error) {
            return { ok: false, message: error?.message || String(error) };
        }
    });
}

module.exports = { registerDbIpcHandlers };
