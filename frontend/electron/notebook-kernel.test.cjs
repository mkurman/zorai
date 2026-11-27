const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { createNotebookKernels } = require('./main/notebook-kernel.cjs');

test('the notebook kernel keeps state between cells, shows the last expression and reports errors', { timeout: 30000 }, async () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'zorai-nb-'));
    const kernels = createNotebookKernels();
    try {
        const first = await kernels.execute(root, 'a.ipynb', 'x = 20\nprint("hello")\nx + 1');
        assert.equal(first.stdout, 'hello\n');
        assert.equal(first.result, '21');
        const second = await kernels.execute(root, 'a.ipynb', 'x * 2');
        assert.equal(second.result, '40');
        const failed = await kernels.execute(root, 'a.ipynb', '1 / 0');
        assert.equal(failed.error.ename, 'ZeroDivisionError');
        const other = await kernels.execute(root, 'b.ipynb', "'x' in globals()");
        assert.equal(other.result, 'False');
        kernels.restart(root, 'a.ipynb');
        assert.equal((await kernels.execute(root, 'a.ipynb', "'x' in globals()")).result, 'False');
    } finally {
        kernels.stopAll();
        fs.rmSync(root, { recursive: true, force: true });
    }
});

const { interpreterCandidates } = require('./main/notebook-kernel.cjs');
const JUPYTER_PYTHON = process.env.ZORAI_TEST_JUPYTER_PYTHON || '/mnt/e/gitlab/ai/grpo-llm-evaluator/.venv/bin/python';

test('interpreter discovery lists workspace venvs first, then conda and pyenv envs, without duplicates', { skip: process.platform === 'win32' }, () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'zorai-nb-root-'));
    const home = fs.mkdtempSync(path.join(os.tmpdir(), 'zorai-nb-home-'));
    const touch = (file) => { fs.mkdirSync(path.dirname(file), { recursive: true }); fs.writeFileSync(file, ''); };
    try {
        touch(path.join(root, '.venv', 'bin', 'python'));
        touch(path.join(home, 'miniconda3', 'envs', 'torch', 'bin', 'python'));
        touch(path.join(home, '.pyenv', 'versions', '3.12.1', 'bin', 'python'));
        const found = interpreterCandidates(root, { PATH: '' }, home);
        assert.deepEqual(found.map((item) => item.label), ['.venv (workspace)', 'conda: torch', 'pyenv: 3.12.1']);
        assert.equal(found[0].path, path.join(root, '.venv', 'bin', 'python'));
    } finally {
        fs.rmSync(root, { recursive: true, force: true });
        fs.rmSync(home, { recursive: true, force: true });
    }
});

test('an interpreter with ipykernel runs real Jupyter: magics, shell commands, streamed output and interrupt', { timeout: 120000, skip: !fs.existsSync(JUPYTER_PYTHON) && 'no interpreter with ipykernel available' }, async () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'zorai-nb-'));
    const kernels = createNotebookKernels();
    try {
        const streamed = [];
        const first = await kernels.execute(root, 'a.ipynb', 'for i in range(2):\n    print(i)\nx = 5\nx * 2', { python: JUPYTER_PYTHON, onOutput: (output) => streamed.push(output.output_type) });
        assert.equal(first.mode, 'jupyter');
        assert.ok(streamed.includes('stream'));
        assert.deepEqual(first.outputs.find((output) => output.output_type === 'execute_result').data['text/plain'], '10');
        const magic = await kernels.execute(root, 'a.ipynb', '!echo shell-ok\n%time x', { python: JUPYTER_PYTHON });
        assert.match(magic.outputs.map((output) => output.text ?? '').join(''), /shell-ok/);
        const slow = kernels.execute(root, 'a.ipynb', 'import time\ntime.sleep(30)', { python: JUPYTER_PYTHON });
        await new Promise((resolve) => setTimeout(resolve, 1500));
        assert.equal(kernels.interrupt(root, 'a.ipynb'), true);
        const interrupted = await slow;
        assert.equal(interrupted.outputs.at(-1).ename, 'KeyboardInterrupt');
        assert.equal((await kernels.execute(root, 'a.ipynb', 'x', { python: JUPYTER_PYTHON })).outputs[0].data['text/plain'], '5');
        assert.deepEqual(fs.readdirSync(root).filter((name) => name.startsWith('kernel-')), []);
    } finally {
        kernels.stopAll();
        fs.rmSync(root, { recursive: true, force: true });
    }
});

test('basic mode completes names and attributes from the live namespace', { timeout: 30000 }, async () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'zorai-nb-'));
    const kernels = createNotebookKernels();
    try {
        await kernels.execute(root, 'c.ipynb', 'import json\nmy_value = 1', { python: 'python3' });
        const attribute = await kernels.complete(root, 'c.ipynb', 'json.du', 7, 'python3');
        assert.ok(attribute.matches.includes('json.dumps'));
        assert.equal(attribute.cursor_start, 0);
        const name = await kernels.complete(root, 'c.ipynb', 'x = my_v', 8, 'python3');
        assert.deepEqual(name.matches, ['my_value']);
    } finally {
        kernels.stopAll();
        fs.rmSync(root, { recursive: true, force: true });
    }
});

test('jupyter mode completes members of imported modules with their kinds', { timeout: 120000, skip: !fs.existsSync(JUPYTER_PYTHON) && 'no interpreter with ipykernel available' }, async () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'zorai-nb-'));
    const kernels = createNotebookKernels();
    try {
        await kernels.execute(root, 'c.ipynb', 'import json', { python: JUPYTER_PYTHON });
        const result = await kernels.complete(root, 'c.ipynb', 'import json\njson.du', 19, JUPYTER_PYTHON);
        const index = result.matches.findIndex((match) => match.endsWith('dumps'));
        assert.ok(index >= 0, JSON.stringify(result));
        const code = 'import json\njson.du';
        assert.equal(code.slice(0, result.cursor_start) + result.matches[index] + code.slice(result.cursor_end), 'import json\njson.dumps');
        assert.equal(result.types[index], 'function');
    } finally {
        kernels.stopAll();
        fs.rmSync(root, { recursive: true, force: true });
    }
});

test('the kernel runs in the notebook\'s own folder so relative paths like ../data resolve as in Jupyter', { timeout: 30000 }, async () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'zorai-nb-'));
    fs.mkdirSync(path.join(root, 'notebooks'));
    const kernels = createNotebookKernels();
    try {
        const nested = await kernels.execute(root, 'notebooks/a.ipynb', 'import os\nos.getcwd()', { python: 'python3' });
        assert.equal(nested.result, `'${fs.realpathSync(path.join(root, 'notebooks'))}'`);
        const missing = await kernels.execute(root, 'gone/b.ipynb', 'import os\nos.getcwd()', { python: 'python3' });
        assert.equal(missing.result, `'${fs.realpathSync(root)}'`);
    } finally {
        kernels.stopAll();
        fs.rmSync(root, { recursive: true, force: true });
    }
});
