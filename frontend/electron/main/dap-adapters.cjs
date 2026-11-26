const fs = require('fs');
const path = require('path');
const { execFile, spawnSync } = require('child_process');

function which(command) {
    const result = spawnSync(process.platform === 'win32' ? 'where' : 'which', [command], { encoding: 'utf8' });
    return result.status === 0 ? String(result.stdout).split(/\r?\n/).map((line) => line.trim()).find(Boolean) ?? null : null;
}

function pythonWithDebugpy() {
    for (const python of ['python3', 'python']) {
        const resolved = which(python);
        if (resolved && spawnSync(resolved, ['-c', 'import debugpy'], { encoding: 'utf8', timeout: 5000 }).status === 0) return resolved;
    }
    return null;
}

const ADAPTERS = [
    {
        id: 'python',
        types: ['python', 'debugpy'],
        label: 'Python (debugpy)',
        hint: 'Install debugpy: pip install debugpy',
        resolve: () => {
            const python = pythonWithDebugpy();
            return python ? { command: python, args: ['-m', 'debugpy.adapter'] } : null;
        },
    },
    {
        id: 'node',
        types: ['node', 'pwa-node', 'node-terminal'],
        label: 'Node.js (js-debug)',
        hint: 'Put js-debug-adapter (from the vscode-js-debug release) on PATH.',
        resolve: () => {
            const adapter = which('js-debug-adapter');
            return adapter ? { command: adapter, args: [], tcp: true } : null;
        },
    },
    {
        id: 'lldb',
        types: ['lldb', 'lldb-dap', 'codelldb'],
        label: 'LLDB (lldb-dap)',
        hint: 'Install lldb-dap (LLVM) or codelldb.',
        resolve: () => {
            const lldb = which('lldb-dap') ?? which('lldb-vscode');
            return lldb ? { command: lldb, args: [] } : null;
        },
    },
    {
        id: 'gdb',
        types: ['gdb', 'cppdbg'],
        label: 'GDB (gdb 14+ --interpreter=dap)',
        hint: 'Install gdb 14 or newer.',
        resolve: () => {
            const gdb = which('gdb');
            if (!gdb) return null;
            const version = /(\d+)\.\d+/.exec(spawnSync(gdb, ['--version'], { encoding: 'utf8', timeout: 5000 }).stdout ?? '');
            return version && Number(version[1]) >= 14 ? { command: gdb, args: ['--interpreter=dap', '--quiet'] } : null;
        },
    },
    {
        id: 'go',
        types: ['go'],
        label: 'Go (Delve)',
        hint: 'Install Delve: go install github.com/go-delve/delve/cmd/dlv@latest',
        resolve: () => {
            const dlv = which('dlv');
            return dlv ? { command: dlv, args: ['dap', '--listen=127.0.0.1:0'], listen: true } : null;
        },
    },
];

function adapterForType(type) {
    const normalized = String(type ?? '').toLowerCase();
    return ADAPTERS.find((adapter) => adapter.types.includes(normalized)) ?? null;
}

function adapterStatuses() {
    return ADAPTERS.map((adapter) => ({ id: adapter.id, label: adapter.label, available: Boolean(adapter.resolve()), hint: adapter.hint }));
}

function stripJsonComments(text) {
    let out = '';
    let inString = false;
    for (let index = 0; index < text.length; index += 1) {
        const char = text[index];
        if (inString) {
            out += char;
            if (char === '\\') { out += text[index + 1] ?? ''; index += 1; } else if (char === '"') inString = false;
            continue;
        }
        if (char === '"') { inString = true; out += char; continue; }
        if (char === '/' && text[index + 1] === '/') { while (index < text.length && text[index] !== '\n') index += 1; out += '\n'; continue; }
        if (char === '/' && text[index + 1] === '*') { index = text.indexOf('*/', index + 2); if (index === -1) break; index += 1; continue; }
        out += char;
    }
    return out.replace(/,(\s*[}\]])/g, '$1');
}

