const { spawn, spawnSync } = require('child_process');
const path = require('path');
const fs = require('fs');
const os = require('os');
const { getZoraiDataDir } = require('./app-data.cjs');

function resolveCommandPath(command) {
    const checker = process.platform === 'win32' ? 'where' : 'which';
    const result = spawnSync(checker, [command], { encoding: 'utf8' });
    if (result.status !== 0) return null;
    return String(result.stdout || '').split(/\r?\n/).map((value) => value.trim()).find(Boolean) ?? null;
}

function managedLanguageServerPath(command) {
    const executable = process.platform === 'win32' ? `${command}.cmd` : command;
    if (['vscode-html-language-server', 'vscode-css-language-server', 'vscode-json-language-server'].includes(command)) return path.join(getZoraiDataDir(), 'language-servers', 'web', 'node_modules', '.bin', executable);
    if (command === 'typescript-language-server') return path.join(getZoraiDataDir(), 'language-servers', 'typescript', 'node_modules', '.bin', executable);
    if (command === 'pyright-langserver') return path.join(getZoraiDataDir(), 'language-servers', 'pyright', 'node_modules', '.bin', executable);
    if (command === 'gopls') return path.join(process.env.GOPATH || path.join(os.homedir(), 'go'), 'bin', process.platform === 'win32' ? 'gopls.exe' : 'gopls');
    return null;
}

function managedLanguageServerComplete(command) {
    if (command !== 'typescript-language-server') return true;
    return fs.existsSync(path.join(getZoraiDataDir(), 'language-servers', 'typescript', 'node_modules', 'typescript', 'lib', 'tsserver.js'));
}

function usableLanguageServerCommand(command) {
    const managed = managedLanguageServerPath(command);
    if (managed && fs.existsSync(managed) && managedLanguageServerComplete(command)) return managed;
    const resolved = resolveCommandPath(command);
    if (!resolved) return null;
    // The Ubuntu/Snap rust-analyzer shim recursively invokes itself when the
    // active rustup toolchain does not contain the component.
    if (command === 'rust-analyzer' && resolved.replace(/\\/g, '/') === '/snap/bin/rust-analyzer') {
        const rustup = resolveCommandPath('rustup');
        if (!rustup) return null;
        const toolchainBinary = spawnSync(rustup, ['which', 'rust-analyzer'], { encoding: 'utf8', timeout: 5000 });
        if (toolchainBinary.status !== 0) return null;
        return String(toolchainBinary.stdout || '').trim() || null;
    }
    return resolved;
}

function typeScriptLanguageId(relativePath) {
    const extension = path.extname(String(relativePath || '')).toLowerCase();
    if (extension === '.tsx') return 'typescriptreact';
    if (extension === '.jsx') return 'javascriptreact';
    if (['.js', '.mjs', '.cjs'].includes(extension)) return 'javascript';
    return 'typescript';
}

function languageServerSpec(language) {
    const normalized = String(language || '').toLowerCase();
    if (normalized === 'rust') return { id: 'rust', languageId: 'rust', candidates: [{ command: 'rust-analyzer', args: [] }] };
    if (normalized === 'python') return {
        id: 'python', languageId: 'python', candidates: [
            { command: 'basedpyright-langserver', args: ['--stdio'] },
            { command: 'pyright-langserver', args: ['--stdio'] },
        ],
    };
    if (['typescript', 'javascript', 'typescriptreact', 'javascriptreact'].includes(normalized)) return {
        id: 'typescript',
        languageId: 'typescript',
        languageIdFor: typeScriptLanguageId,
        candidates: [{ command: 'typescript-language-server', args: ['--stdio'] }],
    };
    if (normalized === 'html') return {
        id: 'html',
        installId: 'web',
        languageId: 'html',
        initializationOptions: { provideFormatter: true, embeddedLanguages: { css: true, javascript: true } },
        candidates: [{ command: 'vscode-html-language-server', args: ['--stdio'] }],
    };
    if (['css', 'scss', 'less'].includes(normalized)) return {
        id: 'css',
        installId: 'web',
        languageId: normalized,
        initializationOptions: { provideFormatter: true },
        candidates: [{ command: 'vscode-css-language-server', args: ['--stdio'] }],
    };
    if (normalized === 'go') return { id: 'go', languageId: 'go', candidates: [{ command: 'gopls', args: [] }] };
    if (['c', 'cpp', 'objective-c'].includes(normalized)) return { id: 'cpp', languageId: normalized === 'c' ? 'c' : 'cpp', candidates: [{ command: 'clangd', args: ['--background-index'] }] };
    if (normalized === 'json') return { id: 'json', installId: 'web', languageId: 'json', initializationOptions: { provideFormatter: true }, candidates: [{ command: 'vscode-json-language-server', args: ['--stdio'] }] };
    if (normalized === 'yaml') return { id: 'yaml', languageId: 'yaml', candidates: [{ command: 'yaml-language-server', args: ['--stdio'] }] };
    if (normalized === 'lua') return { id: 'lua', languageId: 'lua', candidates: [{ command: 'lua-language-server', args: [] }] };
    if (normalized === 'ruby') return { id: 'ruby', languageId: 'ruby', candidates: [{ command: 'ruby-lsp', args: [] }, { command: 'solargraph', args: ['stdio'] }] };
    if (normalized === 'java') return { id: 'java', languageId: 'java', candidates: [{ command: 'jdtls', args: [] }] };
    if (normalized === 'csharp') return { id: 'csharp', languageId: 'csharp', candidates: [{ command: 'csharp-ls', args: [] }] };
    if (normalized === 'shell') return { id: 'shell', languageId: 'shellscript', candidates: [{ command: 'bash-language-server', args: ['start'] }] };
    if (normalized === 'zig') return { id: 'zig', languageId: 'zig', candidates: [{ command: 'zls', args: [] }] };
    return null;
}

