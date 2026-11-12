const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const {
    listWorkspaceDirectory,
    parseGitWorktreeList,
    parseUnifiedDiffHunks,
    readWorkspaceFile,
    resolveWorkspacePath,
    searchWorkspace,
    workspaceGitApplyHunk,
    workspaceGitCommit,
    workspaceGitCreateWorktree,
    workspaceGitDiscard,
    workspaceGitHistory,
    workspaceGitCommitDetail,
    workspaceGitConflicts,
    workspaceGitIntegrateWorktree,
    workspaceGitRemoveWorktree,
    workspaceGitReviewWorktree,
    workspaceGitRemote,
    workspaceGitAction,
    workspaceGitList,
    workspaceGitClone,
    workspaceGitOutput,
    gitActionSteps,
    changesRepository,
    workspaceGitStage,
    workspaceGitUnstage,
    workspaceGitDiff,
    workspaceGitShow,
    writeWorkspaceFile,
} = require('./main/workspace-service.cjs');

function tempWorkspace() {
    return fs.mkdtempSync(path.join(os.tmpdir(), 'zorai-workspace-'));
}

test('workspace paths cannot escape their attached root', () => {
    const root = tempWorkspace();
    assert.throws(() => resolveWorkspacePath(root, '../outside.txt'), { code: 'WORKSPACE_PATH_ESCAPE' });
    fs.rmSync(root, { recursive: true, force: true });
});

test('workspace paths reject symlinks escaping the attached root', () => {
    const root = tempWorkspace();
    const outside = tempWorkspace();
    fs.writeFileSync(path.join(outside, 'secret.txt'), 'secret');
    fs.symlinkSync(outside, path.join(root, 'outside'));
    assert.throws(() => resolveWorkspacePath(root, 'outside/secret.txt'), { code: 'WORKSPACE_SYMLINK_ESCAPE' });
    fs.rmSync(root, { recursive: true, force: true });
    fs.rmSync(outside, { recursive: true, force: true });
});

test('workspace writes are atomic and reject stale editor hashes', async () => {
    const root = tempWorkspace();
    fs.writeFileSync(path.join(root, 'note.txt'), 'first');
    const opened = await readWorkspaceFile(root, 'note.txt');
    const saved = await writeWorkspaceFile(root, 'note.txt', 'second', opened.hash);
    assert.equal(saved.content, 'second');
    assert.notEqual(saved.hash, opened.hash);
    await assert.rejects(
        writeWorkspaceFile(root, 'note.txt', 'stale overwrite', opened.hash),
        { code: 'WORKSPACE_WRITE_CONFLICT' },
    );
    assert.equal(fs.readFileSync(path.join(root, 'note.txt'), 'utf8'), 'second');
    fs.rmSync(root, { recursive: true, force: true });
});

test('workspace tree is lazy, sorted, and hides heavy directories by default', async () => {
    const root = tempWorkspace();
    fs.mkdirSync(path.join(root, 'src'));
    fs.mkdirSync(path.join(root, 'node_modules'));
    fs.writeFileSync(path.join(root, 'z.txt'), 'z');
    const entries = await listWorkspaceDirectory(root, '');
    assert.deepEqual(entries.map((entry) => entry.name), ['src', 'z.txt']);
    fs.rmSync(root, { recursive: true, force: true });
});

test('workspace search is bounded, ignores heavy directories, and reports locations', async () => {
    const root = tempWorkspace();
    fs.mkdirSync(path.join(root, 'src'));
    fs.mkdirSync(path.join(root, 'node_modules'));
    fs.writeFileSync(path.join(root, 'src', 'main.ts'), 'first line\nconst workspaceNeedle = true;\n');
    fs.writeFileSync(path.join(root, 'node_modules', 'ignored.js'), 'workspaceNeedle');
    const results = await searchWorkspace(root, 'workspaceNeedle', { maxResults: 10 });
    assert.deepEqual(results, [{
        path: path.join('src', 'main.ts'),
        line: 2,
        column: 7,
        preview: 'const workspaceNeedle = true;',
    }]);
    fs.rmSync(root, { recursive: true, force: true });
});

