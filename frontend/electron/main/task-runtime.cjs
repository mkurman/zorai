const fs = require('fs');
const path = require('path');
const { spawn } = require('child_process');
const { stripJsonComments, substitute } = require('./dap-adapters.cjs');

const MAX_OUTPUT_CHARS = 2 * 1024 * 1024;

function readText(file) {
    try { return fs.readFileSync(file, 'utf8'); } catch { return null; }
}

function packageManager(root) {
    if (fs.existsSync(path.join(root, 'pnpm-lock.yaml'))) return 'pnpm';
    if (fs.existsSync(path.join(root, 'yarn.lock'))) return 'yarn';
    if (fs.existsSync(path.join(root, 'bun.lockb')) || fs.existsSync(path.join(root, 'bun.lock'))) return 'bun';
    return 'npm';
}

function groupFor(name) {
    if (/^(build|compile)(:|$)/.test(name)) return 'build';
    if (/^test(:|$)/.test(name)) return 'test';
    return null;
}

function configuredTasks(root) {
    const tasks = [];
    for (const file of ['.zorai/tasks.json', '.vscode/tasks.json']) {
        const text = readText(path.join(root, file));
        if (!text) continue;
        let parsed;
        try { parsed = JSON.parse(stripJsonComments(text)); } catch { continue; }
        for (const task of Array.isArray(parsed?.tasks) ? parsed.tasks : []) {
            if (typeof task?.command !== 'string' && !Array.isArray(task?.dependsOn)) continue;
            const group = typeof task.group === 'string' ? task.group : task.group?.kind ?? null;
            tasks.push({
                id: `${file}:${task.label ?? task.command}`,
                label: String(task.label ?? task.command),
                command: task.command ?? '',
                args: Array.isArray(task.args) ? task.args.map(String) : [],
                cwd: task.options?.cwd ?? null,
                env: task.options?.env ?? null,
                shell: task.type !== 'process',
                group,
                isDefault: typeof task.group === 'object' && task.group?.isDefault === true,
                dependsOn: Array.isArray(task.dependsOn) ? task.dependsOn.map(String) : typeof task.dependsOn === 'string' ? [task.dependsOn] : [],
                source: file,
            });
        }
    }
    return tasks;
}

