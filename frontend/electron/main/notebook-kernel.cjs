const fs = require('fs');
const path = require('path');
const os = require('os');
const { execFile, spawn, spawnSync } = require('child_process');

const DRIVER = String.raw`
import ast, base64, contextlib, io, json, os, signal, sys
def emit(message):
    sys.__stdout__.write(json.dumps(message) + "\n")
    sys.__stdout__.flush()
manager = client = None
fallback = None
socket_dir = None
try:
    import queue
    from jupyter_client.manager import KernelManager
    from jupyter_client.kernelspec import KernelSpec
    import ipykernel
    os.environ.pop("MPLBACKEND", None)
    manager = KernelManager()
    if os.name != "nt":
        import tempfile
        socket_dir = tempfile.mkdtemp(prefix="zorai-kernel-")
        manager.transport = "ipc"
        manager.ip = os.path.join(socket_dir, "kernel")
    manager._kernel_spec = KernelSpec(argv=[sys.executable, "-m", "ipykernel_launcher", "-f", "{connection_file}"], display_name="Python", language="python")
    manager.start_kernel()
    client = manager.client()
    client.start_channels()
    client.wait_for_ready(timeout=60)
except ImportError as exception:
    manager = None
    fallback = "Install ipykernel in this interpreter for full Jupyter support (" + str(exception) + ")."
except Exception as exception:
    if manager is not None:
        try:
            manager.shutdown_kernel(now=True)
        except Exception:
            pass
    manager = None
    fallback = "Jupyter kernel failed to start: " + str(exception)

def run_jupyter(request):
    msg_id = client.execute(request["code"], allow_stdin=False)
    count = None
    while True:
        try:
            msg = client.get_iopub_msg(timeout=1)
        except queue.Empty:
            if not manager.is_alive():
                emit({"id": request["id"], "output": {"output_type": "error", "ename": "KernelDied", "evalue": "The kernel stopped while running this cell.", "traceback": []}})
                break
            continue
        if msg["parent_header"].get("msg_id") != msg_id:
            continue
        kind, content = msg["msg_type"], msg["content"]
        if kind == "status" and content.get("execution_state") == "idle":
            break
        output = None
        if kind == "execute_input":
            count = content.get("execution_count")
        elif kind == "stream":
            output = {"output_type": "stream", "name": content["name"], "text": content["text"]}
        elif kind in ("display_data", "execute_result", "update_display_data"):
            output = {"output_type": "display_data" if kind == "update_display_data" else kind, "data": content.get("data", {}), "metadata": content.get("metadata", {})}
            if kind == "execute_result":
                output["execution_count"] = content.get("execution_count")
        elif kind == "error":
            output = {"output_type": "error", "ename": content.get("ename", ""), "evalue": content.get("evalue", ""), "traceback": content.get("traceback", [])}
        elif kind == "clear_output":
            output = {"output_type": "clear_output"}
        if output is not None:
            emit({"id": request["id"], "output": output})
    emit({"id": request["id"], "done": True, "execution_count": count})

def complete_jupyter(request):
    msg_id = client.complete(request["code"], request["cursor"])
    content = {}
    try:
        while True:
            reply = client.get_shell_msg(timeout=10)
            if reply["parent_header"].get("msg_id") == msg_id:
                content = reply["content"]
                break
    except queue.Empty:
        pass
    matches = content.get("matches", [])
    types = {entry.get("text"): entry.get("type") for entry in content.get("metadata", {}).get("_jupyter_types_experimental", [])}
    emit({"id": request["id"], "done": True, "matches": matches, "cursor_start": content.get("cursor_start", request["cursor"]), "cursor_end": content.get("cursor_end", request["cursor"]), "types": [types.get(match) for match in matches]})

def complete_basic(request):
    import re, rlcompleter
    cursor = request["cursor"]
    found = re.search(r"[A-Za-z_][\w.]*$", request["code"][:cursor])
    token = found.group(0) if found else ""
    matches = []
    if token:
        completer = rlcompleter.Completer(namespace)
        for state in range(300):
            try:
                match = completer.complete(token, state)
            except Exception:
                break
            if match is None:
                break
            matches.append(match.rstrip("("))
    emit({"id": request["id"], "done": True, "matches": matches, "cursor_start": cursor - len(token), "cursor_end": cursor, "types": [None] * len(matches)})

namespace = {"__name__": "__main__"}
def run_basic(request):
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
        lines = traceback_lines(exception)
        error = {"ename": type(exception).__name__, "evalue": str(exception), "traceback": lines}
    emit({"id": request["id"], "done": True, "stdout": out.getvalue(), "stderr": err.getvalue(), "result": result, "html": html, "images": images, "error": error})

def traceback_lines(exception):
    import traceback
    lines = traceback.format_exception(type(exception), exception, exception.__traceback__)
    return [entry.rstrip("\n") for entry in lines if "<frozen" not in entry]

signal.signal(signal.SIGINT, (lambda *_: manager.interrupt_kernel()) if manager else signal.default_int_handler)
signal.signal(signal.SIGTERM, lambda *_: sys.exit(0))
emit({"ready": True, "mode": "jupyter" if manager else "basic", "python": sys.executable, "note": fallback})
try:
    while True:
        try:
            line = sys.stdin.readline()
        except KeyboardInterrupt:
            continue
        if not line:
            break
        request = json.loads(line)
        try:
            if request.get("kind") == "complete":
                (complete_jupyter if manager else complete_basic)(request)
            else:
                (run_jupyter if manager else run_basic)(request)
        except KeyboardInterrupt:
            emit({"id": request["id"], "done": True, "stdout": "", "stderr": "", "result": None, "html": None, "images": [], "error": {"ename": "KeyboardInterrupt", "evalue": "", "traceback": []}})
finally:
    if manager is not None:
        client.stop_channels()
        manager.shutdown_kernel(now=True)
    if socket_dir:
        import shutil
        shutil.rmtree(socket_dir, ignore_errors=True)
`;