test('workspace file language IDs match bundled Monaco tokenizers', async () => {
    const root = tempWorkspace();
    const cases = [
        ['app.tsx', 'typescript'], ['app.jsx', 'javascript'], ['theme.css', 'css'], ['page.html', 'html'],
        ['settings.jsonc', 'json'], ['schema.proto', 'proto'], ['main.tf', 'hcl'], ['Cargo.toml', 'ini'],
        ['component.vue', 'html'], ['component.svelte', 'html'], ['script.groovy', 'java'], ['main.zig', 'cpp'], ['Dockerfile', 'dockerfile'],
        ['Containerfile', 'dockerfile'], ['GNUmakefile', 'shell'], ['.prettierrc', 'json'], ['tsconfig.json', 'json'],
    ];
    for (const [name, expected] of cases) {
        fs.writeFileSync(path.join(root, name), 'sample\n');
        const opened = await readWorkspaceFile(root, name);
        assert.equal(opened.language, expected, name);
    }
    fs.rmSync(root, { recursive: true, force: true });
});

test('workspace git operations stage, unstage, and discard tracked changes', async () => {
    const root = tempWorkspace();
    const runGit = (...args) => require('node:child_process').execFileSync('git', args, { cwd: root, encoding: 'utf8' });
    runGit('init');
    runGit('config', 'user.email', 'workspace-test@zorai.local');
    runGit('config', 'user.name', 'Workspace Test');
    fs.writeFileSync(path.join(root, 'tracked.txt'), 'base\n');
    runGit('add', 'tracked.txt');
    runGit('commit', '-m', 'base');
    fs.writeFileSync(path.join(root, 'tracked.txt'), 'modified\n');

    let status = await workspaceGitStage(root, 'tracked.txt');
    assert.equal(status[0].indexStatus, 'M');
    status = await workspaceGitUnstage(root, 'tracked.txt');
    assert.equal(status[0].worktreeStatus, 'M');
    status = await workspaceGitDiscard(root, 'tracked.txt');
    assert.deepEqual(status, []);
    assert.equal(fs.readFileSync(path.join(root, 'tracked.txt'), 'utf8'), 'base\n');
    fs.rmSync(root, { recursive: true, force: true });
});

test('workspace git discard refuses untracked deletion', async () => {
    const root = tempWorkspace();
    require('node:child_process').execFileSync('git', ['init'], { cwd: root });
    fs.writeFileSync(path.join(root, 'untracked.txt'), 'keep');
    await assert.rejects(workspaceGitDiscard(root, 'untracked.txt'), { code: 'WORKSPACE_DISCARD_UNTRACKED' });
    assert.equal(fs.readFileSync(path.join(root, 'untracked.txt'), 'utf8'), 'keep');
    fs.rmSync(root, { recursive: true, force: true });
});

test('a freshly initialised repository shows as a repository with empty history, not an error', async () => {
    const root = tempWorkspace();
    require('node:child_process').execFileSync('git', ['init', '-b', 'main'], { cwd: root });
    assert.deepEqual(await workspaceGitHistory(root, { limit: 50 }), []);
    assert.deepEqual(await workspaceGitHistory(root, { graph: true }), []);
    fs.rmSync(root, { recursive: true, force: true });
});

test('gitignored entries are flagged so Explorer can mute them, and nothing is flagged outside a repository', async () => {
    const root = tempWorkspace();
    fs.mkdirSync(path.join(root, 'generated'));
    fs.mkdirSync(path.join(root, 'src'));
    fs.writeFileSync(path.join(root, '.gitignore'), 'generated/\n*.log\n');
    fs.writeFileSync(path.join(root, 'debug.log'), 'x');
    let flagged = Object.fromEntries((await listWorkspaceDirectory(root, '')).map((entry) => [entry.name, entry.isGitIgnored]));
    assert.deepEqual(flagged, { generated: false, src: false, '.gitignore': false, 'debug.log': false });
    require('node:child_process').execFileSync('git', ['init', '-b', 'main'], { cwd: root });
    flagged = Object.fromEntries((await listWorkspaceDirectory(root, '')).map((entry) => [entry.name, entry.isGitIgnored]));
    assert.deepEqual(flagged, { generated: true, src: false, '.gitignore': false, 'debug.log': true });
    fs.rmSync(root, { recursive: true, force: true });
});