function userLanguageServerSpec(language, configPath = path.join(getZoraiDataDir(), 'lsp.json')) {
    let config;
    try { config = JSON.parse(fs.readFileSync(configPath, 'utf8')); } catch { return null; }
    const normalized = String(language || '').toLowerCase();
    const entry = config?.servers?.[normalized];
    if (!entry || typeof entry.command !== 'string' || !entry.command.trim()) return null;
    return {
        id: `user-${normalized}`,
        languageId: typeof entry.languageId === 'string' ? entry.languageId : normalized,
        initializationOptions: entry.initializationOptions && typeof entry.initializationOptions === 'object' ? entry.initializationOptions : undefined,
        candidates: [{ command: entry.command.trim(), args: Array.isArray(entry.args) ? entry.args.map(String) : [] }],
    };
}

const LANGUAGE_SERVER_INSTALLS = [
    { id: 'rust', label: 'Rust', server: 'rust-analyzer', requires: 'rustup', command: 'rustup', args: () => ['component', 'add', 'rust-analyzer'] },
    { id: 'typescript', label: 'TypeScript / JavaScript', server: 'typescript-language-server', requires: 'npm', command: 'npm', args: () => ['install', '--no-audit', '--no-fund', '--prefix', path.join(getZoraiDataDir(), 'language-servers', 'typescript'), 'typescript-language-server', 'typescript@6'] },
    { id: 'web', probe: 'html', label: 'HTML / CSS', server: 'vscode-langservers-extracted', requires: 'npm', command: 'npm', args: () => ['install', '--no-audit', '--no-fund', '--prefix', path.join(getZoraiDataDir(), 'language-servers', 'web'), 'vscode-langservers-extracted'] },
    { id: 'python', label: 'Python', server: 'pyright', requires: 'npm', command: 'npm', args: () => ['install', '--no-audit', '--no-fund', '--prefix', path.join(getZoraiDataDir(), 'language-servers', 'pyright'), 'pyright'] },
    { id: 'go', label: 'Go', server: 'gopls', requires: 'go', command: 'go', args: () => ['install', 'golang.org/x/tools/gopls@latest'] },
    { id: 'cpp', label: 'C / C++', server: 'clangd', requires: null, hint: 'Install clangd with your system package manager (apt install clangd, brew install llvm, or the LLVM installer on Windows).' },
];

function languageServerStatuses() {
    return LANGUAGE_SERVER_INSTALLS.map((entry) => {
        const server = resolveLanguageServerUncached(entry.probe ?? entry.id);
        const requirementFound = entry.requires ? Boolean(resolveCommandPath(entry.requires)) : false;
        return {
            id: entry.id,
            label: entry.label,
            server: entry.server,
            installedCommand: server?.command ?? null,
            installable: Boolean(entry.command) && requirementFound,
            requires: entry.requires,
            installCommand: entry.command ? [entry.command, ...entry.args()].join(' ') : null,
            hint: entry.hint ?? (entry.requires && !requirementFound ? `${entry.requires} was not found on PATH; install it first.` : null),
        };
    });
}

function installLanguageServer(id) {
    const entry = LANGUAGE_SERVER_INSTALLS.find((candidate) => candidate.id === id);
    if (!entry?.command) return Promise.resolve({ ok: false, output: entry?.hint ?? `No installer is available for ${id}.` });
    const command = resolveCommandPath(entry.command);
    if (!command) return Promise.resolve({ ok: false, output: `${entry.command} was not found on PATH.` });
    return new Promise((resolve) => {
        let output = '';
        const append = (chunk) => { output = `${output}${chunk.toString('utf8')}`.slice(-8000); };
        const child = spawn(command, entry.args(), { cwd: os.homedir(), windowsHide: true, shell: process.platform === 'win32', stdio: ['ignore', 'pipe', 'pipe'] });
        child.stdout.on('data', append);
        child.stderr.on('data', append);
        child.once('error', (error) => resolve({ ok: false, output: error.message }));
        child.once('exit', (code) => {
            languageServerCache.clear();
            const installed = resolveLanguageServerUncached(entry.probe ?? entry.id)?.command ?? null;
            resolve({ ok: code === 0 && Boolean(installed), installedCommand: installed, output: code === 0 && !installed ? `${output}\nInstalled, but ${entry.server} could not be found afterwards.` : output });
        });
    });
}

const LANGUAGE_SERVER_CACHE_TTL_MS = 60000;
const languageServerCache = new Map();

function resolveLanguageServer(language) {
    const cacheKey = String(language || '').toLowerCase();
    const cached = languageServerCache.get(cacheKey);
    if (cached && Date.now() - cached.at < LANGUAGE_SERVER_CACHE_TTL_MS) return cached.server;
    const server = resolveLanguageServerUncached(language);
    languageServerCache.set(cacheKey, { server, at: Date.now() });
    return server;
}

function resolveLanguageServerUncached(language) {
    const spec = userLanguageServerSpec(language) ?? languageServerSpec(language);
    if (!spec) return null;
    const candidate = spec.candidates
        .map((entry) => ({ ...entry, resolvedCommand: usableLanguageServerCommand(entry.command) }))
        .find((entry) => entry.resolvedCommand);
    return candidate
        ? { ...spec, command: candidate.resolvedCommand, displayCommand: candidate.command, args: candidate.args }
        : { ...spec, command: null, displayCommand: null, args: [] };
}

module.exports = { installLanguageServer, languageServerSpec, languageServerStatuses, resolveLanguageServer, typeScriptLanguageId, usableLanguageServerCommand, userLanguageServerSpec };
