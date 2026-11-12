const { execFile, execFileSync } = require('child_process');
const fs = require('fs');
const path = require('path');

const NPM_LATEST_URL = 'https://registry.npmjs.org/zor-ai/latest';

function parseVersion(raw) {
    const match = /^v?(\d+)\.(\d+)\.(\d+)/.exec(String(raw ?? '').trim());
    return match ? match.slice(1, 4).map(Number) : null;
}

function isNewerVersion(latest, current) {
    const next = parseVersion(latest);
    const installed = parseVersion(current);
    if (!next || !installed) return false;
    for (let index = 0; index < 3; index += 1) {
        if (next[index] !== installed[index]) return next[index] > installed[index];
    }
    return false;
}

function resolveUpgradeCliPath({ env, platform, exePath, getCliPath, exists }) {
    const exeName = platform === 'win32' ? 'zorai.exe' : 'zorai';
    const candidates = [];
    if (env.APPIMAGE) candidates.push(path.join(path.dirname(env.APPIMAGE), exeName));
    candidates.push(path.join(path.dirname(exePath), exeName));
    if (platform === 'darwin') candidates.push(path.resolve(path.dirname(exePath), '..', '..', '..', exeName));
    candidates.push(getCliPath());
    return candidates.find((candidate) => candidate !== exePath && exists(candidate)) ?? null;
}

function runCli(execFileImpl, cliPath, args) {
    return new Promise((resolve) => {
        execFileImpl(cliPath, args, { timeout: 900000, maxBuffer: 4 * 1024 * 1024, windowsHide: true }, (error, stdout, stderr) => {
            const output = `${stdout ?? ''}${stderr ?? ''}`.trim();
            resolve({ ok: !error, output: output || (error ? String(error.message ?? error) : '') });
        });
    });
}

function createUpdateRuntime({
    app,
    getCliPath,
    logToFile,
    fetchImpl = fetch,
    execFileImpl = execFile,
    execFileSyncImpl = execFileSync,
    exists = fs.existsSync,
    env = process.env,
    platform = process.platform,
}) {
    let installing = null;
    let installedUpdate = false;
    const cliPath = () => resolveUpgradeCliPath({ env, platform, exePath: app.getPath('exe'), getCliPath, exists });

    return {
        async check() {
            const currentVersion = app.getVersion();
            try {
                const response = await fetchImpl(NPM_LATEST_URL, { signal: AbortSignal.timeout(5000) });
                if (!response.ok) throw new Error(`npm registry returned ${response.status}`);
                const latestVersion = String((await response.json())?.version ?? '');
                return { currentVersion, latestVersion, updateAvailable: isNewerVersion(latestVersion, currentVersion) };
            } catch (error) {
                logToFile?.('warn', 'update check failed', { message: error?.message });
                return { currentVersion, latestVersion: null, updateAvailable: false, error: String(error?.message ?? error) };
            }
        },
        install() {
            if (installing) return installing;
            if (platform === 'darwin' && app.getPath('exe').startsWith('/Volumes/')) {
                return Promise.resolve({ ok: false, output: 'zorai is running from the disk image. Drag it to Applications, open it from there, then update.' });
            }
            const resolved = cliPath();
            installing = (resolved
                ? runCli(execFileImpl, resolved, ['upgrade', '--keep-running'])
                : Promise.resolve({ ok: false, output: 'The zorai CLI was not found next to the desktop app, so it cannot be upgraded from here.' }))
                .then((result) => {
                    if (result.ok) installedUpdate = true;
                    logToFile?.(result.ok ? 'info' : 'error', 'update install finished', { cliPath: resolved, output: result.output });
                    return result;
                })
                .finally(() => { installing = null; });
            return installing;
        },
        stopDaemonIfUpdated() {
            if (!installedUpdate) return false;
            installedUpdate = false;
            const resolved = cliPath();
            if (!resolved) return false;
            try {
                execFileSyncImpl(resolved, ['stop'], { timeout: 15000, windowsHide: true, stdio: 'ignore' });
            } catch (error) {
                logToFile?.('error', 'stopping daemon after update failed', { message: error?.message });
            }
            return true;
        },
        async restartDaemon() {
            const resolved = cliPath();
            if (!resolved) return { ok: false, output: 'The zorai CLI was not found, so the daemon cannot be restarted from here.' };
            const stopped = await runCli(execFileImpl, resolved, ['stop']);
            const started = await runCli(execFileImpl, resolved, ['daemon']);
            logToFile?.(started.ok ? 'info' : 'error', 'daemon restart finished', { stop: stopped.output, start: started.output });
            return { ok: started.ok, output: [stopped.output, started.output].filter(Boolean).join('\n') };
        },
        async restart() {
            const resolved = cliPath();
            if (resolved) await runCli(execFileImpl, resolved, ['stop']);
            app.relaunch(env.APPIMAGE ? { execPath: env.APPIMAGE, args: process.argv.slice(1) } : undefined);
            app.exit(0);
        },
    };
}

module.exports = { createUpdateRuntime, isNewerVersion, resolveUpgradeCliPath, NPM_LATEST_URL };