test('staging many paths skips ones a newer .gitignore excludes instead of failing the whole batch', async () => {
    const root = tempWorkspace();
    const runGit = (...args) => require('node:child_process').execFileSync('git', args, { cwd: root, encoding: 'utf8' });
    runGit('init', '-b', 'main');
    fs.mkdirSync(path.join(root, 'logs'));
    fs.writeFileSync(path.join(root, 'logs', 'run.log'), 'x');
    fs.writeFileSync(path.join(root, 'keep.txt'), 'x');
    fs.writeFileSync(path.join(root, '.gitignore'), 'logs/\n');
    const status = await require('./main/workspace-service.cjs').workspaceGitStageMany(root, ['logs/run.log', 'keep.txt', '.gitignore']);
    assert.deepEqual(runGit('diff', '--cached', '--name-only').trim().split('\n').sort(), ['.gitignore', 'keep.txt']);
    assert.ok(!status.some((entry) => entry.path.startsWith('logs/')));
    fs.rmSync(root, { recursive: true, force: true });
});

test('unified diff parser returns stable structured hunks', () => {
    const diff = [
        'diff --git a/file.txt b/file.txt',
        'index 1234567..7654321 100644',
        '--- a/file.txt',
        '+++ b/file.txt',
        '@@ -1,2 +1,2 @@ first',
        '-old one',
        '+new one',
        ' context',
        '@@ -10 +10,2 @@ second',
        ' line',
        '+added',
        '',
    ].join('\n');
    const hunks = parseUnifiedDiffHunks(diff, 'file.txt', false);
    assert.equal(hunks.length, 2);
    assert.equal(hunks[0].section, 'first');
    assert.equal(hunks[0].additions, 1);
    assert.equal(hunks[0].deletions, 1);
    assert.equal(hunks[1].newLines, 2);
    assert.equal(hunks[0].id.length, 64);
});

test('workspace git hunk actions stage, unstage, and discard only selected hunks', async () => {
    const root = tempWorkspace();
    const runGit = (...args) => require('node:child_process').execFileSync('git', args, { cwd: root, encoding: 'utf8' });
    runGit('init', '-b', 'main');
    runGit('config', 'user.email', 'workspace-test@zorai.local');
    runGit('config', 'user.name', 'Workspace Test');
    const base = Array.from({ length: 20 }, (_, index) => `line ${index + 1}`).join('\n') + '\n';
    fs.writeFileSync(path.join(root, 'file.txt'), base);
    runGit('add', 'file.txt');
    runGit('commit', '-m', 'base');
    const modified = base.replace('line 2', 'line two').replace('line 18', 'line eighteen');
    fs.writeFileSync(path.join(root, 'file.txt'), modified);

    let hunks = parseUnifiedDiffHunks(await require('./main/workspace-service.cjs').workspaceGitDiff(root, 'file.txt'), 'file.txt', false);
    assert.equal(hunks.length, 2);
    let result = await workspaceGitApplyHunk(root, 'file.txt', hunks[0].id, 'stage');
    assert.equal(result.status[0].indexStatus, 'M');
    assert.equal(result.status[0].worktreeStatus, 'M');

    const stagedHunks = parseUnifiedDiffHunks(await require('./main/workspace-service.cjs').workspaceGitDiff(root, 'file.txt', { staged: true }), 'file.txt', true);
    assert.equal(stagedHunks.length, 1);
    result = await workspaceGitApplyHunk(root, 'file.txt', stagedHunks[0].id, 'unstage');
    assert.equal(result.status[0].indexStatus, ' ');

    hunks = parseUnifiedDiffHunks(await require('./main/workspace-service.cjs').workspaceGitDiff(root, 'file.txt'), 'file.txt', false);
    result = await workspaceGitApplyHunk(root, 'file.txt', hunks[0].id, 'discard');
    assert.equal(result.status[0].worktreeStatus, 'M');
    const disk = fs.readFileSync(path.join(root, 'file.txt'), 'utf8');
    assert.ok(disk.includes('line 2') || disk.includes('line 18'));
    assert.ok(disk.includes('line two') || disk.includes('line eighteen'));
    fs.rmSync(root, { recursive: true, force: true });
});

test('stale hunk IDs are rejected instead of applying an arbitrary patch', async () => {
    const root = tempWorkspace();
    const runGit = (...args) => require('node:child_process').execFileSync('git', args, { cwd: root, encoding: 'utf8' });
    runGit('init', '-b', 'main');
    runGit('config', 'user.email', 'workspace-test@zorai.local');
    runGit('config', 'user.name', 'Workspace Test');
    fs.writeFileSync(path.join(root, 'file.txt'), 'base\n');
    runGit('add', 'file.txt');
    runGit('commit', '-m', 'base');
    fs.writeFileSync(path.join(root, 'file.txt'), 'changed\n');
    await assert.rejects(workspaceGitApplyHunk(root, 'file.txt', 'stale-id', 'stage'), { code: 'WORKSPACE_HUNK_STALE' });
    fs.rmSync(root, { recursive: true, force: true });
});

