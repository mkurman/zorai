const test = require('node:test');
const assert = require('node:assert/strict');
const { execFileSync } = require('node:child_process');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { createUpdateRuntime, isNewerVersion, resolveUpgradeCliPath, INSTALL_SCRIPT_URL, SWAP_APP_SCRIPT } = require('./main/update-runtime.cjs');

function fakeApp(version = '1.0.40') {
    const calls = [];
    return {
        calls,
        getVersion: () => version,
        getPath: () => '/opt/zorai/zorai-desktop',
        relaunch: (options) => calls.push(['relaunch', options]),
        exit: (code) => calls.push(['exit', code]),
    };
}

test('only a strictly newer release counts as an update, so equal or older npm tags never prompt', () => {
    assert.equal(isNewerVersion('1.0.41', '1.0.40'), true);
    assert.equal(isNewerVersion('v1.1.0', '1.0.40'), true);
    assert.equal(isNewerVersion('1.0.40', '1.0.40'), false);
    assert.equal(isNewerVersion('1.0.39', '1.0.40'), false);
    assert.equal(isNewerVersion('garbage', '1.0.40'), false);
});

test('an AppImage resolves the CLI next to the real AppImage file, not inside its temporary mount', () => {
    const resolved = resolveUpgradeCliPath({
        env: { APPIMAGE: '/home/me/.local/bin/zorai-desktop' },
        platform: 'linux',
        exePath: '/tmp/.mount_zoraiXYZ/zorai',
        getCliPath: () => '/tmp/.mount_zoraiXYZ/resources/bin/zorai',
        exists: (candidate) => candidate === '/home/me/.local/bin/zorai',
    });
    assert.equal(resolved, '/home/me/.local/bin/zorai');
});

test('a failed registry lookup reports no update instead of throwing at startup', async () => {
    const runtime = createUpdateRuntime({ app: fakeApp(), getCliPath: () => '', fetchImpl: async () => { throw new Error('offline'); } });
    const status = await runtime.check();
    assert.equal(status.updateAvailable, false);
    assert.equal(status.error, 'offline');
});

test('install runs the CLI upgrade without stopping the daemon, so work continues until the user restarts', async () => {
    const invocations = [];
    const runtime = createUpdateRuntime({
        app: fakeApp(),
        getCliPath: () => '/opt/zorai/zorai',
        exists: (candidate) => candidate === '/opt/zorai/zorai',
        env: {},
        platform: 'linux',
        fetchImpl: async () => ({ ok: true, json: async () => ({ version: '1.0.41' }) }),
        execFileImpl: (file, args, _options, callback) => { invocations.push([file, args]); callback(null, 'Upgrade installed.', ''); },
    });
    assert.equal((await runtime.check()).updateAvailable, true);
    assert.deepEqual(await runtime.install(), { ok: true, output: 'Upgrade installed.' });
    assert.deepEqual(invocations, [['/opt/zorai/zorai', ['upgrade', '--keep-running']]]);
});

test('restart stops the old daemon before relaunching so the new binaries are the ones started', async () => {
    const app = fakeApp();
    const invocations = [];
    const runtime = createUpdateRuntime({
        app,
        getCliPath: () => '/opt/zorai/zorai',
        exists: () => true,
        env: {},
        platform: 'linux',
        execFileImpl: (file, args, _options, callback) => { invocations.push(args); callback(null, '', ''); },
    });
    await runtime.restart();
    assert.deepEqual(invocations, [['stop']]);
    assert.deepEqual(app.calls, [['relaunch', undefined], ['exit', 0]]);
});

test('quitting after an installed update stops the old daemon once, so the next launch starts the new one', async () => {
    const stops = [];
    const runtime = createUpdateRuntime({
        app: fakeApp(),
        getCliPath: () => '/opt/zorai/zorai',
        exists: () => true,
        env: {},
        platform: 'linux',
        execFileImpl: (_file, _args, _options, callback) => callback(null, '', ''),
        execFileSyncImpl: (file, args) => stops.push([file, args]),
    });
    assert.equal(runtime.stopDaemonIfUpdated(), false);
    await runtime.install();
    assert.equal(runtime.stopDaemonIfUpdated(), true);
    assert.equal(runtime.stopDaemonIfUpdated(), false);
    assert.deepEqual(stops, [['/opt/zorai/zorai', ['stop']]]);
});

