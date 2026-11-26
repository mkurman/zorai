const fs = require('fs');
const path = require('path');
const { stripJsonComments } = require('./dap-adapters.cjs');

function parseSnippetFile(text, defaultScope, source) {
    let parsed;
    try { parsed = JSON.parse(stripJsonComments(text)); } catch { return []; }
    if (!parsed || typeof parsed !== 'object') return [];
    return Object.entries(parsed).flatMap(([name, entry]) => {
        if (!entry || typeof entry !== 'object') return [];
        const prefixes = Array.isArray(entry.prefix) ? entry.prefix : [entry.prefix];
        const body = Array.isArray(entry.body) ? entry.body.join('\n') : entry.body;
        if (typeof body !== 'string') return [];
        const scope = typeof entry.scope === 'string' ? entry.scope.split(',').map((value) => value.trim()).filter(Boolean) : defaultScope;
        return prefixes.filter((prefix) => typeof prefix === 'string' && prefix.trim()).map((prefix) => ({
            name, prefix, body, description: typeof entry.description === 'string' ? entry.description : name, scope, source,
        }));
    });
}

function readDir(dir, filter) {
    try { return fs.readdirSync(dir).filter(filter).map((name) => path.join(dir, name)); } catch { return []; }
}

function loadSnippets(root, userDir) {
    const snippets = [];
    for (const file of root ? [...readDir(path.join(root, '.vscode'), (name) => name.endsWith('.code-snippets')), ...readDir(path.join(root, '.zorai', 'snippets'), (name) => name.endsWith('.code-snippets') || name.endsWith('.json'))] : []) {
        const scope = file.endsWith('.json') ? [path.basename(file, '.json')] : [];
        snippets.push(...parseSnippetFile(fs.readFileSync(file, 'utf8'), scope, path.relative(root, file)));
    }
    for (const file of readDir(userDir, (name) => name.endsWith('.json') || name.endsWith('.code-snippets'))) {
        const scope = file.endsWith('.json') ? [path.basename(file, '.json')] : [];
        snippets.push(...parseSnippetFile(fs.readFileSync(file, 'utf8'), scope, `user/${path.basename(file)}`));
    }
    return snippets;
}

module.exports = { loadSnippets, parseSnippetFile };