test('workspace commit requires staged changes and creates only a local commit', async () => {
    const root = tempWorkspace();
    const runGit = (...args) => require('node:child_process').execFileSync('git', args, { cwd: root, encoding: 'utf8' });
    runGit('init', '-b', 'main');
    runGit('config', 'user.email', 'workspace-test@zorai.local');
    runGit('config', 'user.name', 'Workspace Test');
    fs.writeFileSync(path.join(root, 'file.txt'), 'base\n');
    runGit('add', 'file.txt');
    runGit('commit', '-m', 'base');
    fs.writeFileSync(path.join(root, 'file.txt'), 'next\n');
    await assert.rejects(workspaceGitCommit(root, 'not staged'), { code: 'WORKSPACE_NOTHING_STAGED' });
    await require('./main/workspace-service.cjs').workspaceGitStage(root, 'file.txt');
    const committed = await workspaceGitCommit(root, 'workspace commit');
    assert.equal(committed.subject, 'workspace commit');
    assert.equal(committed.commit.length, 40);
    assert.deepEqual(committed.status, []);
    assert.equal(runGit('remote').trim(), '');
    fs.rmSync(root, { recursive: true, force: true });
});

test('workspace commit validates the message before invoking git', async () => {
    const root = tempWorkspace();
    require('node:child_process').execFileSync('git', ['init', '-b', 'main'], { cwd: root });
    await assert.rejects(workspaceGitCommit(root, '   '), { code: 'WORKSPACE_COMMIT_MESSAGE_REQUIRED' });
    fs.rmSync(root, { recursive: true, force: true });
});

test('git worktree porcelain parser returns branch and state metadata', () => {
    const parsed = parseGitWorktreeList('worktree /repo\nHEAD abc123\nbranch refs/heads/main\n\nworktree /repo-worktrees/feature\nHEAD def456\ndetached\nlocked reason\n');
    assert.deepEqual(parsed, [
        { path: '/repo', head: 'abc123', branch: 'main', detached: false, bare: false, locked: false, prunable: false },
        { path: '/repo-worktrees/feature', head: 'def456', branch: null, detached: true, bare: false, locked: true, prunable: false },
    ]);
});

test('managed worktree creation and clean removal stay inside sibling container', async () => {
    const parent = tempWorkspace();
    const root = path.join(parent, 'repo');
    fs.mkdirSync(root);
    const runGit = (...args) => require('node:child_process').execFileSync('git', args, { cwd: root, encoding: 'utf8' });
    runGit('init', '-b', 'main');
    runGit('config', 'user.email', 'workspace-test@zorai.local');
    runGit('config', 'user.name', 'Workspace Test');
    fs.writeFileSync(path.join(root, 'file.txt'), 'base\n');
    runGit('add', 'file.txt');
    runGit('commit', '-m', 'base');
    const created = await workspaceGitCreateWorktree(root, { name: 'safe-feature', branch: 'feature/safe', baseRef: 'HEAD' });
    assert.equal(created.root, path.join(parent, 'repo-worktrees', 'safe-feature'));
    assert.equal(fs.existsSync(created.root), true);
    const remaining = await workspaceGitRemoveWorktree(root, created.root);
    assert.equal(remaining.some((entry) => entry.path === created.root), false);
    assert.equal(fs.existsSync(created.root), false);
    fs.rmSync(parent, { recursive: true, force: true });
});

test('managed worktree removal refuses dirty worktrees', async () => {
    const parent = tempWorkspace();
    const root = path.join(parent, 'repo');
    fs.mkdirSync(root);
    const runGit = (...args) => require('node:child_process').execFileSync('git', args, { cwd: root, encoding: 'utf8' });
    runGit('init', '-b', 'main');
    runGit('config', 'user.email', 'workspace-test@zorai.local');
    runGit('config', 'user.name', 'Workspace Test');
    fs.writeFileSync(path.join(root, 'file.txt'), 'base\n');
    runGit('add', 'file.txt');
    runGit('commit', '-m', 'base');
    const created = await workspaceGitCreateWorktree(root, { name: 'dirty-feature', branch: 'feature/dirty' });
    fs.writeFileSync(path.join(created.root, 'file.txt'), 'dirty\n');
    await assert.rejects(workspaceGitRemoveWorktree(root, created.root), { code: 'WORKSPACE_WORKTREE_DIRTY' });
    assert.equal(fs.existsSync(created.root), true);
    runGit('worktree', 'remove', '--force', created.root);
    fs.rmSync(parent, { recursive: true, force: true });
});

