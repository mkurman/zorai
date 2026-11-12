const fs = require('fs');

const PLUGIN_CLI_TIMEOUT_MS = 10 * 60 * 1000;

function pluginSourceSpec(installSource, exists = fs.existsSync) {
    const source = String(installSource ?? '').trim();
    if (source.startsWith('npm:')) {
        const spec = source.slice(4).trim();
        return spec ? { spec } : { reason: 'The npm package name is missing.' };
    }
    if (source.startsWith('github:')) return { spec: source };
    if (source.startsWith('local:')) {
        const folder = source.slice(6).trim();
        if (!folder) return { reason: 'The source folder is missing.' };
        if (!exists(folder)) return { reason: `It was installed from ${folder}, which no longer exists.` };
        return { spec: folder };
    }
    return { reason: 'zorai found this plugin in its plugins folder but has no record of where it was installed from.' };
}

function runPluginCli(execFileImpl, cliPath, args) {
    return new Promise((resolve) => {
        if (!cliPath) {
            resolve({ ok: false, output: 'The zorai CLI was not found, so plugins cannot be managed from here.' });
            return;
        }
        execFileImpl(cliPath, args, { timeout: PLUGIN_CLI_TIMEOUT_MS, maxBuffer: 4 * 1024 * 1024, windowsHide: true }, (error, stdout, stderr) => {
            const output = `${stdout ?? ''}${stderr ?? ''}`.trim();
            resolve({ ok: !error, output: output || (error ? String(error.message ?? error) : '') });
        });
    });
}

function registerPluginCliIpcHandlers(ipcMain, { execFileImpl, getCliPath, dialog, logToFile }) {
    const run = async (args) => {
        const result = await runPluginCli(execFileImpl, getCliPath(), args);
        logToFile?.(result.ok ? 'info' : 'warn', 'plugin cli finished', { args, output: result.output });
        return result;
    };
    ipcMain.handle('plugin-cli-install', (_event, spec) => {
        const value = String(spec ?? '').trim();
        if (!value) return { ok: false, output: 'Enter an npm package, a GitHub address, or a folder.' };
        return run(['plugin', 'add', value]);
    });
    ipcMain.handle('plugin-cli-update', (_event, installSource) => {
        const resolved = pluginSourceSpec(installSource);
        if (!resolved.spec) return { ok: false, output: resolved.reason };
        return run(['plugin', 'add', resolved.spec]);
    });
    ipcMain.handle('plugin-cli-remove', (_event, name) => {
        const value = String(name ?? '').trim();
        if (!value) return { ok: false, output: 'No plugin name given.' };
        return run(['plugin', 'remove', value]);
    });
    ipcMain.handle('plugin-update-source', (_event, installSource) => pluginSourceSpec(installSource));
    ipcMain.handle('plugin-pick-folder', async () => {
        const selection = await dialog.showOpenDialog({ title: 'Plugin Folder', properties: ['openDirectory'] });
        return selection.canceled || !selection.filePaths?.[0] ? null : selection.filePaths[0];
    });
}

module.exports = { pluginSourceSpec, registerPluginCliIpcHandlers, runPluginCli };
