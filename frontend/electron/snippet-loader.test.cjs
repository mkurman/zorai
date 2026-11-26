const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { loadSnippets } = require('./main/snippet-loader.cjs');

test('workspace .code-snippets and per-language user snippets load with their scopes', () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'zorai-snip-'));
    const user = fs.mkdtempSync(path.join(os.tmpdir(), 'zorai-snip-user-'));
    fs.mkdirSync(path.join(root, '.vscode'));
    fs.writeFileSync(path.join(root, '.vscode', 'team.code-snippets'), '{ // c\n "Log": { "scope": "javascript,typescript", "prefix": ["log", "cl"], "body": ["console.log($1);", "$0"] }, }');
    fs.writeFileSync(path.join(user, 'rust.json'), '{ "Test": { "prefix": "tst", "body": "#[test]\\nfn ${1:name}() {}" }, "Broken": { "prefix": "x" } }');
    const snippets = loadSnippets(root, user);
    assert.deepEqual(snippets.map((snippet) => [snippet.prefix, snippet.scope.join('|'), snippet.source]), [
        ['log', 'javascript|typescript', path.join('.vscode', 'team.code-snippets')],
        ['cl', 'javascript|typescript', path.join('.vscode', 'team.code-snippets')],
        ['tst', 'rust', 'user/rust.json'],
    ]);
    assert.equal(snippets[0].body, 'console.log($1);\n$0');
    fs.rmSync(root, { recursive: true, force: true });
    fs.rmSync(user, { recursive: true, force: true });
});