test('reviewed worktree integration cherry-picks only the refreshed commit list', async () => {
    const parent = tempWorkspace();
    const root = path.join(parent, 'repo');
    fs.mkdirSync(root);
    const runGit = (cwd, ...args) => require('node:child_process').execFileSync('git', args, { cwd, encoding: 'utf8' });
    runGit(root, 'init', '-b', 'main');
    runGit(root, 'config', 'user.email', 'workspace-test@zorai.local');
    runGit(root, 'config', 'user.name', 'Workspace Test');
    fs.writeFileSync(path.join(root, 'file.txt'), 'base\n');
    runGit(root, 'add', 'file.txt');
    runGit(root, 'commit', '-m', 'base');
    const created = await workspaceGitCreateWorktree(root, { name: 'review-feature', branch: 'zorai/task-review' });
    fs.writeFileSync(path.join(created.root, 'file.txt'), 'reviewed\n');
    runGit(created.root, 'add', 'file.txt');
    runGit(created.root, 'commit', '-m', 'reviewed change');
    const review = await workspaceGitReviewWorktree(root, created.root);
    assert.equal(review.canIntegrate, true);
    assert.equal(review.commits.length, 1);
    assert.equal(review.files[0].path, 'file.txt');
    await assert.rejects(workspaceGitIntegrateWorktree(root, created.root, ['stale']), { code: 'WORKSPACE_INTEGRATION_STALE' });
    const integrated = await workspaceGitIntegrateWorktree(root, created.root, review.commits.map((commit) => commit.hash));
    assert.equal(fs.readFileSync(path.join(root, 'file.txt'), 'utf8'), 'reviewed\n');
    assert.deepEqual(integrated.status, []);
    assert.equal(fs.existsSync(created.root), true);
    runGit(root, 'worktree', 'remove', created.root);
    fs.rmSync(parent, { recursive: true, force: true });
});

test('reviewed integration refuses a dirty target worktree', async () => {
    const parent = tempWorkspace();
    const root = path.join(parent, 'repo');
    fs.mkdirSync(root);
    const runGit = (cwd, ...args) => require('node:child_process').execFileSync('git', args, { cwd, encoding: 'utf8' });
    runGit(root, 'init', '-b', 'main');
    runGit(root, 'config', 'user.email', 'workspace-test@zorai.local');
    runGit(root, 'config', 'user.name', 'Workspace Test');
    fs.writeFileSync(path.join(root, 'file.txt'), 'base\n');
    runGit(root, 'add', 'file.txt');
    runGit(root, 'commit', '-m', 'base');
    const created = await workspaceGitCreateWorktree(root, { name: 'dirty-target-feature', branch: 'zorai/task-dirty-target' });
    fs.writeFileSync(path.join(created.root, 'new.txt'), 'isolated\n');
    runGit(created.root, 'add', 'new.txt');
    runGit(created.root, 'commit', '-m', 'isolated change');
    const review = await workspaceGitReviewWorktree(root, created.root);
    fs.writeFileSync(path.join(root, 'file.txt'), 'dirty target\n');
    await assert.rejects(workspaceGitIntegrateWorktree(root, created.root, review.commits.map((commit) => commit.hash)), { code: 'WORKSPACE_TARGET_DIRTY' });
    runGit(root, 'restore', 'file.txt');
    runGit(root, 'worktree', 'remove', created.root);
    fs.rmSync(parent, { recursive: true, force: true });
});

test('workspace Git history and commit detail remain read-only and structured', async () => {
    const root = tempWorkspace();
    const runGit = (...args) => require('node:child_process').execFileSync('git', args, { cwd: root, encoding: 'utf8' });
    runGit('init', '-b', 'main');
    runGit('config', 'user.email', 'workspace-test@zorai.local');
    runGit('config', 'user.name', 'Workspace Test');
    fs.writeFileSync(path.join(root, 'history.txt'), 'history\n');
    runGit('add', 'history.txt');
    runGit('commit', '-m', 'history commit');
    const history = await workspaceGitHistory(root, { limit: 10 });
    assert.equal(history[0].subject, 'history commit');
    const detail = await workspaceGitCommitDetail(root, history[0].hash);
    assert.equal(detail.subject, 'history commit');
    assert.deepEqual(detail.files, [{ status: 'A', path: 'history.txt' }]);
    assert.equal(runGit('status', '--porcelain'), '');
    fs.rmSync(root, { recursive: true, force: true });
});

