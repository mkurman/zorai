const crypto = require('crypto');
const fs = require('fs');
const net = require('net');
const os = require('os');
const path = require('path');

const HELPER_SOURCE = `const net = require('net');
const prompt = process.argv.slice(2).join(' ');
const socket = net.connect(process.env.ZORAI_ASKPASS_SOCKET || '');
let reply = '';
socket.setEncoding('utf8');
socket.on('connect', () => socket.write(JSON.stringify({ token: process.env.ZORAI_ASKPASS_TOKEN || '', prompt }) + '\\n'));
socket.on('data', (chunk) => { reply += chunk; });
socket.on('error', () => process.exit(1));
socket.on('close', () => {
    try {
        const answer = JSON.parse(reply);
        if (typeof answer.value !== 'string') process.exit(1);
        process.stdout.write(answer.value + '\\n', () => process.exit(0));
    } catch {
        process.exit(1);
    }
});
`;

const MAX_REQUEST_BYTES = 64 * 1024;

let state = null;

function quoteSh(value) {
    return `'${String(value).replace(/'/g, `'\\''`)}'`;
}

function writeHelper(dir, runtimePath) {
    const helperPath = path.join(dir, 'askpass-helper.cjs');
    fs.writeFileSync(helperPath, HELPER_SOURCE, { mode: 0o600 });
    if (process.platform === 'win32') {
        const scriptPath = path.join(dir, 'askpass.cmd');
        fs.writeFileSync(scriptPath, `@echo off\r\nset ELECTRON_RUN_AS_NODE=1\r\n"${runtimePath}" "${helperPath}" %*\r\n`);
        return scriptPath;
    }
    const scriptPath = path.join(dir, 'askpass.sh');
    fs.writeFileSync(scriptPath, `#!/bin/sh\nELECTRON_RUN_AS_NODE=1 exec ${quoteSh(runtimePath)} ${quoteSh(helperPath)} "$@"\n`, { mode: 0o700 });
    return scriptPath;
}

function handleConnection(socket, token, requestPrompt) {
    let buffer = '';
    let settled = false;
    let controller = null;
    socket.setEncoding('utf8');
    const finish = (value) => {
        if (settled) return;
        settled = true;
        if (!socket.destroyed) socket.end(JSON.stringify(typeof value === 'string' ? { value } : {}));
    };
    socket.on('data', (chunk) => {
        if (controller) return;
        buffer += chunk;
        if (buffer.length > MAX_REQUEST_BYTES) return socket.destroy();
        const newline = buffer.indexOf('\n');
        if (newline < 0) return;
        let request = null;
        try {
            request = JSON.parse(buffer.slice(0, newline));
        } catch {
            return socket.destroy();
        }
        const provided = Buffer.from(String(request?.token ?? ''));
        const expected = Buffer.from(token);
        if (provided.length !== expected.length || !crypto.timingSafeEqual(provided, expected)) return socket.destroy();
        controller = new AbortController();
        Promise.resolve(requestPrompt(String(request.prompt ?? ''), controller.signal))
            .then(finish, () => finish(null));
    });
    socket.on('close', () => {
        settled = true;
        controller?.abort();
    });
    socket.on('error', () => {});
}

function startGitAskpass({ requestPrompt, runtimePath = process.execPath, baseDir = os.tmpdir() }) {
    if (state) return state;
    const dir = fs.mkdtempSync(path.join(baseDir, 'zorai-askpass-'));
    fs.chmodSync(dir, 0o700);
    const token = crypto.randomBytes(32).toString('hex');
    const socketPath = process.platform === 'win32'
        ? `\\\\.\\pipe\\zorai-askpass-${crypto.randomBytes(8).toString('hex')}`
        : path.join(dir, 'askpass.sock');
    const scriptPath = writeHelper(dir, runtimePath);
    const server = net.createServer((socket) => handleConnection(socket, token, requestPrompt));
    server.listen(socketPath);
    const env = {
        GIT_ASKPASS: scriptPath,
        SSH_ASKPASS: scriptPath,
        SSH_ASKPASS_REQUIRE: 'force',
        GIT_TERMINAL_PROMPT: '0',
        ZORAI_ASKPASS_SOCKET: socketPath,
        ZORAI_ASKPASS_TOKEN: token,
        ...(process.env.DISPLAY ? {} : { DISPLAY: ':0' }),
    };
    const stop = () => {
        server.close();
        fs.rmSync(dir, { recursive: true, force: true });
        state = null;
    };
    state = { env, stop, server };
    return state;
}

let forwardedEnv = null;

function setGitAskpassEnv(env) {
    forwardedEnv = env && typeof env === 'object' ? { ...env } : null;
}

function gitAskpassEnv() {
    return state ? state.env : forwardedEnv ?? {};
}

module.exports = { startGitAskpass, gitAskpassEnv, setGitAskpassEnv };