function substitute(value, context) {
    if (typeof value === 'string') {
        return value.replace(/\$\{(\w+)(?::([^}]+))?\}/g, (match, name, argument) => {
            if (name === 'env') return process.env[argument] ?? '';
            if (name === 'workspaceFolder' || name === 'workspaceRoot') return context.root;
            if (name === 'workspaceFolderBasename') return path.basename(context.root);
            if (name === 'file') return context.file ? path.join(context.root, context.file) : '';
            if (name === 'relativeFile') return context.file ?? '';
            if (name === 'fileBasename') return context.file ? path.basename(context.file) : '';
            if (name === 'fileBasenameNoExtension') return context.file ? path.basename(context.file, path.extname(context.file)) : '';
            if (name === 'fileDirname') return context.file ? path.dirname(path.join(context.root, context.file)) : '';
            if (name === 'pathSeparator') return path.sep;
            return match;
        });
    }
    if (Array.isArray(value)) return value.map((entry) => substitute(entry, context));
    if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value).map(([key, entry]) => [key, substitute(entry, context)]));
    return value;
}

function readLaunchConfigurations(root) {
    const configs = [];
    for (const file of ['.zorai/launch.json', '.vscode/launch.json']) {
        let parsed;
        try { parsed = JSON.parse(stripJsonComments(fs.readFileSync(path.join(root, file), 'utf8'))); } catch { continue; }
        for (const config of Array.isArray(parsed?.configurations) ? parsed.configurations : []) {
            if (config && typeof config.name === 'string' && typeof config.type === 'string') configs.push({ ...config, source: file });
        }
        for (const compound of Array.isArray(parsed?.compounds) ? parsed.compounds : []) {
            const members = Array.isArray(compound?.configurations) ? compound.configurations.filter((name) => typeof name === 'string') : [];
            if (typeof compound?.name === 'string' && members.length > 0) configs.push({ ...compound, name: compound.name, type: 'compound', request: 'launch', configurations: members, source: file });
        }
    }
    return configs;
}

function defaultConfigurations(file) {
    const extension = path.extname(String(file ?? '')).toLowerCase();
    if (extension === '.py') return [{ name: 'Python: Current File', type: 'python', request: 'launch', program: '${file}', console: 'internalConsole', source: 'default' }];
    if (['.js', '.mjs', '.cjs', '.ts'].includes(extension)) return [{ name: 'Node: Current File', type: 'node', request: 'launch', program: '${file}', source: 'default' }];
    if (extension === '.go') return [{ name: 'Go: Current Package', type: 'go', request: 'launch', mode: 'debug', program: '${fileDirname}', source: 'default' }];
    return [];
}

function cargoTestBinary(root, file) {
    let crateDir = path.dirname(path.join(root, file));
    while (crateDir.startsWith(root) && !fs.existsSync(path.join(crateDir, 'Cargo.toml'))) crateDir = path.dirname(crateDir);
    if (!fs.existsSync(path.join(crateDir, 'Cargo.toml'))) return Promise.reject(new Error('No Cargo.toml found above this test.'));
    const absoluteFile = path.join(root, file);
    return new Promise((resolve, reject) => {
        execFile('cargo', ['test', '--no-run', '--message-format=json'], { cwd: crateDir, maxBuffer: 64 * 1024 * 1024, timeout: 600000 }, (error, stdout, stderr) => {
            const artifacts = String(stdout).split('\n').flatMap((line) => {
                try {
                    const message = JSON.parse(line);
                    return message.reason === 'compiler-artifact' && message.profile?.test && message.executable ? [message] : [];
                } catch { return []; }
            });
            const integration = artifacts.find((artifact) => artifact.target?.src_path === absoluteFile);
            const local = artifacts.filter((artifact) => String(artifact.target?.src_path ?? '').startsWith(crateDir));
            const chosen = integration ?? local.find((artifact) => artifact.target?.kind?.includes('lib')) ?? local[0] ?? artifacts[0];
            if (chosen) resolve({ program: chosen.executable, cwd: crateDir });
            else reject(new Error(error ? `cargo test --no-run failed: ${String(stderr).trim().split('\n').slice(-5).join('\n')}` : 'cargo produced no test binary.'));
        });
    });
}

module.exports = { adapterForType, adapterStatuses, cargoTestBinary, defaultConfigurations, readLaunchConfigurations, stripJsonComments, substitute };