test('workspace conflict detection reports unresolved paths', async () => {
    const root = tempWorkspace();
    const runGit = (...args) => require('node:child_process').execFileSync('git', args, { cwd: root, encoding: 'utf8' });
    runGit('init', '-b', 'main');
    runGit('config', 'user.email', 'workspace-test@zorai.local');
    runGit('config', 'user.name', 'Workspace Test');
    fs.writeFileSync(path.join(root, 'conflict.txt'), 'base\n');
    runGit('add', 'conflict.txt');
    runGit('commit', '-m', 'base');
    runGit('checkout', '-b', 'other');
    fs.writeFileSync(path.join(root, 'conflict.txt'), 'other\n');
    runGit('commit', '-am', 'other');
    runGit('checkout', 'main');
    fs.writeFileSync(path.join(root, 'conflict.txt'), 'main\n');
    runGit('commit', '-am', 'main');
    try { runGit('merge', 'other'); } catch {}
    assert.deepEqual(await workspaceGitConflicts(root), [{ path: 'conflict.txt' }]);
    runGit('merge', '--abort');
    fs.rmSync(root, { recursive: true, force: true });
});

test('binary and oversized files are rejected from text context', async () => {
    const root = tempWorkspace();
    fs.writeFileSync(path.join(root, 'binary.bin'), Buffer.from([1, 0, 2]));
    fs.writeFileSync(path.join(root, 'large.txt'), 'abcdef');
    await assert.rejects(readWorkspaceFile(root, 'binary.bin'), { code: 'WORKSPACE_BINARY_FILE' });
    await assert.rejects(readWorkspaceFile(root, 'large.txt', { maxBytes: 3 }), { code: 'WORKSPACE_FILE_TOO_LARGE' });
    fs.rmSync(root, { recursive: true, force: true });
});

test('workspace git diff includes untracked files when requested and show returns HEAD for tracked files', async () => {
    const root = tempWorkspace();
    const runGit = (...args) => require('node:child_process').execFileSync('git', args, { cwd: root, encoding: 'utf8' });
    runGit('init', '-b', 'main');
    runGit('config', 'user.email', 'workspace-test@zorai.local');
    runGit('config', 'user.name', 'Workspace Test');
    fs.writeFileSync(path.join(root, 'tracked.txt'), 'base\n');
    runGit('add', 'tracked.txt');
    runGit('commit', '-m', 'base');
    fs.writeFileSync(path.join(root, 'tracked.txt'), 'changed\n');
    fs.writeFileSync(path.join(root, 'new.txt'), 'alpha\nbeta\n');
    assert.equal(await workspaceGitDiff(root, 'new.txt'), '');
    const untracked = await workspaceGitDiff(root, 'new.txt', { againstHead: true, includeUntracked: true });
    assert.match(untracked, /\+alpha/);
    assert.match(untracked, /\+beta/);
    const tracked = await workspaceGitDiff(root, 'tracked.txt', { againstHead: true });
    assert.match(tracked, /-base/);
    assert.match(tracked, /\+changed/);
    assert.equal(await workspaceGitShow(root, 'tracked.txt'), 'base\n');
    assert.equal(await workspaceGitShow(root, 'new.txt'), '');
    fs.rmSync(root, { recursive: true, force: true });
});

test('workspace git remote actions push a new branch upstream, then pull and sync with the remote', async () => {
    const { execFileSync } = require('node:child_process');
    const base = fs.mkdtempSync(path.join(os.tmpdir(), 'zorai-git-remote-'));
    const remote = path.join(base, 'remote.git');
    const local = path.join(base, 'local');
    const other = path.join(base, 'other');
    const git = (cwd, ...args) => execFileSync('git', args, { cwd, encoding: 'utf8' });
    const identity = ['-c', 'user.name=Test', '-c', 'user.email=test@example.com'];
    git(base, 'init', '--bare', '-b', 'main', remote);
    git(base, 'clone', remote, local);
    git(local, 'checkout', '-b', 'main');
    fs.writeFileSync(path.join(local, 'a.txt'), 'one\n');
    git(local, 'add', 'a.txt');
    git(local, ...identity, 'commit', '-m', 'first');

    const pushed = await workspaceGitRemote(local, 'push');
    assert.equal(pushed.overview.upstream, 'origin/main');
    assert.equal(pushed.overview.ahead, 0);

    git(base, 'clone', remote, other);
    fs.writeFileSync(path.join(other, 'b.txt'), 'two\n');
    git(other, 'add', 'b.txt');
    git(other, ...identity, 'commit', '-m', 'second');
    git(other, 'push');

    const fetched = await workspaceGitRemote(local, 'fetch');
    assert.equal(fetched.overview.behind, 1);
    const pulled = await workspaceGitRemote(local, 'pull');
    assert.equal(pulled.overview.behind, 0);
    assert.ok(fs.existsSync(path.join(local, 'b.txt')));

    fs.writeFileSync(path.join(local, 'c.txt'), 'three\n');
    git(local, 'add', 'c.txt');
    git(local, ...identity, 'commit', '-m', 'third');
    const synced = await workspaceGitRemote(local, 'sync');
    assert.equal(synced.overview.ahead, 0);
    assert.equal(git(remote, 'log', '-1', '--format=%s', 'main').trim(), 'third');

    await assert.rejects(workspaceGitRemote(local, 'force-push'), /Unknown git action/);
});

