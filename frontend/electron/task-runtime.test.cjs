const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { createTaskRuntime, discoverTasks, matchProblems } = require('./main/task-runtime.cjs');

test('tasks come from tasks.json first, then package.json, Cargo.toml and Makefile without duplicates', () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'zorai-tasks-'));
    fs.mkdirSync(path.join(root, '.vscode'));
    fs.writeFileSync(path.join(root, '.vscode', 'tasks.json'), '{ // c\n "tasks": [{ "label": "npm: build", "type": "shell", "command": "npm run build", "group": { "kind": "build", "isDefault": true } }] }');
    fs.writeFileSync(path.join(root, 'package.json'), JSON.stringify({ scripts: { build: 'tsc', test: 'vitest', lint: 'eslint .' } }));
    fs.writeFileSync(path.join(root, 'Cargo.toml'), '[package]\nname = "x"\n');
    fs.writeFileSync(path.join(root, 'Makefile'), '.PHONY: all\nall: build\nbuild:\n\tcc x.c\nFOO := 1\n');
    const tasks = discoverTasks(root);
    const labels = tasks.map((task) => task.label);
    assert.equal(labels.filter((label) => label === 'npm: build').length, 1);
    assert.equal(tasks.find((task) => task.label === 'npm: build').isDefault, true);
    assert.ok(labels.includes('npm: test') && labels.includes('cargo: test') && labels.includes('make: all') && labels.includes('make: build'));
    assert.ok(!labels.includes('make: FOO'));
    assert.equal(tasks.find((task) => task.label === 'npm: test').group, 'test');
    fs.rmSync(root, { recursive: true, force: true });
});

test('build output turns into clickable workspace problems for rustc, tsc and gcc', () => {
    const output = [
        'error[E0425]: cannot find value `x` in this scope',
        '  --> src/main.rs:4:13',
        'src/app.ts(10,5): error TS2322: Type string is not assignable to type number.',
        '\u001b[1mmain.c:3:1: warning: implicit declaration\u001b[0m',
        '/usr/include/stdio.h:1:1: error: outside the workspace',
    ].join('\n');
    const problems = matchProblems(output, '/repo', '/repo');
    assert.deepEqual(problems.map((problem) => [problem.source, problem.path, problem.line, problem.column, problem.severity]), [
        ['rustc', 'src/main.rs', 4, 13, 'error'],
        ['tsc', 'src/app.ts', 10, 5, 'error'],
        ['gcc', 'main.c', 3, 1, 'warning'],
    ]);
});

test('running a task streams output and reports its exit code and problems', async () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'zorai-run-'));
    const events = [];
    const done = new Promise((resolve) => {
        const runtime = createTaskRuntime();
        runtime.run({ isDestroyed: () => false, send: (_channel, payload) => { events.push(payload); if (payload.type === 'exit') resolve(); } }, root, { label: 'echo', command: 'echo "a.c:2:3: error: boom" && exit 3', args: [], shell: true });
    });
    await done;
    assert.equal(events[0].type, 'start');
    assert.ok(events.some((event) => event.type === 'output' && event.text.includes('boom')));
    const exit = events.find((event) => event.type === 'exit');
    assert.equal(exit.code, 3);
    assert.deepEqual(exit.problems.map((problem) => [problem.path, problem.line]), [['a.c', 2]]);
    fs.rmSync(root, { recursive: true, force: true });
});
