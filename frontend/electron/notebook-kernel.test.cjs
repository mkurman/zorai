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
