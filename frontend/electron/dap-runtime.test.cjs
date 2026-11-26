const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { execFileSync } = require('node:child_process');
const { createDapRuntime, LISTEN_PATTERN } = require('./main/dap-runtime.cjs');
const { adapterForType, stripJsonComments, substitute, readLaunchConfigurations } = require('./main/dap-adapters.cjs');

test('launch.json with comments and VS Code variables resolves to concrete paths', () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'zorai-launch-'));
    fs.mkdirSync(path.join(root, '.vscode'));
    fs.writeFileSync(path.join(root, '.vscode', 'launch.json'), '{\n // comment\n "configurations": [{ "name": "Run", "type": "python", "request": "launch", "program": "${workspaceFolder}/app.py", "args": ["${fileBasename}"], /* x */ },],\n}');
    const [config] = readLaunchConfigurations(root);
    assert.equal(config.name, 'Run');
    assert.deepEqual(substitute(config, { root, file: 'src/a.py' }).args, ['a.py']);
    assert.equal(substitute(config, { root, file: null }).program, `${root}/app.py`);
    assert.equal(JSON.parse(stripJsonComments('{"url": "http://x//y"}')).url, 'http://x//y');
    fs.rmSync(root, { recursive: true, force: true });
});

test('TCP adapters announce their port in a line we can parse', () => {
    assert.deepEqual(LISTEN_PATTERN.exec('DAP server listening at: 127.0.0.1:38211').slice(1), ['127.0.0.1', '38211']);
    assert.equal(LISTEN_PATTERN.exec('Debug server listening at ::1:8123')[2], '8123');
});

const gdb = adapterForType('gdb')?.resolve();
let cc = null;
try { cc = execFileSync('which', ['gcc'], { encoding: 'utf8' }).trim(); } catch {}

test('a real gdb session stops at a breakpoint, shows locals, evaluates and runs to the end', { skip: !gdb || !cc, timeout: 60000 }, async () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'zorai-dap-'));
    fs.writeFileSync(path.join(root, 'main.c'), '#include <stdio.h>\nint main(void) {\n  int answer = 41;\n  answer += 1;\n  printf("%d\\n", answer);\n  return 0;\n}\n');
    execFileSync(cc, ['-g', '-O0', '-o', path.join(root, 'main'), path.join(root, 'main.c')]);
    const events = [];
    const waiters = [];
    const webContents = { isDestroyed: () => false, send: (_channel, payload) => { events.push(payload); waiters.splice(0).forEach((check) => check()); } };
    const waitFor = (event) => new Promise((resolve, reject) => {
        const timer = setTimeout(() => reject(new Error(`timed out waiting for ${event}: ${JSON.stringify(events.map((entry) => entry.event))}`)), 20000);
        const check = () => {
            const found = events.find((entry) => entry.event === event && !entry.seen);
            if (found) { found.seen = true; clearTimeout(timer); resolve(found); } else waiters.push(check);
        };
        check();
    });
    const runtime = createDapRuntime();
    let sessionId;
    try {
    ({ sessionId } = await runtime.start(webContents, root, 'main.c', { name: 'gdb', type: 'gdb', request: 'launch', program: '${workspaceFolder}/main' }, { 'main.c': [{ line: 4 }] }, []));
    const stopped = await waitFor('stopped');
    const threadId = stopped.body.threadId ?? (await runtime.request(sessionId, 'threads', {})).threads[0].id;
    const { stackFrames } = await runtime.request(sessionId, 'stackTrace', { threadId });
    assert.equal(stackFrames[0].line, 4);
    const { scopes } = await runtime.request(sessionId, 'scopes', { frameId: stackFrames[0].id });
    const { variables } = await runtime.request(sessionId, 'variables', { variablesReference: scopes[0].variablesReference });
    assert.equal(variables.find((variable) => variable.name === 'answer')?.value, '41');
    const evaluated = await runtime.request(sessionId, 'evaluate', { expression: 'answer * 2', frameId: stackFrames[0].id, context: 'watch' });
    assert.equal(evaluated.result, '82');
    await runtime.request(sessionId, 'continue', { threadId });
    await Promise.race([waitFor('terminated'), waitFor('exited')]);
    } finally {
        await runtime.stopAll();
        fs.rmSync(root, { recursive: true, force: true });
    }
});

let cargo = null;
try { cargo = execFileSync('which', ['cargo'], { encoding: 'utf8' }).trim(); } catch {}

test('Debug Test on a Rust test builds the test binary and stops inside the test', { skip: !gdb || !cargo, timeout: 240000 }, async () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'zorai-rust-dap-'));
    fs.writeFileSync(path.join(root, 'Cargo.toml'), '[package]\nname = "dbg_probe"\nversion = "0.1.0"\nedition = "2021"\n');
    fs.mkdirSync(path.join(root, 'src'));
    fs.writeFileSync(path.join(root, 'src', 'lib.rs'), '#[cfg(test)]\nmod tests {\n    #[test]\n    fn adds() {\n        let value = 2;\n        assert_eq!(value + 1, 3);\n    }\n}\n');
    const events = [];
    const waiters = [];
    const webContents = { isDestroyed: () => false, send: (_channel, payload) => { events.push(payload); waiters.splice(0).forEach((check) => check()); } };
    const waitFor = (event) => new Promise((resolve, reject) => {
        const timer = setTimeout(() => reject(new Error(`timed out waiting for ${event}: ${JSON.stringify(events.map((entry) => [entry.event, entry.body?.output]))}`)), 120000);
        const check = () => {
            const found = events.find((entry) => entry.event === event && !entry.seen);
            if (found) { found.seen = true; clearTimeout(timer); resolve(found); } else waiters.push(check);
        };
        check();
    });
    const runtime = createDapRuntime();
    try {
        const { sessionId } = await runtime.start(webContents, root, 'src/lib.rs', { name: 'Debug adds', type: 'rust-test', request: 'launch', testFile: 'src/lib.rs', args: ['adds', '--nocapture'] }, { 'src/lib.rs': [{ line: 6 }] }, []);
        const stopped = await waitFor('stopped');
        const threadId = stopped.body.threadId ?? (await runtime.request(sessionId, 'threads', {})).threads[0].id;
        const { stackFrames } = await runtime.request(sessionId, 'stackTrace', { threadId });
        assert.equal(stackFrames[0].line, 6);
    } finally {
        await runtime.stopAll();
        fs.rmSync(root, { recursive: true, force: true });
    }
});