function detectedTasks(root) {
    const tasks = [];
    const add = (source, label, command, group = null) => tasks.push({ id: `${source}:${label}`, label, command, args: [], cwd: null, env: null, shell: true, group, isDefault: false, dependsOn: [], source });
    const pkg = readText(path.join(root, 'package.json'));
    if (pkg) {
        try {
            const manager = packageManager(root);
            for (const name of Object.keys(JSON.parse(pkg).scripts ?? {})) add('package.json', `${manager}: ${name}`, `${manager} run ${name}`, groupFor(name));
        } catch {}
    }
    const cargo = readText(path.join(root, 'Cargo.toml'));
    if (cargo) {
        add('Cargo.toml', 'cargo: build', 'cargo build', 'build');
        add('Cargo.toml', 'cargo: check', 'cargo check', 'build');
        add('Cargo.toml', 'cargo: test', 'cargo test', 'test');
        add('Cargo.toml', 'cargo: clippy', 'cargo clippy');
        if (/^\s*\[\[bin\]\]|^\s*\[package\]/m.test(cargo)) add('Cargo.toml', 'cargo: run', 'cargo run');
        for (const match of cargo.matchAll(/^\s*\[\[example\]\][^[]*?name\s*=\s*"([^"]+)"/gm)) add('Cargo.toml', `cargo: example ${match[1]}`, `cargo run --example ${match[1]}`);
    }
    const makefile = readText(path.join(root, 'Makefile')) ?? readText(path.join(root, 'makefile'));
    if (makefile) {
        const targets = new Set();
        for (const match of makefile.matchAll(/^([A-Za-z0-9][\w.-]*)\s*:(?!=)/gm)) if (match[1] !== '.PHONY') targets.add(match[1]);
        for (const target of targets) add('Makefile', `make: ${target}`, `make ${target}`, groupFor(target));
    }
    const pyproject = readText(path.join(root, 'pyproject.toml'));
    if (pyproject) {
        const section = /^\[(?:project\.scripts|tool\.poetry\.scripts)\]\s*$([\s\S]*?)(?=^\[|(?![\s\S]))/m.exec(pyproject)?.[1] ?? '';
        for (const match of section.matchAll(/^\s*([\w-]+)\s*=/gm)) add('pyproject.toml', `script: ${match[1]}`, match[1]);
        if (/pytest/.test(pyproject)) add('pyproject.toml', 'pytest', 'python -m pytest', 'test');
    }
    return tasks;
}

function discoverTasks(root) {
    const configured = configuredTasks(root);
    const labels = new Set(configured.map((task) => task.label));
    return [...configured, ...detectedTasks(root).filter((task) => !labels.has(task.label))];
}

const MATCHERS = [
    { source: 'tsc', pattern: /^(.+?)\((\d+),(\d+)\):\s+(error|warning)\s+(TS\d+):\s+(.*)$/, map: (m) => ({ file: m[1], line: +m[2], column: +m[3], severity: m[4], code: m[5], message: m[6] }) },
    { source: 'gcc', pattern: /^(.+?):(\d+):(\d+):\s+(?:fatal\s+)?(error|warning|note):\s+(.*)$/, map: (m) => ({ file: m[1], line: +m[2], column: +m[3], severity: m[4] === 'note' ? 'info' : m[4], code: null, message: m[5] }) },
    { source: 'python', pattern: /^\s*File "(.+?)", line (\d+)/, map: (m) => ({ file: m[1], line: +m[2], column: 1, severity: 'error', code: null, message: 'Traceback frame' }) },
    { source: 'go', pattern: /^(\S+\.go):(\d+):(\d+):\s+(.*)$/, map: (m) => ({ file: m[1], line: +m[2], column: +m[3], severity: 'error', code: null, message: m[4] }) },
];

function matchProblems(output, root, cwd) {
    const problems = [];
    const lines = output.split(/\r?\n/).map((line) => line.replace(/\u001b\[[0-9;]*m/g, ''));
    const toRelative = (file) => {
        const absolute = path.isAbsolute(file) ? file : path.resolve(cwd, file);
        const relative = path.relative(root, absolute);
        return relative && !relative.startsWith('..') && !path.isAbsolute(relative) ? relative.split(path.sep).join('/') : null;
    };
    for (let index = 0; index < lines.length; index += 1) {
        const rust = /^(error|warning)(?:\[(\w+)\])?:\s+(.*)$/.exec(lines[index]);
        const location = rust ? /^\s*-->\s+(.+?):(\d+):(\d+)/.exec(lines[index + 1] ?? '') : null;
        if (rust && location) {
            const file = toRelative(location[1]);
            if (file) problems.push({ path: file, line: +location[2], column: +location[3], severity: rust[1], code: rust[2] ?? null, message: rust[3], source: 'rustc' });
            continue;
        }
        for (const matcher of MATCHERS) {
            const match = matcher.pattern.exec(lines[index]);
            if (!match) continue;
            const problem = matcher.map(match);
            const file = toRelative(problem.file);
            if (file) problems.push({ path: file, line: problem.line, column: problem.column, severity: problem.severity, code: problem.code, message: problem.message, source: matcher.source });
            break;
        }
    }
    return problems;
}

function createTaskRuntime() {
    const runs = new Map();
    let nextId = 1;
    return {
        discover: (rootPath) => discoverTasks(rootPath),
        run(webContents, rootPath, task) {
            const id = `task-${nextId++}`;
            const resolved = substitute(task, { root: rootPath, file: null });
            const cwd = resolved.cwd ? path.resolve(rootPath, resolved.cwd) : rootPath;
            const command = resolved.shell === false ? resolved.command : [resolved.command, ...resolved.args.map((arg) => (/\s/.test(arg) ? JSON.stringify(arg) : arg))].join(' ');
            const child = resolved.shell === false
                ? spawn(resolved.command, resolved.args, { cwd, env: { ...process.env, ...(resolved.env ?? {}) }, windowsHide: true })
                : spawn(command, { cwd, env: { ...process.env, FORCE_COLOR: '0', ...(resolved.env ?? {}) }, shell: true, windowsHide: true });
            let output = '';
            const send = (payload) => { if (!webContents.isDestroyed()) webContents.send('task-event', { runId: id, ...payload }); };
            const append = (chunk) => {
                const text = chunk.toString('utf8');
                output = `${output}${text}`.slice(-MAX_OUTPUT_CHARS);
                send({ type: 'output', text });
            };
            child.stdout.on('data', append);
            child.stderr.on('data', append);
            child.once('error', (error) => send({ type: 'exit', code: -1, error: error.message, problems: [] }));
            child.once('close', (code) => {
                runs.delete(id);
                send({ type: 'exit', code, problems: matchProblems(output, rootPath, cwd) });
            });
            runs.set(id, child);
            send({ type: 'start', label: resolved.label, command });
            return { runId: id };
        },
        stop(runId) {
            const child = runs.get(runId);
            if (!child) return false;
            child.kill();
            return true;
        },
        stopAll() {
            for (const child of runs.values()) child.kill();
            runs.clear();
        },
    };
}

module.exports = { createTaskRuntime, discoverTasks, matchProblems };