test('a failed install leaves the running daemon alone on quit', async () => {
    const stops = [];
    const runtime = createUpdateRuntime({
        app: fakeApp(),
        getCliPath: () => '/opt/zorai/zorai',
        exists: () => true,
        env: {},
        platform: 'linux',
        execFileImpl: (_file, _args, _options, callback) => callback(new Error('boom'), '', 'installer failed'),
        execFileSyncImpl: (file, args) => stops.push([file, args]),
    });
    await runtime.install();
    assert.equal(runtime.stopDaemonIfUpdated(), false);
    assert.deepEqual(stops, []);
});

test('restarting the daemon stops it and starts it again without relaunching the app', async () => {
    const app = fakeApp();
    const invocations = [];
    const runtime = createUpdateRuntime({
        app,
        getCliPath: () => '/opt/zorai/zorai',
        exists: () => true,
        env: {},
        platform: 'linux',
        execFileImpl: (file, args, _options, callback) => { invocations.push(args); callback(null, 'done', ''); },
    });
    const result = await runtime.restartDaemon();
    assert.deepEqual(invocations, [['stop'], ['daemon']]);
    assert.equal(result.ok, true);
    assert.deepEqual(app.calls, []);
});

test('a daemon that fails to start again is reported, not shown as restarted', async () => {
    const runtime = createUpdateRuntime({
        app: fakeApp(),
        getCliPath: () => '/opt/zorai/zorai',
        exists: () => true,
        env: {},
        platform: 'linux',
        execFileImpl: (file, args, _options, callback) => {
            if (args[0] === 'daemon') callback(new Error('exit 1'), '', 'daemon failed to bind socket');
            else callback(null, '', '');
        },
    });
    const result = await runtime.restartDaemon();
    assert.equal(result.ok, false);
    assert.match(result.output, /failed to bind socket/);
});

test('the desktop executable named zorai is never mistaken for the CLI, which would open another app window instead of upgrading', () => {
    const resolved = resolveUpgradeCliPath({
        env: {},
        platform: 'darwin',
        exePath: '/Applications/zorai.app/Contents/MacOS/zorai',
        getCliPath: () => '/Applications/zorai.app/Contents/Resources/bin/zorai',
        exists: (candidate) => candidate.startsWith('/Applications/zorai.app/'),
    });
    assert.equal(resolved, '/Applications/zorai.app/Contents/Resources/bin/zorai');
});

test('an app running from the read-only disk image refuses to update instead of looping on the old version', async () => {
    const calls = [];
    const app = { ...fakeApp(), getPath: () => '/Volumes/zorai/zorai.app/Contents/MacOS/zorai' };
    const runtime = createUpdateRuntime({ app, platform: 'darwin', getCliPath: () => '/Volumes/zorai/zorai.app/Contents/Resources/bin/zorai', exists: () => true, execFileImpl: (...args) => calls.push(args) });
    const result = await runtime.install();
    assert.equal(result.ok, false);
    assert.match(result.output, /Applications/);
    assert.equal(calls.length, 0);
    assert.equal(runtime.stopDaemonIfUpdated(), false);
});

function dmgApp() {
    const calls = [];
    const paths = { exe: '/Applications/zorai.app/Contents/MacOS/zorai', userData: '/Users/me/Library/Application Support/zorai' };
    return {
        calls,
        getVersion: () => '1.0.43',
        getPath: (name) => paths[name],
        relaunch: (options) => calls.push(['relaunch', options]),
        exit: (code) => calls.push(['exit', code]),
    };
}

function dmgRuntime(app, overrides = {}) {
    const execCalls = [];
    const spawned = [];
    const runtime = createUpdateRuntime({
        app,
        platform: 'darwin',
        getCliPath: () => '/Applications/zorai.app/Contents/Resources/bin/zorai',
        exists: (candidate) => candidate.startsWith('/Applications/zorai.app/') || candidate.endsWith('/pending-update/zorai-desktop.app/Contents/MacOS/zorai'),
        execFileImpl: (file, args, options, callback) => { execCalls.push([file, args, options.env?.ZORAI_INSTALL_DIR]); callback(null, 'ok', ''); },
        execFileSyncImpl: (file, args) => execCalls.push([file, args]),
        spawnImpl: (file, args, options) => { spawned.push([file, args, options]); return { unref() {} }; },
        removeDir: () => {},
        canWrite: () => true,
        pid: 4242,
        ...overrides,
    });
    return { runtime, execCalls, spawned };
}