test('git action steps reject option injection and unknown actions before running git', () => {
    assert.throws(() => gitActionSteps('branchCreate', { name: '--force' }), /Invalid branch name/);
    assert.throws(() => gitActionSteps('checkout', { ref: '-b evil' }), /Invalid ref/);
    assert.throws(() => gitActionSteps('stashDrop', { ref: '--all' }), /Invalid stash/);
    assert.throws(() => gitActionSteps('remoteAdd', { name: 'up', url: '--upload-pack=touch /tmp/x' }), /Invalid remote URL/);
    assert.throws(() => gitActionSteps('commitAll', { message: '  ' }), /message is required/);
    assert.throws(() => gitActionSteps('gc'), /Unknown git action/);
    assert.deepEqual(gitActionSteps('commitAmend', {}), [['commit', '--amend', '--no-edit']]);
    assert.deepEqual(gitActionSteps('stash', { includeUntracked: true, message: 'wip' }), [['stash', 'push', '--include-untracked', '-m', 'wip']]);
});

test('git actions manage commits, branches, stashes, tags and remotes in a real repository', async () => {
    const { execFileSync } = require('node:child_process');
    const base = fs.mkdtempSync(path.join(os.tmpdir(), 'zorai-git-actions-'));
    const repo = path.join(base, 'repo');
    fs.mkdirSync(repo);
    const git = (...args) => execFileSync('git', args, { cwd: repo, encoding: 'utf8' }).trim();
    git('init', '-b', 'main');
    git('config', 'user.name', 'Test');
    git('config', 'user.email', 'test@example.com');
    fs.writeFileSync(path.join(repo, 'a.txt'), 'one\n');
    await workspaceGitAction(repo, 'commitAll', { message: 'first' });
    assert.equal(git('log', '-1', '--format=%s'), 'first');

    fs.writeFileSync(path.join(repo, 'b.txt'), 'two\n');
    await workspaceGitAction(repo, 'commitAll', { message: 'second' });
    await workspaceGitAction(repo, 'commitAmend', { message: 'second, amended' });
    assert.equal(git('log', '-1', '--format=%s'), 'second, amended');
    await workspaceGitAction(repo, 'undoCommit');
    assert.equal(git('log', '-1', '--format=%s'), 'first');
    assert.match(git('status', '--porcelain'), /^A  b\.txt/m);
    await workspaceGitAction(repo, 'unstageAll');
    assert.match(git('status', '--porcelain'), /^\?\? b\.txt/m);

    await workspaceGitAction(repo, 'stash', { includeUntracked: true, message: 'parked' });
    assert.equal(fs.existsSync(path.join(repo, 'b.txt')), false);
    const stashes = await workspaceGitList(repo, 'stashes');
    assert.equal(stashes[0].value, 'stash@{0}');
    assert.match(stashes[0].label, /parked/);
    await workspaceGitAction(repo, 'stashPop', { ref: 'stash@{0}' });
    assert.equal(fs.existsSync(path.join(repo, 'b.txt')), true);
    await workspaceGitAction(repo, 'discardAll');
    assert.equal(fs.existsSync(path.join(repo, 'b.txt')), false);
    assert.equal(git('status', '--porcelain'), '');

    await workspaceGitAction(repo, 'branchCreate', { name: 'feature' });
    assert.equal(git('branch', '--show-current'), 'feature');
    fs.writeFileSync(path.join(repo, 'c.txt'), 'three\n');
    await workspaceGitAction(repo, 'commitAll', { message: 'feature work' });
    await workspaceGitAction(repo, 'checkout', { ref: 'main' });
    await workspaceGitAction(repo, 'merge', { ref: 'feature' });
    assert.equal(fs.existsSync(path.join(repo, 'c.txt')), true);
    await workspaceGitAction(repo, 'branchRename', { from: 'feature', to: 'feature-done' });
    assert.deepEqual((await workspaceGitList(repo, 'localBranches')).map((item) => item.value).sort(), ['feature-done', 'main']);
    await workspaceGitAction(repo, 'branchDelete', { name: 'feature-done' });
    assert.deepEqual((await workspaceGitList(repo, 'localBranches')).map((item) => item.value), ['main']);

    await workspaceGitAction(repo, 'tagCreate', { name: 'v1.0.0', message: 'release' });
    assert.deepEqual((await workspaceGitList(repo, 'tags')).map((item) => item.value), ['v1.0.0']);
    await workspaceGitAction(repo, 'tagDelete', { name: 'v1.0.0' });
    assert.deepEqual(await workspaceGitList(repo, 'tags'), []);

    const remote = path.join(base, 'remote.git');
    execFileSync('git', ['init', '--bare', '-b', 'main', remote]);
    await workspaceGitAction(repo, 'remoteAdd', { name: 'upstream', url: remote });
    await workspaceGitAction(repo, 'remoteRename', { from: 'upstream', to: 'origin' });
    const remotes = await workspaceGitList(repo, 'remotes');
    assert.equal(remotes[0].value, 'origin');
    await workspaceGitAction(repo, 'pushTo', { remote: 'origin' });
    assert.equal(execFileSync('git', ['--git-dir', remote, 'log', '-1', '--format=%s', 'main'], { encoding: 'utf8' }).trim(), 'feature work');

    const cloned = await workspaceGitClone(remote, base, 'copy');
    assert.equal(fs.existsSync(path.join(cloned.path, 'c.txt')), true);
    await assert.rejects(workspaceGitClone(remote, base, 'copy'), /already exists/);

    await workspaceGitAction(repo, 'remoteRemove', { name: 'origin' });
    assert.deepEqual(await workspaceGitList(repo, 'remotes'), []);

    await assert.rejects(workspaceGitAction(repo, 'branchDelete', { name: 'missing' }), /missing/);
    const log = workspaceGitOutput();
    assert.ok(log.some((entry) => entry.command === 'git commit -m first' && entry.ok));
    assert.ok(log.some((entry) => entry.command === 'git branch -d missing' && !entry.ok));
});

