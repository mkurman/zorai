const test = require('node:test');
const assert = require('node:assert/strict');
const { serverSupports, typeScriptLanguageId, LspMessageReader, encodeLspMessage, languageServerSpec, normalizeDiagnostic, resolveLanguageServer } = require('./main/lsp-runtime.cjs');

test('LSP message reader handles split and concatenated JSON-RPC frames', () => {
    const messages = [];
    const reader = new LspMessageReader((message) => messages.push(message));
    const first = encodeLspMessage({ jsonrpc: '2.0', id: 1, result: { ready: true } });
    const second = encodeLspMessage({ jsonrpc: '2.0', method: 'window/logMessage', params: { message: 'ok' } });
    reader.push(first.subarray(0, 12));
    assert.equal(messages.length, 0);
    reader.push(Buffer.concat([first.subarray(12), second]));
    assert.equal(messages.length, 2);
    assert.equal(messages[0].result.ready, true);
    assert.equal(messages[1].params.message, 'ok');
});

test('LSP diagnostic normalization converts zero-based ranges for Monaco', () => {
    assert.deepEqual(normalizeDiagnostic({
        message: 'borrowed value moved',
        severity: 1,
        source: 'rustc',
        code: 'E0382',
        range: { start: { line: 9, character: 4 }, end: { line: 9, character: 12 } },
    }), {
        message: 'borrowed value moved',
        severity: 1,
        source: 'rustc',
        code: 'E0382',
        startLine: 10,
        startColumn: 5,
        endLine: 10,
        endColumn: 13,
    });
});

test('broken Snap rust-analyzer shim is not advertised as available', () => {
    const resolved = resolveLanguageServer('rust');
    if (resolved?.command) {
        assert.notEqual(resolved.command.replace(/\\/g, '/'), '/snap/bin/rust-analyzer');
    } else {
        assert.equal(resolved?.id, 'rust');
    }
});

test('language server specifications cover Rust, Python, Go, and C++', () => {
    assert.equal(languageServerSpec('rust').candidates[0].command, 'rust-analyzer');
    assert.equal(languageServerSpec('python').candidates.length, 2);
    assert.equal(languageServerSpec('go').candidates[0].command, 'gopls');
    assert.equal(languageServerSpec('cpp').candidates[0].command, 'clangd');
    assert.equal(languageServerSpec('markdown').candidates[0].command, 'marksman');
    assert.equal(languageServerSpec('markdown').candidates[0].args[0], 'server');
    assert.equal(languageServerSpec('brainfuck'), null);
});

test('TypeScript server gets the JSX-aware language id so .tsx/.jsx files parse JSX', () => {
    assert.equal(languageServerSpec('typescript').id, 'typescript');
    assert.equal(languageServerSpec('javascript').id, 'typescript');
    assert.equal(typeScriptLanguageId('src/App.tsx'), 'typescriptreact');
    assert.equal(typeScriptLanguageId('src/App.jsx'), 'javascriptreact');
    assert.equal(typeScriptLanguageId('electron/main.cjs'), 'javascript');
    assert.equal(typeScriptLanguageId('src/lib/a.ts'), 'typescript');
});

test('HTML and CSS share one install so a single click enables both', () => {
    assert.equal(languageServerSpec('html').installId, 'web');
    assert.equal(languageServerSpec('scss').installId, 'web');
    assert.equal(languageServerSpec('scss').languageId, 'scss');
    assert.equal(languageServerSpec('html').initializationOptions.provideFormatter, true);
    assert.notEqual(languageServerSpec('html').id, languageServerSpec('css').id);
});

test('requests a server never advertised are skipped, so the lightbulb does not spam method-not-found errors', () => {
    const capabilities = { hoverProvider: true, definitionProvider: { workDoneProgress: true }, codeActionProvider: false };
    assert.equal(serverSupports(capabilities, 'hover'), true);
    assert.equal(serverSupports(capabilities, 'definition'), true);
    assert.equal(serverSupports(capabilities, 'codeAction'), false);
    assert.equal(serverSupports(capabilities, 'rename'), false);
    assert.equal(serverSupports(undefined, 'rename'), true);
});

test('completion resolve and capability probes follow what the server advertised', () => {
    assert.equal(serverSupports({ completionProvider: {} }, 'completionResolve'), false);
    assert.equal(serverSupports({ completionProvider: { resolveProvider: true } }, 'completionResolve'), true);
    assert.equal(serverSupports({}, 'serverCapabilities'), true);
    assert.equal(serverSupports({ inlayHintProvider: true }, 'inlayHint'), true);
    assert.equal(serverSupports({}, 'semanticTokens'), false);
});

test('incremental sync sends only the edited range, and applying it reproduces the new text exactly', () => {
    const { incrementalChange } = require('./main/lsp-session.cjs');
    const apply = (text, change) => {
        const offset = ({ line, character }) => text.split('\n').slice(0, line).reduce((sum, part) => sum + part.length + 1, 0) + character;
        return text.slice(0, offset(change.range.start)) + change.text + text.slice(offset(change.range.end));
    };
    const cases = [
        ['fn main() {\n    let a = 1;\n}\n', 'fn main() {\n    let ab = 1;\n}\n'],
        ['a\nb\nc', 'a\nc'],
        ['', 'hello'],
        ['x 😀 y', 'x 😃 y'],
        ['same', 'same!'],
    ];
    for (const [before, after] of cases) assert.equal(apply(before, incrementalChange(before, after)), after);
    assert.deepEqual(incrementalChange('a\nbc\n', 'a\nbXc\n'), { range: { start: { line: 1, character: 1 }, end: { line: 1, character: 1 } }, text: 'X' });
});

test('a user lsp.json entry overrides or adds a language server, and broken entries are ignored', () => {
    const fs = require('node:fs');
    const os = require('node:os');
    const path = require('node:path');
    const { userLanguageServerSpec } = require('./main/lsp-registry.cjs');
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'zorai-lsp-'));
    const file = path.join(dir, 'lsp.json');
    fs.writeFileSync(file, JSON.stringify({ servers: { elixir: { command: 'elixir-ls', args: ['--stdio'] }, rust: { command: '' } } }));
    assert.deepEqual(userLanguageServerSpec('Elixir', file).candidates, [{ command: 'elixir-ls', args: ['--stdio'] }]);
    assert.equal(userLanguageServerSpec('rust', file), null);
    assert.equal(userLanguageServerSpec('go', path.join(dir, 'missing.json')), null);
    fs.rmSync(dir, { recursive: true, force: true });
});