test('an app installed from the DMG downloads the new app into staging instead of upgrading the CLI inside its own bundle', async () => {
    const app = dmgApp();
    const { runtime, execCalls } = dmgRuntime(app);
    const result = await runtime.install();
    assert.equal(result.ok, true);
    assert.deepEqual(execCalls, [['/bin/sh', ['-c', `curl -fsSL ${INSTALL_SCRIPT_URL} | sh`], '/Users/me/Library/Application Support/zorai/pending-update']]);
});

test('restart after a DMG update replaces /Applications/zorai.app once the app exits and reopens it, instead of relaunching the old bundle', async () => {
    const app = dmgApp();
    const { runtime, execCalls, spawned } = dmgRuntime(app);
    await runtime.install();
    await runtime.restart();
    assert.deepEqual(execCalls.at(-1), ['/Applications/zorai.app/Contents/Resources/bin/zorai', ['stop'], undefined]);
    assert.equal(spawned.length, 1);
    assert.deepEqual(spawned[0][1].slice(3), ['4242', '/Applications/zorai.app', '/Users/me/Library/Application Support/zorai/pending-update/zorai-desktop.app', '/Users/me/Library/Application Support/zorai/pending-update', '1']);
    assert.equal(spawned[0][2].detached, true);
    assert.deepEqual(app.calls, [['exit', 0]]);
});

test('quitting after a DMG update still swaps in the new app on exit, without reopening it', async () => {
    const app = dmgApp();
    const { runtime, spawned } = dmgRuntime(app);
    await runtime.install();
    assert.equal(runtime.stopDaemonIfUpdated(), true);
    assert.equal(spawned.length, 1);
    assert.equal(spawned[0][1].at(-1), '0');
    assert.equal(runtime.stopDaemonIfUpdated(), false);
    assert.equal(spawned.length, 1);
});

test('a DMG app in a folder this account cannot write refuses to update instead of downloading for nothing', async () => {
    const app = dmgApp();
    const { runtime, execCalls, spawned } = dmgRuntime(app, { canWrite: () => false });
    const result = await runtime.install();
    assert.equal(result.ok, false);
    assert.match(result.output, /Applications/);
    assert.equal(execCalls.length, 0);
    await runtime.restart();
    assert.equal(spawned.length, 0);
});

test('a direct-installer app next to its own CLI keeps using zorai upgrade, which already replaces that app in place', async () => {
    const calls = [];
    const app = { ...fakeApp(), getPath: () => '/Users/me/.local/bin/zorai-desktop.app/Contents/MacOS/zorai' };
    const runtime = createUpdateRuntime({
        app,
        platform: 'darwin',
        getCliPath: () => '/Users/me/.local/bin/zorai-desktop.app/Contents/Resources/bin/zorai',
        exists: (candidate) => candidate === '/Users/me/.local/bin/zorai',
        execFileImpl: (file, args, options, callback) => { calls.push([file, args]); callback(null, '', ''); },
    });
    await runtime.install();
    assert.deepEqual(calls, [['/Users/me/.local/bin/zorai', ['upgrade', '--keep-running']]]);
});

test('the swap script waits for the app to exit, replaces the bundle with the staged one and cleans up', () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'zorai-swap-'));
    const bundle = path.join(root, 'Applications', 'zorai.app');
    const staging = path.join(root, 'pending-update');
    const staged = path.join(staging, 'zorai-desktop.app');
    fs.mkdirSync(bundle, { recursive: true });
    fs.writeFileSync(path.join(bundle, 'version'), '1.0.43');
    fs.mkdirSync(staged, { recursive: true });
    fs.writeFileSync(path.join(staged, 'version'), '1.0.44');
    const exited = execFileSync('/bin/sh', ['-c', 'echo $$']).toString().trim();
    execFileSync('/bin/sh', ['-c', SWAP_APP_SCRIPT, 'zorai-update', exited, bundle, staged, staging, '0']);
    assert.equal(fs.readFileSync(path.join(bundle, 'version'), 'utf8'), '1.0.44');
    assert.equal(fs.existsSync(`${bundle}.previous`), false);
    assert.equal(fs.existsSync(staging), false);
    fs.rmSync(root, { recursive: true, force: true });
});
