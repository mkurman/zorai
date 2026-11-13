const { execFile, execFileSync, spawn } = require('child_process');
const fs = require('fs');
const path = require('path');

const NPM_LATEST_URL = 'https://registry.npmjs.org/zor-ai/latest';
const INSTALL_SCRIPT_URL = 'https://raw.githubusercontent.com/mkurman/zorai/main/scripts/install.sh';
const SWAP_APP_SCRIPT = [
    'pid="$1"; bundle="$2"; staged="$3"; staging="$4"; reopen="$5"',
    'while kill -0 "$pid" 2>/dev/null; do sleep 0.2; done',
    'backup="$bundle.previous"',
    'rm -rf "$backup"',
    'if mv "$bundle" "$backup"; then',
    '  if mv "$staged" "$bundle"; then rm -rf "$backup"; else mv "$backup" "$bundle"; fi',
    'fi',
    'rm -rf "$staging"',
    'if [ "$reopen" = "1" ]; then open "$bundle"; fi',
].join('\n');

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

function macAppBundle(platform, exePath) {
    if (platform !== 'darwin') return null;
    const match = /^(.+\.app)\/Contents\/MacOS\/[^/]+$/.exec(exePath);
    return match ? match[1] : null;
}

function canWriteDefault(target) {
    try {
        fs.accessSync(target, fs.constants.W_OK);
        return true;
    } catch {
        return false;
    }
}

function runCli(execFileImpl, cliPath, args, extraEnv = null) {
    return new Promise((resolve) => {
        const options = { timeout: 900000, maxBuffer: 4 * 1024 * 1024, windowsHide: true };
        if (extraEnv) options.env = { ...process.env, ...extraEnv };
        execFileImpl(cliPath, args, options, (error, stdout, stderr) => {
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
    spawnImpl = spawn,
    removeDir = (target) => fs.rmSync(target, { recursive: true, force: true }),
    canWrite = canWriteDefault,
    pid = process.pid,
}) {
    let installing = null;
    let installedUpdate = false;
    let stagedApp = null;
    const cliPath = () => resolveUpgradeCliPath({ env, platform, exePath: app.getPath('exe'), getCliPath, exists });
    const selfContainedBundle = () => {
        const bundle = macAppBundle(platform, app.getPath('exe'));
        if (!bundle) return null;
        const resolved = cliPath();
        return !resolved || resolved.startsWith(`${bundle}/`) ? bundle : null;
    };
    const stagingDir = () => path.join(app.getPath('userData'), 'pending-update');
    const swapStagedApp = (reopen) => {
        const bundle = selfContainedBundle();
        if (!stagedApp || !bundle) return false;
        const child = spawnImpl('/bin/sh', ['-c', SWAP_APP_SCRIPT, 'zorai-update', String(pid), bundle, stagedApp, stagingDir(), reopen ? '1' : '0'], { detached: true, stdio: 'ignore' });
        child.unref?.();
        logToFile?.('info', 'replacing app bundle after exit', { bundle, stagedApp, reopen });
        stagedApp = null;
        return true;
    };
    const installBundle = (bundle) => {
        if (!canWrite(bundle) || !canWrite(path.dirname(bundle))) {
            return Promise.resolve({ ok: false, output: `zorai can't replace ${bundle} because this account can't write there. Download the new version from the releases page and drag it to Applications.` });
        }
        const staging = stagingDir();
        removeDir(staging);
        return runCli(execFileImpl, '/bin/sh', ['-c', `curl -fsSL ${INSTALL_SCRIPT_URL} | sh`], { ZORAI_INSTALL_DIR: staging }).then((result) => {
            const staged = path.join(staging, 'zorai-desktop.app');
            if (result.ok && !exists(path.join(staged, 'Contents', 'MacOS', 'zorai'))) {
                return { ok: false, output: `${result.output}\nThe download finished but the new app was not found in ${staging}.`.trim() };
            }
            if (result.ok) stagedApp = staged;
            return result;
        });
    };

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
            const bundle = selfContainedBundle();
            installing = (bundle
                ? installBundle(bundle)
                : resolved
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
            swapStagedApp(false);
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
            if (swapStagedApp(true)) {
                installedUpdate = false;
                app.exit(0);
                return;
            }
            app.relaunch(env.APPIMAGE ? { execPath: env.APPIMAGE, args: process.argv.slice(1) } : undefined);
            app.exit(0);
        },
    };
}

module.exports = { createUpdateRuntime, isNewerVersion, resolveUpgradeCliPath, NPM_LATEST_URL, INSTALL_SCRIPT_URL, SWAP_APP_SCRIPT };