function workspacePythons(root) {
    return ['.venv', 'venv', 'env'].flatMap((name) => [path.join(root, name, 'bin', 'python'), path.join(root, name, 'Scripts', 'python.exe')]);
}

function pythonFor(root) {
    for (const candidate of workspacePythons(root)) {
        if (fs.existsSync(candidate)) return candidate;
    }
    for (const name of ['python3', 'python']) {
        if (spawnSync(name, ['--version'], { encoding: 'utf8', timeout: 5000 }).status === 0) return name;
    }
    return null;
}

function interpreterCandidates(root, env = process.env, home = os.homedir()) {
    const bin = process.platform === 'win32' ? ['python.exe'] : ['bin/python'];
    const candidates = workspacePythons(root).map((file) => ({ path: file, source: 'workspace', label: `${path.basename(path.dirname(path.dirname(file)))} (workspace)` }));
    for (const variable of ['VIRTUAL_ENV', 'CONDA_PREFIX']) {
        if (env[variable]) candidates.push(...bin.map((tail) => ({ path: path.join(env[variable], tail), source: 'active', label: `${path.basename(env[variable])} (active)` })));
    }
    for (const base of ['miniconda3', 'anaconda3', 'miniforge3', 'mambaforge'].map((name) => path.join(home, name)).concat('/opt/conda')) {
        candidates.push(...bin.map((tail) => ({ path: path.join(base, tail), source: 'conda', label: `conda: base (${path.basename(base)})` })));
        for (const name of safeReaddir(path.join(base, 'envs'))) candidates.push(...bin.map((tail) => ({ path: path.join(base, 'envs', name, tail), source: 'conda', label: `conda: ${name}` })));
    }
    for (const version of safeReaddir(path.join(env.PYENV_ROOT || path.join(home, '.pyenv'), 'versions'))) {
        candidates.push({ path: path.join(env.PYENV_ROOT || path.join(home, '.pyenv'), 'versions', version, 'bin', 'python'), source: 'pyenv', label: `pyenv: ${version}` });
    }
    const seenReal = new Set();
    for (const directory of String(env.PATH || '').split(path.delimiter).filter(Boolean)) {
        for (const name of process.platform === 'win32' ? ['python.exe'] : ['python3', 'python']) {
            const file = path.join(directory, name);
            if (!fs.existsSync(file)) continue;
            let real = file;
            try { real = fs.realpathSync(file); } catch {}
            if (seenReal.has(real)) continue;
            seenReal.add(real);
            candidates.push({ path: file, source: 'path', label: file });
        }
    }
    const seen = new Set();
    return candidates.filter((candidate) => {
        if (seen.has(candidate.path) || !fs.existsSync(candidate.path)) return false;
        seen.add(candidate.path);
        return true;
    });
}

function safeReaddir(directory) {
    try { return fs.readdirSync(directory); } catch { return []; }
}

function pythonVersion(file) {
    return new Promise((resolve) => {
        execFile(file, ['-c', 'import sys, importlib.util as u; print("%d.%d.%d" % sys.version_info[:3], bool(u.find_spec("ipykernel") and u.find_spec("jupyter_client")))'], { timeout: 5000 }, (error, stdout) => {
            if (error) return resolve(null);
            const [version, jupyter] = String(stdout).trim().split(/\s+/);
            resolve({ version, jupyter: jupyter === 'True' });
        });
    });
}

async function listInterpreters(root) {
    const candidates = interpreterCandidates(root);
    const details = await Promise.all(candidates.map((candidate) => pythonVersion(candidate.path)));
    return {
        defaultPython: pythonFor(root),
        interpreters: candidates.flatMap((candidate, index) => (details[index] ? [{ ...candidate, ...details[index] }] : [])),
    };
}

