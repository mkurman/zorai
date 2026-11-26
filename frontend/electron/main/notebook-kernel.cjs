const fs = require('fs');
const path = require('path');
const { spawn, spawnSync } = require('child_process');

const DRIVER = String.raw`
import ast, base64, contextlib, io, json, sys, traceback
namespace = {"__name__": "__main__"}
for line in sys.stdin:
    request = json.loads(line)
    out, err = io.StringIO(), io.StringIO()
    result = html = error = None
    images = []
    try:
        tree = ast.parse(request["code"], mode="exec")
        last = None
        if tree.body and isinstance(tree.body[-1], ast.Expr):
            last = ast.Expression(tree.body.pop().value)
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            exec(compile(tree, "<cell>", "exec"), namespace)
            if last is not None:
                value = eval(compile(last, "<cell>", "eval"), namespace)
                if value is not None:
                    namespace["_"] = value
                    result = repr(value)
                    render = getattr(value, "_repr_html_", None)
                    if callable(render):
                        html = render()
            plt = sys.modules.get("matplotlib.pyplot")
            if plt is not None:
                for number in plt.get_fignums():
                    buffer = io.BytesIO()
                    plt.figure(number).savefig(buffer, format="png", bbox_inches="tight")
                    images.append(base64.b64encode(buffer.getvalue()).decode())
                plt.close("all")
    except BaseException as exception:
        lines = traceback.format_exception(type(exception), exception, exception.__traceback__)
        error = {"ename": type(exception).__name__, "evalue": str(exception), "traceback": [entry.rstrip("\n") for entry in lines if "<frozen" not in entry]}
    sys.__stdout__.write(json.dumps({"id": request["id"], "stdout": out.getvalue(), "stderr": err.getvalue(), "result": result, "html": html, "images": images, "error": error}) + "\n")
    sys.__stdout__.flush()
`;

function pythonFor(root) {
    for (const candidate of [path.join(root, '.venv', 'bin', 'python'), path.join(root, 'venv', 'bin', 'python'), path.join(root, '.venv', 'Scripts', 'python.exe')]) {
        if (fs.existsSync(candidate)) return candidate;
    }
    for (const name of ['python3', 'python']) {
        if (spawnSync(name, ['--version'], { encoding: 'utf8', timeout: 5000 }).status === 0) return name;
    }
    return null;
}

class NotebookKernel {
    constructor(root) {
        const python = pythonFor(root);
        if (!python) throw new Error('Python was not found. Install Python 3 or create a .venv in the workspace.');
        this.python = python;
        this.process = spawn(python, ['-u', '-c', DRIVER], { cwd: root, env: { ...process.env, MPLBACKEND: 'Agg', PYTHONUNBUFFERED: '1' }, windowsHide: true });
        this.pending = new Map();
        this.buffer = '';
        this.nextId = 1;
        this.stderr = '';
        this.process.stdout.on('data', (chunk) => {
            this.buffer += chunk.toString('utf8');
            let newline = this.buffer.indexOf('\n');
            while (newline !== -1) {
                const line = this.buffer.slice(0, newline);
                this.buffer = this.buffer.slice(newline + 1);
                newline = this.buffer.indexOf('\n');
                let message;
                try { message = JSON.parse(line); } catch { continue; }
                this.pending.get(message.id)?.resolve(message);
                this.pending.delete(message.id);
            }
        });
        this.process.stderr.on('data', (chunk) => { this.stderr = `${this.stderr}${chunk.toString('utf8')}`.slice(-4000); });
        this.process.once('exit', (code) => {
            for (const pending of this.pending.values()) pending.reject(new Error(`The Python kernel stopped (${code ?? 'signal'}). ${this.stderr.trim()}`.trim()));
            this.pending.clear();
            this.exited = true;
        });
    }

    execute(code) {
        if (this.exited) return Promise.reject(new Error('The Python kernel has stopped; restart it.'));
        const id = this.nextId++;
        return new Promise((resolve, reject) => {
            this.pending.set(id, { resolve, reject });
            this.process.stdin.write(`${JSON.stringify({ id, code: String(code ?? '') })}\n`);
        });
    }

    stop() {
        if (!this.exited) this.process.kill();
    }
}

function createNotebookKernels() {
    const kernels = new Map();
    const key = (root, notebook) => `${root}\0${notebook}`;
    return {
        async execute(root, notebook, code) {
            let kernel = kernels.get(key(root, notebook));
            if (!kernel || kernel.exited) {
                kernel = new NotebookKernel(root);
                kernels.set(key(root, notebook), kernel);
            }
            return kernel.execute(code);
        },
        restart(root, notebook) {
            kernels.get(key(root, notebook))?.stop();
            kernels.delete(key(root, notebook));
            return true;
        },
        stopAll() {
            for (const kernel of kernels.values()) kernel.stop();
            kernels.clear();
        },
    };
}

module.exports = { createNotebookKernels };
