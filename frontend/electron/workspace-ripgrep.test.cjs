const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { ripgrepArgs, ripgrepCommand, ripgrepMatchesFromEvent, ripgrepSearch } = require('./main/workspace-ripgrep.cjs');

test('ripgrep flags mirror the search options so both engines agree on what matches', () => {
    const args = ripgrepArgs('a.b', { caseSensitive: false, wholeWord: true, include: 'src/**, *.ts', exclude: 'gen/**' });
    assert.ok(args.includes('--fixed-strings') && args.includes('--ignore-case') && args.includes('--word-regexp'));
    assert.ok(args.includes('src/**') && args.includes('*.ts') && args.includes('!gen/**') && args.includes('!**/node_modules/**'));
    assert.ok(ripgrepArgs('x', { regex: true, caseSensitive: true }).every((arg) => arg !== '--fixed-strings' && arg !== '--ignore-case'));
    assert.ok(ripgrepArgs('a\nb', {}).includes('--multiline'));
});

test('ripgrep byte offsets become editor columns even after multi-byte characters', () => {
    const [hit] = ripgrepMatchesFromEvent({ type: 'match', data: { path: { text: './src/a.ts' }, lines: { text: '  ź = needle;\n' }, line_number: 3, submatches: [{ match: { text: 'needle' }, start: 7, end: 13 }] } });
    assert.equal(hit.path, 'src/a.ts');
    assert.equal(hit.line, 3);
    assert.equal(hit.column, 7);
    assert.equal(hit.preview.slice(hit.matchStart, hit.matchStart + hit.matchLength), 'needle');
});

test('ripgrep search returns hits from a real workspace when rg is installed', { skip: !ripgrepCommand() }, async () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'zorai-rg-'));
    fs.writeFileSync(path.join(root, 'a.txt'), 'one needle\ntwo\n');
    fs.mkdirSync(path.join(root, 'node_modules'));
    fs.writeFileSync(path.join(root, 'node_modules', 'b.txt'), 'needle\n');
    const batches = [];
    const hits = await ripgrepSearch(root, 'NEEDLE', { maxResults: 10 }, (batch) => batches.push(batch));
    assert.deepEqual(hits.map((hit) => [hit.path, hit.line, hit.column]), [['a.txt', 1, 5]]);
    assert.equal(batches.flat().length, 1);
    fs.rmSync(root, { recursive: true, force: true });
});