class NotebookKernel {
    constructor(root, requestedPython, cwd = root) {
        const python = requestedPython || pythonFor(root);
        if (!python) throw new Error('Python was not found. Install Python 3 or create a .venv in the workspace.');
        this.python = python;
        this.mode = null;
        this.note = null;
        this.process = spawn(python, ['-u', '-c', DRIVER], { cwd, env: { ...process.env, MPLBACKEND: 'Agg', PYTHONUNBUFFERED: '1' }, windowsHide: true });
        this.pending = new Map();
        this.buffer = '';
        this.nextId = 1;
        this.stderr = '';
        this.process.on('error', (error) => this.fail(`Could not start ${python}: ${error.message}`));
        this.process.stdout.on('data', (chunk) => {
            this.buffer += chunk.toString('utf8');
            let newline = this.buffer.indexOf('\n');
            while (newline !== -1) {
                const line = this.buffer.slice(0, newline);
                this.buffer = this.buffer.slice(newline + 1);
                newline = this.buffer.indexOf('\n');
                let message;
                try { message = JSON.parse(line); } catch { continue; }
                this.receive(message);
            }
        });
        this.process.stderr.on('data', (chunk) => { this.stderr = `${this.stderr}${chunk.toString('utf8')}`.slice(-4000); });
        this.process.once('exit', (code) => {
            this.exited = true;
            this.fail(`The Python kernel stopped (${code ?? 'signal'}). ${this.stderr.trim()}`.trim());
        });
    }

    receive(message) {
        if (message.ready) {
            this.mode = message.mode;
            this.note = message.note || null;
            return;
        }
        const pending = this.pending.get(message.id);
        if (!pending) return;
        if (message.output) {
            if (message.output.output_type === 'clear_output') pending.outputs = [];
            else pending.outputs.push(message.output);
            pending.onOutput?.(message.output);
            return;
        }
        this.pending.delete(message.id);
        pending.resolve({ ...message, ...(this.mode === 'jupyter' ? { outputs: pending.outputs } : {}), mode: this.mode, python: this.python, note: this.note });
    }

    fail(reason) {
        for (const pending of this.pending.values()) pending.reject(new Error(reason));
        this.pending.clear();
    }

    send(payload, onOutput) {
        if (this.exited) return Promise.reject(new Error('The Python kernel has stopped; restart it.'));
        const id = this.nextId++;
        return new Promise((resolve, reject) => {
            this.pending.set(id, { resolve, reject, onOutput, outputs: [] });
            this.process.stdin.write(`${JSON.stringify({ ...payload, id })}\n`);
        });
    }

    execute(code, onOutput) {
        return this.send({ code: String(code ?? '') }, onOutput);
    }

    complete(code, cursor) {
        return this.send({ kind: 'complete', code: String(code ?? ''), cursor: Number(cursor) || 0 });
    }

    interrupt() {
        if (this.exited || this.pending.size === 0) return false;
        if (process.platform === 'win32') return false;
        this.process.kill('SIGINT');
        return true;
    }

    stop() {
        if (!this.exited) this.process.kill();
    }
}

function notebookDirectory(root, notebook) {
    const directory = path.dirname(path.resolve(root, String(notebook || '')));
    return fs.existsSync(directory) ? directory : root;
}

function createNotebookKernels() {
    const kernels = new Map();
    const key = (root, notebook) => `${root}\0${notebook}`;
    const kernelFor = (root, notebook, requestedPython) => {
        const python = requestedPython || null;
        let kernel = kernels.get(key(root, notebook));
        if (kernel && !kernel.exited && kernel.python !== (python || pythonFor(root))) {
            kernel.stop();
            kernel = null;
        }
        if (!kernel || kernel.exited) {
            kernel = new NotebookKernel(root, python, notebookDirectory(root, notebook));
            kernels.set(key(root, notebook), kernel);
        }
        return kernel;
    };
    return {
        async execute(root, notebook, code, options = {}) {
            return kernelFor(root, notebook, options.python).execute(code, options.onOutput);
        },
        async complete(root, notebook, code, cursor, python) {
            const result = await kernelFor(root, notebook, python).complete(code, cursor);
            return { matches: result.matches ?? [], cursor_start: result.cursor_start, cursor_end: result.cursor_end, types: result.types ?? [] };
        },
        interrupt(root, notebook) {
            return kernels.get(key(root, notebook))?.interrupt() ?? false;
        },
        restart(root, notebook) {
            kernels.get(key(root, notebook))?.stop();
            kernels.delete(key(root, notebook));
            return true;
        },
        interpreters: listInterpreters,
        stopAll() {
            for (const kernel of kernels.values()) kernel.stop();
            kernels.clear();
        },
    };
}

module.exports = { createNotebookKernels, interpreterCandidates };