test('the git output log records commands that change the repository, not read-only status checks', async () => {
    assert.equal(changesRepository(['add', '--', 'a.txt']), true);
    assert.equal(changesRepository(['worktree', 'list', '--porcelain']), false);
    assert.equal(changesRepository(['branch', '--show-current']), false);
    assert.equal(changesRepository(['branch', '--format=%(refname:short)']), false);
    assert.equal(changesRepository(['status', '--porcelain']), false);

    const { execFileSync } = require('node:child_process');
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'zorai-git-log-'));
    execFileSync('git', ['init', '-b', 'main'], { cwd: root });
    fs.writeFileSync(path.join(root, 'tracked.txt'), 'x\n');
    const before = workspaceGitOutput().length;
    await workspaceGitStage(root, 'tracked.txt');
    const added = workspaceGitOutput().slice(before);
    assert.ok(added.some((entry) => entry.command.startsWith('git add') && entry.ok));
    assert.ok(added.every((entry) => !entry.command.startsWith('git status')));
});

test('git that exits before reading its input reports its own error instead of crashing the app with EPIPE', async () => {
    const { runGitWithInput } = require('./main/workspace-service.cjs');
    const root = tempWorkspace();
    const uncaught = [];
    const onUncaught = (error) => uncaught.push(error.code ?? error.message);
    process.on('uncaughtException', onUncaught);
    try {
        const results = await Promise.allSettled(Array.from({ length: 10 }, () =>
            runGitWithInput(['check-ignore', '-z', '--stdin'], root, 'a'.repeat(200000))));
        assert.equal(results.every((result) => result.status === 'rejected' && /not a git repository/i.test(result.reason.message)), true);
        await new Promise((resolve) => setTimeout(resolve, 200));
    } finally {
        process.off('uncaughtException', onUncaught);
        fs.rmSync(root, { recursive: true, force: true });
    }
    assert.deepEqual(uncaught, []);
});
