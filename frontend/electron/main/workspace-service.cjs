const crypto = require('crypto');
const fs = require('fs');
const path = require('path');
const { execFile, spawn } = require('child_process');
const { promisify } = require('util');

const execFileAsync = promisify(execFile);
const DEFAULT_MAX_FILE_BYTES = 2 * 1024 * 1024;
const GIT_MAX_BUFFER = 8 * 1024 * 1024;
const GIT_REPOSITORY_CHANGING_COMMANDS = new Set(['add', 'apply', 'branch', 'checkout', 'cherry-pick', 'clean', 'commit', 'merge', 'mv', 'rebase', 'reset', 'restore', 'rm', 'stash', 'switch', 'tag', 'worktree']);
const GIT_READ_ONLY_FLAGS = new Set(['--show-current', '--list', '--format', '-l', 'list']);

function changesRepository(args) {
    if (!GIT_REPOSITORY_CHANGING_COMMANDS.has(args[0])) return false;
    return !args.slice(1).some((arg) => GIT_READ_ONLY_FLAGS.has(arg) || arg.startsWith('--format='));
}

async function execGit(args, options = {}) {
    const record = changesRepository(args);
    try {
        const result = await execFileAsync('git', args, options);
        if (record) recordGitOutput({ cwd: options.cwd ?? '', command: `git ${args.join(' ')}`, ok: true, output: [result.stdout, result.stderr].filter((text) => text && String(text).trim()).join('\n').trim() });
        return result;
    } catch (error) {
        if (record) recordGitOutput({ cwd: options.cwd ?? '', command: `git ${args.join(' ')}`, ok: false, output: error?.stderr?.trim() || error?.message || 'git failed.' });
        throw error;
    }
}
const IGNORED_DIRECTORY_NAMES = new Set([
    '.git', 'node_modules', 'target', 'dist', 'build', '.venv', '__pycache__', '.next', '.cache',
]);

function workspaceError(code, message, details = {}) {
    const error = new Error(message);
    error.code = code;
    Object.assign(error, details);
    return error;
}

function sha256(content) {
    return crypto.createHash('sha256').update(content).digest('hex');
}

function isWithinRoot(root, target) {
    const relative = path.relative(root, target);
    return relative === '' || (!relative.startsWith(`..${path.sep}`) && relative !== '..' && !path.isAbsolute(relative));
}

function canonicalWorkspaceRoot(rootPath) {
    if (typeof rootPath !== 'string' || !rootPath.trim()) {
        throw workspaceError('WORKSPACE_ROOT_REQUIRED', 'A workspace root is required.');
    }
    const resolved = path.resolve(rootPath.trim().replace(/^~(?=$|[\\/])/, require('os').homedir()));
    let canonical;
    try {
        canonical = fs.realpathSync.native(resolved);
    } catch {
        throw workspaceError('WORKSPACE_ROOT_NOT_FOUND', `Workspace root does not exist: ${resolved}`);
    }
    if (!fs.statSync(canonical).isDirectory()) {
        throw workspaceError('WORKSPACE_ROOT_NOT_DIRECTORY', `Workspace root is not a directory: ${canonical}`);
    }
    return canonical;
}

function resolveWorkspacePath(rootPath, relativePath = '', { allowMissing = false } = {}) {
    const root = canonicalWorkspaceRoot(rootPath);
    if (typeof relativePath !== 'string' || path.isAbsolute(relativePath)) {
        throw workspaceError('WORKSPACE_PATH_INVALID', 'Workspace paths must be relative.');
    }
    const normalized = path.normalize(relativePath || '.');
    const candidate = path.resolve(root, normalized);
    if (!isWithinRoot(root, candidate)) {
        throw workspaceError('WORKSPACE_PATH_ESCAPE', `Path escapes workspace root: ${relativePath}`);
    }

    if (fs.existsSync(candidate)) {
        const canonical = fs.realpathSync.native(candidate);
        if (!isWithinRoot(root, canonical)) {
            throw workspaceError('WORKSPACE_SYMLINK_ESCAPE', `Symlink escapes workspace root: ${relativePath}`);
        }
        return { root, absolutePath: canonical, relativePath: path.relative(root, canonical) };
    }
    if (!allowMissing) {
        throw workspaceError('WORKSPACE_PATH_NOT_FOUND', `Workspace path does not exist: ${relativePath}`);
    }

    let existingParent = path.dirname(candidate);
    while (!fs.existsSync(existingParent)) {
        const next = path.dirname(existingParent);
        if (next === existingParent) break;
        existingParent = next;
    }
    const canonicalParent = fs.realpathSync.native(existingParent);
    if (!isWithinRoot(root, canonicalParent)) {
        throw workspaceError('WORKSPACE_SYMLINK_ESCAPE', `Parent symlink escapes workspace root: ${relativePath}`);
    }
    return { root, absolutePath: candidate, relativePath: path.relative(root, candidate) };
}

const LANGUAGE_BY_EXTENSION = {
        '.c': 'c', '.h': 'c', '.cc': 'cpp', '.cpp': 'cpp', '.cxx': 'cpp', '.hpp': 'cpp', '.cs': 'csharp',
        '.css': 'css', '.scss': 'scss', '.sass': 'scss', '.less': 'less', '.dart': 'dart', '.ex': 'elixir', '.exs': 'elixir',
        '.fs': 'fsharp', '.fsx': 'fsharp', '.go': 'go', '.graphql': 'graphql', '.gql': 'graphql', '.groovy': 'java',
        '.html': 'html', '.htm': 'html', '.java': 'java', '.js': 'javascript', '.mjs': 'javascript', '.cjs': 'javascript',
        '.jsx': 'javascript', '.json': 'json', '.jsonc': 'json', '.kt': 'kotlin', '.kts': 'kotlin', '.lua': 'lua',
        '.md': 'markdown', '.mdx': 'mdx', '.php': 'php', '.pl': 'perl', '.proto': 'proto', '.py': 'python', '.r': 'r',
        '.rb': 'ruby', '.rs': 'rust', '.sh': 'shell', '.bash': 'shell', '.zsh': 'shell', '.sql': 'sql', '.swift': 'swift',
        '.svelte': 'html', '.tf': 'hcl', '.tfvars': 'hcl', '.toml': 'ini', '.ts': 'typescript', '.mts': 'typescript',
        '.cts': 'typescript', '.tsx': 'typescript', '.vue': 'html', '.xml': 'xml', '.yaml': 'yaml', '.yml': 'yaml', '.zig': 'cpp',
    };
const LANGUAGE_BY_NAME = {
    dockerfile: 'dockerfile', containerfile: 'dockerfile', makefile: 'shell', gnumakefile: 'shell',
    rakefile: 'ruby', gemfile: 'ruby', guardfile: 'ruby', procfile: 'shell',
    '.bashrc': 'shell', '.bash_profile': 'shell', '.zshrc': 'shell', '.profile': 'shell',
    '.editorconfig': 'ini', '.npmrc': 'ini', '.prettierrc': 'json', '.eslintrc': 'json',
    'tsconfig.json': 'json', 'jsconfig.json': 'json', 'deno.json': 'json', 'deno.jsonc': 'json',
};

function languageForPath(filePath) {
    const name = path.basename(filePath).toLowerCase();
    return LANGUAGE_BY_NAME[name] || LANGUAGE_BY_EXTENSION[path.extname(name)] || 'plaintext';
}

function isProbablyBinary(buffer) {
    const sample = buffer.subarray(0, Math.min(buffer.length, 8192));
    return sample.includes(0);
}

async function openWorkspace(rootPath) {
    const root = canonicalWorkspaceRoot(rootPath);
    const gitRoot = await resolveGitRoot(root);
    return {
        root,
        name: path.basename(root),
        gitRoot,
        isGitRepository: gitRoot === root,
    };
}

async function listWorkspaceDirectory(rootPath, relativePath = '', options = {}) {
    const resolved = resolveWorkspacePath(rootPath, relativePath);
    const stats = fs.statSync(resolved.absolutePath);
    if (!stats.isDirectory()) throw workspaceError('WORKSPACE_NOT_DIRECTORY', `${relativePath} is not a directory.`);
    const includeIgnored = options.includeIgnored === true;
    const entries = (await fs.promises.readdir(resolved.absolutePath, { withFileTypes: true }))
        .filter((entry) => includeIgnored || !IGNORED_DIRECTORY_NAMES.has(entry.name));
    const gitIgnored = await gitIgnoredNames(resolved.absolutePath, entries.map((entry) => entry.name));
    return entries
        .map((entry) => {
            const absolutePath = path.join(resolved.absolutePath, entry.name);
            let itemStats = null;
            try { itemStats = fs.statSync(absolutePath); } catch { itemStats = null; }
            return {
                name: entry.name,
                path: path.relative(resolved.root, absolutePath),
                isDirectory: entry.isDirectory(),
                isSymbolicLink: entry.isSymbolicLink(),
                sizeBytes: itemStats?.size ?? null,
                modifiedAt: itemStats?.mtimeMs ?? null,
                isGitIgnored: gitIgnored.has(entry.name),
            };
        })
        .sort((a, b) => Number(b.isDirectory) - Number(a.isDirectory) || a.name.localeCompare(b.name));
}

async function gitIgnoredNames(directory, names) {
    if (names.length === 0) return new Set();
    const output = await runGitWithInput(['check-ignore', '-z', '--stdin'], directory, `${names.join('\0')}\0`).catch(() => '');
    return new Set(output.split('\0').filter(Boolean));
}

async function statWorkspaceFile(rootPath, relativePath) {
    const resolved = resolveWorkspacePath(rootPath, relativePath);
    const stats = await fs.promises.stat(resolved.absolutePath);
    if (!stats.isFile()) throw workspaceError('WORKSPACE_NOT_FILE', `${relativePath} is not a file.`);
    return { path: resolved.relativePath, sizeBytes: stats.size, modifiedAt: stats.mtimeMs };
}

async function readWorkspaceFile(rootPath, relativePath, options = {}) {
    const resolved = resolveWorkspacePath(rootPath, relativePath);
    const stats = await fs.promises.stat(resolved.absolutePath);
    if (!stats.isFile()) throw workspaceError('WORKSPACE_NOT_FILE', `${relativePath} is not a file.`);
    const maxBytes = Math.max(1, Math.min(Number(options.maxBytes) || DEFAULT_MAX_FILE_BYTES, 100 * 1024 * 1024));
    if (stats.size > maxBytes) {
        throw workspaceError('WORKSPACE_FILE_TOO_LARGE', `File is ${stats.size} bytes; limit is ${maxBytes}.`, { sizeBytes: stats.size, maxBytes });
    }
    const buffer = await fs.promises.readFile(resolved.absolutePath);
    if (isProbablyBinary(buffer)) {
        throw workspaceError('WORKSPACE_BINARY_FILE', 'Binary files cannot be opened in the text editor.');
    }
    const content = buffer.toString('utf8');
    return {
        path: resolved.relativePath,
        content,
        hash: sha256(buffer),
        sizeBytes: buffer.length,
        modifiedAt: stats.mtimeMs,
        language: languageForPath(resolved.absolutePath),
    };
}

async function writeWorkspaceFile(rootPath, relativePath, content, expectedHash = null) {
    if (typeof content !== 'string') throw workspaceError('WORKSPACE_CONTENT_INVALID', 'File content must be text.');
    const resolved = resolveWorkspacePath(rootPath, relativePath, { allowMissing: true });
    let currentHash = null;
    if (fs.existsSync(resolved.absolutePath)) {
        const current = await fs.promises.readFile(resolved.absolutePath);
        currentHash = sha256(current);
        if (expectedHash !== null && expectedHash !== currentHash) {
            throw workspaceError('WORKSPACE_WRITE_CONFLICT', 'File changed since it was opened.', { currentHash, expectedHash });
        }
    } else if (expectedHash !== null) {
        throw workspaceError('WORKSPACE_WRITE_CONFLICT', 'File was removed since it was opened.', { currentHash: null, expectedHash });
    }
    await fs.promises.mkdir(path.dirname(resolved.absolutePath), { recursive: true });
    const tempPath = path.join(path.dirname(resolved.absolutePath), `.${path.basename(resolved.absolutePath)}.zorai-${process.pid}-${Date.now()}.tmp`);
    try {
        await fs.promises.writeFile(tempPath, content, { encoding: 'utf8', flag: 'wx' });
        await fs.promises.rename(tempPath, resolved.absolutePath);
    } finally {
        await fs.promises.rm(tempPath, { force: true }).catch(() => {});
    }
    return readWorkspaceFile(resolved.root, resolved.relativePath, { maxBytes: Math.max(DEFAULT_MAX_FILE_BYTES, Buffer.byteLength(content)) });
}

async function createWorkspaceDirectory(rootPath, relativePath) {
    const resolved = resolveWorkspacePath(rootPath, relativePath, { allowMissing: true });
    await fs.promises.mkdir(resolved.absolutePath, { recursive: false });
    return { path: resolved.relativePath };
}

async function renameWorkspacePath(rootPath, fromRelativePath, toRelativePath) {
    const from = resolveWorkspacePath(rootPath, fromRelativePath);
    const to = resolveWorkspacePath(rootPath, toRelativePath, { allowMissing: true });
    if (from.root !== to.root) throw workspaceError('WORKSPACE_ROOT_MISMATCH', 'Rename must remain in one workspace.');
    await fs.promises.rename(from.absolutePath, to.absolutePath);
    return { from: from.relativePath, to: to.relativePath };
}

async function deleteWorkspacePath(rootPath, relativePath, options = {}) {
    const resolved = resolveWorkspacePath(rootPath, relativePath);
    if (!resolved.relativePath) throw workspaceError('WORKSPACE_DELETE_ROOT', 'The workspace root cannot be deleted.');
    const stats = await fs.promises.stat(resolved.absolutePath);
    if (stats.isDirectory() && options.recursive !== true) {
        throw workspaceError('WORKSPACE_RECURSIVE_REQUIRED', 'Deleting a directory requires recursive confirmation.');
    }
    await fs.promises.rm(resolved.absolutePath, { recursive: options.recursive === true, force: false });
    return true;
}

async function resolveGitRoot(targetPath) {
    try {
        const { stdout } = await execGit(['rev-parse', '--show-toplevel'], { cwd: targetPath, encoding: 'utf8', timeout: 5000 });
        return fs.realpathSync.native(stdout.trim());
    } catch { return null; }
}

async function searchWorkspace(rootPath, query, options = {}) {
    const root = canonicalWorkspaceRoot(rootPath);
    const needle = typeof query === 'string' ? query.trim() : '';
    if (!needle) return [];
    const caseSensitive = options.caseSensitive === true;
    const comparableNeedle = caseSensitive ? needle : needle.toLowerCase();
    const maxResults = Math.max(1, Math.min(Number(options.maxResults) || 100, 500));
    const maxFiles = Math.max(1, Math.min(Number(options.maxFiles) || 5000, 20000));
    const results = [];
    const pending = [root];
    let visitedFiles = 0;

    while (pending.length > 0 && results.length < maxResults && visitedFiles < maxFiles) {
        const directory = pending.pop();
        let entries;
        try { entries = await fs.promises.readdir(directory, { withFileTypes: true }); } catch { continue; }
        for (const entry of entries) {
            if (results.length >= maxResults || visitedFiles >= maxFiles) break;
            if (IGNORED_DIRECTORY_NAMES.has(entry.name) || entry.isSymbolicLink()) continue;
            const absolutePath = path.join(directory, entry.name);
            if (entry.isDirectory()) {
                pending.push(absolutePath);
                continue;
            }
            if (!entry.isFile()) continue;
            visitedFiles += 1;
            let stats;
            try { stats = await fs.promises.stat(absolutePath); } catch { continue; }
            if (stats.size > 1024 * 1024) continue;
            let buffer;
            try { buffer = await fs.promises.readFile(absolutePath); } catch { continue; }
            if (isProbablyBinary(buffer)) continue;
            const lines = buffer.toString('utf8').split(/\r?\n/);
            for (let lineIndex = 0; lineIndex < lines.length && results.length < maxResults; lineIndex += 1) {
                const comparableLine = caseSensitive ? lines[lineIndex] : lines[lineIndex].toLowerCase();
                const columnIndex = comparableLine.indexOf(comparableNeedle);
                if (columnIndex < 0) continue;
                results.push({
                    path: path.relative(root, absolutePath),
                    line: lineIndex + 1,
                    column: columnIndex + 1,
                    preview: lines[lineIndex].trim().slice(0, 240),
                });
            }
        }
    }
    return results;
}

async function workspaceGitStatus(rootPath) {
    const root = canonicalWorkspaceRoot(rootPath);
    const gitRoot = await resolveGitRoot(root);
    if (!gitRoot || !isWithinRoot(gitRoot, root)) return [];
    const { stdout } = await execGit(['status', '--porcelain=v1', '-z', '--untracked-files=all'], {
        cwd: root, encoding: 'utf8', timeout: 10000, maxBuffer: GIT_MAX_BUFFER,
    });
    const records = stdout.split('\0').filter(Boolean);
    const result = [];
    for (let index = 0; index < records.length; index += 1) {
        const record = records[index];
        const indexStatus = record[0];
        const worktreeStatus = record[1];
        let filePath = record.slice(3);
        let previousPath = null;
        if (indexStatus === 'R' || indexStatus === 'C') {
            previousPath = filePath;
            filePath = records[index + 1] || filePath;
            index += 1;
        }
        result.push({ path: filePath, previousPath, indexStatus, worktreeStatus });
    }
    return result;
}

function parseGitWorktreeList(output) {
    const worktrees = [];
    let current = null;
    for (const line of String(output || '').split(/\r?\n/)) {
        if (line.startsWith('worktree ')) {
            if (current) worktrees.push(current);
            current = { path: line.slice('worktree '.length), head: null, branch: null, detached: false, bare: false, locked: false, prunable: false };
        } else if (current && line.startsWith('HEAD ')) current.head = line.slice(5);
        else if (current && line.startsWith('branch ')) current.branch = line.slice(7).replace(/^refs\/heads\//, '');
        else if (current && line === 'detached') current.detached = true;
        else if (current && line === 'bare') current.bare = true;
        else if (current && line.startsWith('locked')) current.locked = true;
        else if (current && line.startsWith('prunable')) current.prunable = true;
    }
    if (current) worktrees.push(current);
    return worktrees;
}

async function workspaceGitListWorktrees(rootPath) {
    const root = canonicalWorkspaceRoot(rootPath);
    const gitRoot = await resolveGitRoot(root);
    if (!gitRoot) return [];
    const { stdout } = await execGit(['worktree', 'list', '--porcelain'], { cwd: gitRoot, encoding: 'utf8', timeout: 10000, maxBuffer: GIT_MAX_BUFFER });
    return parseGitWorktreeList(stdout);
}

async function workspaceGitCreateWorktree(rootPath, options = {}) {
    const root = canonicalWorkspaceRoot(rootPath);
    const gitRoot = await resolveGitRoot(root);
    if (!gitRoot) throw workspaceError('WORKSPACE_NOT_GIT_REPOSITORY', 'Workspace is not inside a Git repository.');
    const name = typeof options.name === 'string' ? options.name.trim() : '';
    if (!/^[A-Za-z0-9][A-Za-z0-9._-]{0,79}$/.test(name)) {
        throw workspaceError('WORKSPACE_WORKTREE_NAME_INVALID', 'Worktree name must use letters, numbers, dots, underscores, or dashes.');
    }
    const branch = typeof options.branch === 'string' ? options.branch.trim() : '';
    if (!branch) throw workspaceError('WORKSPACE_WORKTREE_BRANCH_REQUIRED', 'A branch name is required.');
    await execGit(['check-ref-format', '--branch', branch], { cwd: gitRoot, encoding: 'utf8', timeout: 5000 })
        .catch(() => { throw workspaceError('WORKSPACE_WORKTREE_BRANCH_INVALID', `Invalid branch name: ${branch}`); });
    const baseRef = typeof options.baseRef === 'string' && options.baseRef.trim() ? options.baseRef.trim() : 'HEAD';
    await execGit(['rev-parse', '--verify', `${baseRef}^{commit}`], { cwd: gitRoot, encoding: 'utf8', timeout: 5000 })
        .catch(() => { throw workspaceError('WORKSPACE_WORKTREE_BASE_INVALID', `Base revision does not resolve to a commit: ${baseRef}`); });
    const container = path.join(path.dirname(gitRoot), `${path.basename(gitRoot)}-worktrees`);
    await fs.promises.mkdir(container, { recursive: true });
    const destination = path.join(container, name);
    if (fs.existsSync(destination)) throw workspaceError('WORKSPACE_WORKTREE_EXISTS', `Worktree destination already exists: ${destination}`);
    const branchExists = await execGit(['show-ref', '--verify', '--quiet', `refs/heads/${branch}`], { cwd: gitRoot, timeout: 5000 })
        .then(() => true).catch(() => false);
    const args = branchExists
        ? ['worktree', 'add', destination, branch]
        : ['worktree', 'add', '-b', branch, destination, baseRef];
    await execGit(args, { cwd: gitRoot, encoding: 'utf8', timeout: 60000, maxBuffer: GIT_MAX_BUFFER })
        .catch((error) => { throw workspaceError('WORKSPACE_WORKTREE_CREATE_FAILED', error?.stderr?.trim() || error?.message || 'Git worktree creation failed.'); });
    return { root: fs.realpathSync.native(destination), branch, baseRef, worktrees: await workspaceGitListWorktrees(gitRoot) };
}

async function resolveManagedWorktree(repoRoot, worktreePath) {
    const candidate = canonicalWorkspaceRoot(worktreePath);
    const containerPath = path.join(path.dirname(repoRoot), `${path.basename(repoRoot)}-worktrees`);
    if (!fs.existsSync(containerPath)) throw workspaceError('WORKSPACE_WORKTREE_REMOVE_SCOPE', 'Managed worktree container does not exist.');
    const container = fs.realpathSync.native(containerPath);
    if (!isWithinRoot(container, candidate) || candidate === container || candidate === repoRoot) {
        throw workspaceError('WORKSPACE_WORKTREE_REMOVE_SCOPE', 'Only sibling worktrees in the managed worktree container are allowed.');
    }
    const worktrees = await workspaceGitListWorktrees(repoRoot);
    const registered = worktrees.find((entry) => path.resolve(entry.path) === candidate);
    if (!registered) throw workspaceError('WORKSPACE_WORKTREE_NOT_REGISTERED', 'The selected directory is not a registered worktree for this repository.');
    return { candidate, registered, worktrees };
}

async function workspaceGitReviewWorktree(rootPath, worktreePath) {
    const root = canonicalWorkspaceRoot(rootPath);
    const gitRoot = await resolveGitRoot(root);
    if (!gitRoot) throw workspaceError('WORKSPACE_NOT_GIT_REPOSITORY', 'Workspace is not inside a Git repository.');
    const { candidate, registered } = await resolveManagedWorktree(gitRoot, worktreePath);
    const sourceStatus = await execGit(['status', '--porcelain'], { cwd: candidate, encoding: 'utf8', timeout: 10000, maxBuffer: GIT_MAX_BUFFER });
    const targetStatus = await execGit(['status', '--porcelain'], { cwd: root, encoding: 'utf8', timeout: 10000, maxBuffer: GIT_MAX_BUFFER });
    const sourceHead = registered.head || await execGit(['rev-parse', 'HEAD'], { cwd: candidate, encoding: 'utf8', timeout: 5000 }).then(({ stdout }) => stdout.trim());
    const targetHead = await execGit(['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8', timeout: 5000 }).then(({ stdout }) => stdout.trim());
    const mergeBase = await execGit(['merge-base', targetHead, sourceHead], { cwd: gitRoot, encoding: 'utf8', timeout: 5000 }).then(({ stdout }) => stdout.trim());
    const commitsRaw = await execGit(['log', '--reverse', '--format=%H%x00%s', `${targetHead}..${sourceHead}`], { cwd: gitRoot, encoding: 'utf8', timeout: 10000, maxBuffer: GIT_MAX_BUFFER }).then(({ stdout }) => stdout);
    const commits = commitsRaw.split('\n').filter(Boolean).map((line) => {
        const separator = line.indexOf('\0');
        return { hash: separator >= 0 ? line.slice(0, separator) : line, subject: separator >= 0 ? line.slice(separator + 1) : '' };
    });
    const filesRaw = await execGit(['diff', '--name-status', `${mergeBase}..${sourceHead}`], { cwd: gitRoot, encoding: 'utf8', timeout: 10000, maxBuffer: GIT_MAX_BUFFER }).then(({ stdout }) => stdout);
    const files = filesRaw.split(/\r?\n/).filter(Boolean).map((line) => {
        const [status, ...parts] = line.split('\t');
        return { status, path: parts[parts.length - 1] || '', previousPath: parts.length > 1 ? parts[0] : null };
    });
    return {
        source: { path: candidate, branch: registered.branch, head: sourceHead, clean: !sourceStatus.stdout.trim() },
        target: { path: root, head: targetHead, clean: !targetStatus.stdout.trim() },
        mergeBase,
        commits,
        files,
        canIntegrate: !sourceStatus.stdout.trim() && !targetStatus.stdout.trim() && commits.length > 0,
    };
}

async function workspaceGitIntegrateWorktree(rootPath, worktreePath, expectedCommitHashes) {
    const review = await workspaceGitReviewWorktree(rootPath, worktreePath);
    if (!review.source.clean) throw workspaceError('WORKSPACE_WORKTREE_DIRTY', 'The isolated worktree has uncommitted changes. Commit or discard them before integration.');
    if (!review.target.clean) throw workspaceError('WORKSPACE_TARGET_DIRTY', 'The target worktree has uncommitted changes. Commit or stash them before integration.');
    const expected = Array.isArray(expectedCommitHashes) ? expectedCommitHashes.map(String) : [];
    const actual = review.commits.map((commit) => commit.hash);
    if (expected.length === 0 || expected.length !== actual.length || expected.some((hash, index) => hash !== actual[index])) {
        throw workspaceError('WORKSPACE_INTEGRATION_STALE', 'The isolated commit list changed. Refresh the review before integrating.');
    }
    try {
        await execGit(['cherry-pick', ...actual], { cwd: review.target.path, encoding: 'utf8', timeout: 120000, maxBuffer: GIT_MAX_BUFFER });
    } catch (error) {
        await execGit(['cherry-pick', '--abort'], { cwd: review.target.path, encoding: 'utf8', timeout: 30000, maxBuffer: GIT_MAX_BUFFER }).catch(() => {});
        throw workspaceError('WORKSPACE_INTEGRATION_CONFLICT', error?.stderr?.trim() || error?.message || 'Cherry-pick conflicted and was aborted.');
    }
    return {
        integratedCommits: actual,
        overview: await workspaceGitOverview(review.target.path),
        status: await workspaceGitStatus(review.target.path),
        review: await workspaceGitReviewWorktree(review.target.path, worktreePath),
    };
}

async function workspaceGitRemoveWorktree(rootPath, worktreePath) {
    const root = canonicalWorkspaceRoot(rootPath);
    const gitRoot = await resolveGitRoot(root);
    if (!gitRoot) throw workspaceError('WORKSPACE_NOT_GIT_REPOSITORY', 'Workspace is not inside a Git repository.');
    const { candidate } = await resolveManagedWorktree(gitRoot, worktreePath);
    const { stdout: status } = await execGit(['status', '--porcelain'], { cwd: candidate, encoding: 'utf8', timeout: 10000, maxBuffer: GIT_MAX_BUFFER });
    if (status.trim()) throw workspaceError('WORKSPACE_WORKTREE_DIRTY', 'The worktree has uncommitted changes and cannot be removed.');
    await execGit(['worktree', 'remove', candidate], { cwd: gitRoot, encoding: 'utf8', timeout: 60000, maxBuffer: GIT_MAX_BUFFER })
        .catch((error) => { throw workspaceError('WORKSPACE_WORKTREE_REMOVE_FAILED', error?.stderr?.trim() || error?.message || 'Git worktree removal failed.'); });
    return workspaceGitListWorktrees(gitRoot);
}

async function workspaceGitHistory(rootPath, options = {}) {
    const root = canonicalWorkspaceRoot(rootPath);
    const gitRoot = await resolveGitRoot(root);
    if (!gitRoot) return [];
    const limit = Math.max(1, Math.min(Number(options.limit) || 50, 200));
    if (options.graph) {
        const graphLimit = Math.max(1, Math.min(Number(options.limit) || 200, 1000));
        const { stdout } = await execGit(['log', '--all', '--topo-order', `-${graphLimit}`, '--date=iso-strict', '--format=%H%x00%h%x00%an%x00%ad%x00%s%x00%P%x00%D'], { cwd: gitRoot, encoding: 'utf8', timeout: 10000, maxBuffer: GIT_MAX_BUFFER });
        return stdout.split(/\r?\n/).filter(Boolean).map((line) => {
            const [hash, shortHash, author, date, subject, parentsRaw, refsRaw] = line.split('\0');
            return {
                hash, shortHash, author, date, subject,
                parents: parentsRaw ? parentsRaw.split(' ').filter(Boolean) : [],
                refs: refsRaw ? refsRaw.split(', ').map((ref) => ref.replace(/^HEAD -> /, '')).filter(Boolean) : [],
            };
        });
    }
    const hasCommits = await execGit(['rev-parse', '--verify', '--quiet', 'HEAD'], { cwd: gitRoot, encoding: 'utf8', timeout: 5000 })
        .then(() => true, () => false);
    if (!hasCommits) return [];
    const { stdout } = await execGit(['log', `-${limit}`, '--date=iso-strict', '--format=%H%x00%h%x00%an%x00%ad%x00%s'], { cwd: gitRoot, encoding: 'utf8', timeout: 10000, maxBuffer: GIT_MAX_BUFFER });
    return stdout.split(/\r?\n/).filter(Boolean).map((line) => {
        const [hash, shortHash, author, date, subject] = line.split('\0');
        return { hash, shortHash, author, date, subject };
    });
}

async function workspaceGitCommitDetail(rootPath, commitHash) {
    const root = canonicalWorkspaceRoot(rootPath);
    const gitRoot = await resolveGitRoot(root);
    if (!gitRoot) throw workspaceError('WORKSPACE_NOT_GIT_REPOSITORY', 'Workspace is not inside a Git repository.');
    const normalized = typeof commitHash === 'string' ? commitHash.trim() : '';
    if (!/^[0-9a-fA-F]{7,64}$/.test(normalized)) throw workspaceError('WORKSPACE_COMMIT_INVALID', 'Invalid commit hash.');
    const { stdout: metadata } = await execGit(['show', '-s', '--date=iso-strict', '--format=%H%x00%an%x00%ad%x00%B', normalized], { cwd: gitRoot, encoding: 'utf8', timeout: 10000, maxBuffer: GIT_MAX_BUFFER });
    const firstLineEnd = metadata.indexOf('\n');
    const header = firstLineEnd >= 0 ? metadata.slice(0, firstLineEnd) : metadata;
    const body = firstLineEnd >= 0 ? metadata.slice(firstLineEnd + 1).trim() : '';
    const [hash, author, date, subject] = header.split('\0');
    const { stdout: filesRaw } = await execGit(['show', '--name-status', '--format=', normalized], { cwd: gitRoot, encoding: 'utf8', timeout: 10000, maxBuffer: GIT_MAX_BUFFER });
    const files = filesRaw.split(/\r?\n/).filter(Boolean).map((line) => { const [status, ...parts] = line.split('\t'); return { status, path: parts[parts.length - 1] || '' }; });
    return { hash, author, date, subject, body, files };
}

async function workspaceGitConflicts(rootPath) {
    const root = canonicalWorkspaceRoot(rootPath);
    const gitRoot = await resolveGitRoot(root);
    if (!gitRoot) return [];
    const { stdout } = await execGit(['diff', '--name-only', '--diff-filter=U'], { cwd: gitRoot, encoding: 'utf8', timeout: 10000, maxBuffer: GIT_MAX_BUFFER });
    return stdout.split(/\r?\n/).filter(Boolean).map((filePath) => ({ path: filePath }));
}

const GIT_BRANCH_NAME_PATTERN = /^(?!-)[\w./^~-]+$/;

async function workspaceGitBranches(rootPath) {
    const root = canonicalWorkspaceRoot(rootPath);
    const gitRoot = await resolveGitRoot(root);
    if (!gitRoot) return [];
    const { stdout } = await execGit(['branch', '--format=%(refname:short)%00%(HEAD)'], {
        cwd: gitRoot, encoding: 'utf8', timeout: 10000, maxBuffer: GIT_MAX_BUFFER,
    });
    return stdout.split(/\r?\n/).filter(Boolean).map((line) => {
        const [name, headMarker] = line.split('\0');
        return { name, isCurrent: headMarker === '*' };
    }).sort((left, right) => (right.isCurrent ? 1 : 0) - (left.isCurrent ? 1 : 0) || left.name.localeCompare(right.name));
}

async function workspaceGitCheckout(rootPath, branch) {
    const root = canonicalWorkspaceRoot(rootPath);
    const gitRoot = await resolveGitRoot(root);
    if (!gitRoot) throw workspaceError('WORKSPACE_NOT_GIT_REPOSITORY', 'Workspace is not inside a Git repository.');
    const normalized = typeof branch === 'string' ? branch.trim() : '';
    if (!normalized || !GIT_BRANCH_NAME_PATTERN.test(normalized)) {
        throw workspaceError('WORKSPACE_BRANCH_INVALID', 'Invalid branch name.');
    }
    await execGit(['checkout', '--', normalized], {
        cwd: gitRoot, encoding: 'utf8', timeout: 30000, maxBuffer: GIT_MAX_BUFFER,
    }).catch(async (error) => {
        // `git checkout -- <name>` fails for branch names that shadow paths; retry bare.
        await execGit(['checkout', normalized], {
            cwd: gitRoot, encoding: 'utf8', timeout: 30000, maxBuffer: GIT_MAX_BUFFER,
        }).catch(() => {
            throw workspaceError('WORKSPACE_CHECKOUT_FAILED', error?.stderr?.trim() || error?.message || 'Git checkout failed.');
        });
    });
    return { overview: await workspaceGitOverview(root), status: await workspaceGitStatus(root) };
}

const GIT_OUTPUT_LOG_MAX = 200;
const gitOutputLog = [];

function recordGitOutput(entry) {
    gitOutputLog.push({ ...entry, at: Date.now() });
    if (gitOutputLog.length > GIT_OUTPUT_LOG_MAX) gitOutputLog.splice(0, gitOutputLog.length - GIT_OUTPUT_LOG_MAX);
}

function workspaceGitOutput() {
    return gitOutputLog.slice();
}

async function runGitLogged(cwd, args, timeout = 120000) {
    const command = `git ${args.join(' ')}`;
    try {
        const { stdout, stderr } = await execFileAsync('git', args, {
            cwd,
            encoding: 'utf8',
            timeout,
            maxBuffer: GIT_MAX_BUFFER,
            env: { ...process.env, GIT_TERMINAL_PROMPT: '0' },
        });
        const output = [stdout, stderr].filter((text) => text && text.trim()).join('\n').trim();
        recordGitOutput({ cwd, command, ok: true, output });
        return output;
    } catch (error) {
        const message = error?.stderr?.trim() || error?.stdout?.trim() || error?.message || `${command} failed.`;
        recordGitOutput({ cwd, command, ok: false, output: message });
        throw workspaceError('WORKSPACE_GIT_ACTION_FAILED', message);
    }
}

function gitName(value, label) {
    const text = typeof value === 'string' ? value.trim() : '';
    if (!text || !GIT_BRANCH_NAME_PATTERN.test(text)) throw workspaceError('WORKSPACE_GIT_ARGUMENT_INVALID', `Invalid ${label}.`);
    return text;
}

function gitRef(value, label) {
    const text = typeof value === 'string' ? value.trim() : '';
    if (!text || text.startsWith('-') || /[\s\0]/.test(text)) throw workspaceError('WORKSPACE_GIT_ARGUMENT_INVALID', `Invalid ${label}.`);
    return text;
}

function gitStashRef(value) {
    const text = typeof value === 'string' ? value.trim() : '';
    if (!/^stash@\{\d+\}$/.test(text)) throw workspaceError('WORKSPACE_GIT_ARGUMENT_INVALID', 'Invalid stash.');
    return text;
}

function gitMessage(value, required) {
    const text = typeof value === 'string' ? value.trim() : '';
    if (required && !text) throw workspaceError('WORKSPACE_GIT_ARGUMENT_INVALID', 'A message is required.');
    return text;
}

function gitRemoteUrl(value) {
    const text = typeof value === 'string' ? value.trim() : '';
    if (!text || text.startsWith('-') || /[\s\0]/.test(text)) throw workspaceError('WORKSPACE_GIT_ARGUMENT_INVALID', 'Invalid remote URL.');
    return text;
}

function gitActionSteps(action, params = {}) {
    switch (action) {
        case 'pull': return [['pull', '--no-rebase']];
        case 'pullRebase': return [['pull', '--rebase']];
        case 'pullFrom': return [['pull', '--no-rebase', gitName(params.remote, 'remote'), gitRef(params.branch, 'branch')]];
        case 'push': return [['push']];
        case 'pushForce': return [['push', '--force-with-lease']];
        case 'pushTo': return [['push', '--set-upstream', gitName(params.remote, 'remote'), 'HEAD']];
        case 'pushTags': return [['push', '--tags']];
        case 'fetch': return [['fetch', '--all', '--prune']];
        case 'sync': return [['pull', '--no-rebase'], ['push']];
        case 'commitAll': return [['add', '-A'], ['commit', '-m', gitMessage(params.message, true)]];
        case 'commitAmend': {
            const message = gitMessage(params.message, false);
            return [message ? ['commit', '--amend', '-m', message] : ['commit', '--amend', '--no-edit']];
        }
        case 'undoCommit': return [['reset', '--soft', 'HEAD~1']];
        case 'stageAll': return [['add', '-A']];
        case 'unstageAll': return [['restore', '--staged', ':/']];
        case 'discardAll': return [['restore', '--staged', '--worktree', ':/'], ['clean', '-fd']];
        case 'checkout': return [['checkout', gitRef(params.ref, 'ref')]];
        case 'branchCreate': {
            const name = gitName(params.name, 'branch name');
            const from = typeof params.from === 'string' && params.from.trim() ? gitRef(params.from, 'start point') : null;
            return [from ? ['switch', '-c', name, from] : ['switch', '-c', name]];
        }
        case 'branchRename': return [['branch', '-m', gitName(params.from, 'branch'), gitName(params.to, 'new branch name')]];
        case 'branchDelete': return [['branch', params.force ? '-D' : '-d', gitName(params.name, 'branch')]];
        case 'merge': return [['merge', '--no-edit', gitRef(params.ref, 'branch')]];
        case 'rebase': return [['rebase', gitRef(params.ref, 'branch')]];
        case 'stash': {
            const args = ['stash', 'push'];
            if (params.includeUntracked) args.push('--include-untracked');
            const message = gitMessage(params.message, false);
            if (message) args.push('-m', message);
            return [args];
        }
        case 'stashApply': return [['stash', 'apply', gitStashRef(params.ref)]];
        case 'stashPop': return [['stash', 'pop', gitStashRef(params.ref)]];
        case 'stashDrop': return [['stash', 'drop', gitStashRef(params.ref)]];
        case 'tagCreate': {
            const name = gitName(params.name, 'tag name');
            const message = gitMessage(params.message, false);
            const args = message ? ['tag', '-a', name, '-m', message] : ['tag', name];
            if (typeof params.ref === 'string' && params.ref.trim()) args.push(gitRef(params.ref, 'ref'));
            return [args];
        }
        case 'tagDelete': return [['tag', '-d', gitName(params.name, 'tag')]];
        case 'remoteAdd': return [['remote', 'add', gitName(params.name, 'remote name'), gitRemoteUrl(params.url)]];
        case 'remoteRemove': return [['remote', 'remove', gitName(params.name, 'remote')]];
        case 'remoteRename': return [['remote', 'rename', gitName(params.from, 'remote'), gitName(params.to, 'new remote name')]];
        default: throw workspaceError('WORKSPACE_GIT_ACTION_INVALID', `Unknown git action: ${action}`);
    }
}

async function workspaceGitAction(rootPath, action, params = {}) {
    const root = canonicalWorkspaceRoot(rootPath);
    const gitRoot = await resolveGitRoot(root);
    if (!gitRoot) throw workspaceError('WORKSPACE_NOT_GIT_REPOSITORY', 'Workspace is not inside a Git repository.');
    const steps = gitActionSteps(action, params || {});
    const output = [];
    for (const args of steps) {
        if (args[0] === 'push' && args.length === 1) {
            try {
                output.push(await runGitLogged(gitRoot, args));
            } catch (error) {
                if (!/no upstream branch|has no upstream/i.test(error?.message || '')) throw error;
                output.push(await runGitLogged(gitRoot, ['push', '--set-upstream', 'origin', 'HEAD']));
            }
        } else {
            output.push(await runGitLogged(gitRoot, args));
        }
    }
    return {
        output: output.filter(Boolean).join('\n'),
        overview: await workspaceGitOverview(root),
        status: await workspaceGitStatus(root),
    };
}

async function workspaceGitRemote(rootPath, action) {
    if (!['pull', 'push', 'fetch', 'sync'].includes(action)) throw workspaceError('WORKSPACE_GIT_REMOTE_INVALID', `Unknown git action: ${action}`);
    return workspaceGitAction(rootPath, action);
}

async function workspaceGitList(rootPath, kind) {
    const root = canonicalWorkspaceRoot(rootPath);
    const gitRoot = await resolveGitRoot(root);
    if (!gitRoot) return [];
    const run = async (args) => (await execGit(args, { cwd: gitRoot, encoding: 'utf8', timeout: 10000, maxBuffer: GIT_MAX_BUFFER })).stdout;
    const lines = (text) => text.split('\n').map((line) => line.trim()).filter(Boolean);
    if (kind === 'branches' || kind === 'localBranches') {
        const refs = kind === 'branches' ? ['refs/heads', 'refs/remotes'] : ['refs/heads'];
        return lines(await run(['for-each-ref', '--sort=-committerdate', '--format=%(refname:short)', ...refs]))
            .filter((name) => !name.endsWith('/HEAD') && name !== 'origin')
            .map((name) => ({ value: name, label: name }));
    }
    if (kind === 'tags') return lines(await run(['tag', '--sort=-creatordate'])).map((name) => ({ value: name, label: name }));
    if (kind === 'stashes') {
        return lines(await run(['stash', 'list', '--format=%gd%x00%s'])).map((line) => {
            const [ref, subject] = line.split('\0');
            return { value: ref, label: `${ref}  ${subject ?? ''}`.trim() };
        });
    }
    if (kind === 'remotes') {
        const seen = new Map();
        for (const line of lines(await run(['remote', '-v']))) {
            const [name, url] = line.split(/\s+/);
            if (name && !seen.has(name)) seen.set(name, url ?? '');
        }
        return [...seen.entries()].map(([name, url]) => ({ value: name, label: url ? `${name}  ${url}` : name }));
    }
    throw workspaceError('WORKSPACE_GIT_LIST_INVALID', `Unknown git list: ${kind}`);
}

async function workspaceGitClone(url, parentDir, folderName) {
    const source = gitRemoteUrl(url);
    const parent = canonicalWorkspaceRoot(parentDir);
    const fallback = source.replace(/[\\/]+$/, '').split(/[\\/:]/).pop().replace(/\.git$/, '');
    const name = typeof folderName === 'string' && folderName.trim() ? folderName.trim() : fallback;
    if (!name || name.startsWith('-') || name.startsWith('.') || /[\\/\0]/.test(name)) throw workspaceError('WORKSPACE_GIT_ARGUMENT_INVALID', 'Invalid folder name.');
    const target = path.join(parent, name);
    if (fs.existsSync(target)) throw workspaceError('WORKSPACE_GIT_CLONE_EXISTS', `${target} already exists.`);
    await runGitLogged(parent, ['clone', '--', source, target], 600000);
    return { path: target };
}

async function workspaceGitOverview(rootPath) {
    const root = canonicalWorkspaceRoot(rootPath);
    const gitRoot = await resolveGitRoot(root);
    if (!gitRoot) return { isRepository: false, root, gitRoot: null, branch: null, upstream: null, ahead: 0, behind: 0, stagedFiles: 0, unstagedFiles: 0 };
    const branch = await execGit(['branch', '--show-current'], { cwd: gitRoot, encoding: 'utf8', timeout: 5000 })
        .then(({ stdout }) => stdout.trim() || null).catch(() => null);
    const upstream = await execGit(['rev-parse', '--abbrev-ref', '--symbolic-full-name', '@{upstream}'], { cwd: gitRoot, encoding: 'utf8', timeout: 5000 })
        .then(({ stdout }) => stdout.trim() || null).catch(() => null);
    let ahead = 0;
    let behind = 0;
    if (upstream) {
        const counts = await execGit(['rev-list', '--left-right', '--count', `HEAD...${upstream}`], { cwd: gitRoot, encoding: 'utf8', timeout: 5000 })
            .then(({ stdout }) => stdout.trim().split(/\s+/).map(Number)).catch(() => [0, 0]);
        ahead = Number.isFinite(counts[0]) ? counts[0] : 0;
        behind = Number.isFinite(counts[1]) ? counts[1] : 0;
    }
    const status = await workspaceGitStatus(root);
    return {
        isRepository: true,
        root,
        gitRoot,
        branch,
        upstream,
        ahead,
        behind,
        stagedFiles: status.filter((entry) => entry.indexStatus.trim() && entry.indexStatus !== '?').length,
        unstagedFiles: status.filter((entry) => entry.worktreeStatus.trim() || entry.indexStatus === '?').length,
    };
}

async function workspaceGitCommit(rootPath, message) {
    const root = canonicalWorkspaceRoot(rootPath);
    const gitRoot = await resolveGitRoot(root);
    if (!gitRoot) throw workspaceError('WORKSPACE_NOT_GIT_REPOSITORY', 'Workspace is not inside a Git repository.');
    const normalizedMessage = typeof message === 'string' ? message.trim() : '';
    if (!normalizedMessage) throw workspaceError('WORKSPACE_COMMIT_MESSAGE_REQUIRED', 'A commit message is required.');
    if (normalizedMessage.length > 4096) throw workspaceError('WORKSPACE_COMMIT_MESSAGE_TOO_LONG', 'Commit message exceeds 4096 characters.');
    const status = await workspaceGitStatus(root);
    if (!status.some((entry) => entry.indexStatus.trim() && entry.indexStatus !== '?')) {
        throw workspaceError('WORKSPACE_NOTHING_STAGED', 'There are no staged changes to commit.');
    }
    await execGit(['commit', '-m', normalizedMessage], {
        cwd: gitRoot, encoding: 'utf8', timeout: 30000, maxBuffer: GIT_MAX_BUFFER,
    }).catch((error) => {
        throw workspaceError('WORKSPACE_GIT_COMMIT_FAILED', error?.stderr?.trim() || error?.message || 'Git commit failed.');
    });
    const { stdout: commit } = await execGit(['rev-parse', 'HEAD'], { cwd: gitRoot, encoding: 'utf8', timeout: 5000 });
    const { stdout: subject } = await execGit(['log', '-1', '--pretty=%s'], { cwd: gitRoot, encoding: 'utf8', timeout: 5000 });
    return { commit: commit.trim(), subject: subject.trim(), overview: await workspaceGitOverview(root), status: await workspaceGitStatus(root) };
}

async function workspaceGitStage(rootPath, relativePath) {
    const root = canonicalWorkspaceRoot(rootPath);
    const gitRoot = await resolveGitRoot(root);
    if (!gitRoot) throw workspaceError('WORKSPACE_NOT_GIT_REPOSITORY', 'Workspace is not inside a Git repository.');
    const resolved = resolveWorkspacePath(root, relativePath, { allowMissing: true });
    await execGit(['add', '--', path.relative(gitRoot, resolved.absolutePath)], {
        cwd: gitRoot, encoding: 'utf8', timeout: 10000, maxBuffer: GIT_MAX_BUFFER,
    });
    return workspaceGitStatus(root);
}

function relativeGitPathsOrThrow(rootPath, relativePaths) {
    const root = canonicalWorkspaceRoot(rootPath);
    if (!Array.isArray(relativePaths) || relativePaths.length === 0) {
        throw workspaceError('WORKSPACE_PATH_INVALID', 'No paths provided.');
    }
    const resolved = relativePaths.map((relativePath) => resolveWorkspacePath(root, relativePath, { allowMissing: true }));
    return { root, resolved };
}

async function workspaceGitStageMany(rootPath, relativePaths) {
    const { root, resolved } = relativeGitPathsOrThrow(rootPath, relativePaths);
    const gitRoot = await resolveGitRoot(root);
    if (!gitRoot) throw workspaceError('WORKSPACE_NOT_GIT_REPOSITORY', 'Workspace is not inside a Git repository.');
    const requested = resolved.map((entry) => path.relative(gitRoot, entry.absolutePath));
    const ignored = await gitIgnoredNames(gitRoot, requested);
    const gitPaths = requested.filter((gitPath) => !ignored.has(gitPath));
    if (gitPaths.length > 0) {
        await execGit(['add', '--', ...gitPaths], {
            cwd: gitRoot, encoding: 'utf8', timeout: 30000, maxBuffer: GIT_MAX_BUFFER,
        });
    }
    return workspaceGitStatus(root);
}

async function workspaceGitUnstage(rootPath, relativePath) {
    const root = canonicalWorkspaceRoot(rootPath);
    const gitRoot = await resolveGitRoot(root);
    if (!gitRoot) throw workspaceError('WORKSPACE_NOT_GIT_REPOSITORY', 'Workspace is not inside a Git repository.');
    const resolved = resolveWorkspacePath(root, relativePath, { allowMissing: true });
    const gitPath = path.relative(gitRoot, resolved.absolutePath);
    await execGit(['restore', '--staged', '--', gitPath], {
        cwd: gitRoot, encoding: 'utf8', timeout: 10000, maxBuffer: GIT_MAX_BUFFER,
    }).catch(async () => {
        await execGit(['reset', 'HEAD', '--', gitPath], {
            cwd: gitRoot, encoding: 'utf8', timeout: 10000, maxBuffer: GIT_MAX_BUFFER,
        });
    });
    return workspaceGitStatus(root);
}

async function workspaceGitUnstageMany(rootPath, relativePaths) {
    const { root, resolved } = relativeGitPathsOrThrow(rootPath, relativePaths);
    const gitRoot = await resolveGitRoot(root);
    if (!gitRoot) throw workspaceError('WORKSPACE_NOT_GIT_REPOSITORY', 'Workspace is not inside a Git repository.');
    const gitPaths = resolved.map((entry) => path.relative(gitRoot, entry.absolutePath));
    await execGit(['restore', '--staged', '--', ...gitPaths], {
        cwd: gitRoot, encoding: 'utf8', timeout: 30000, maxBuffer: GIT_MAX_BUFFER,
    }).catch(async () => {
        await execGit(['reset', 'HEAD', '--', ...gitPaths], {
            cwd: gitRoot, encoding: 'utf8', timeout: 30000, maxBuffer: GIT_MAX_BUFFER,
        });
    });
    return workspaceGitStatus(root);
}

async function workspaceGitDiscard(rootPath, relativePath) {
    const root = canonicalWorkspaceRoot(rootPath);
    const gitRoot = await resolveGitRoot(root);
    if (!gitRoot) throw workspaceError('WORKSPACE_NOT_GIT_REPOSITORY', 'Workspace is not inside a Git repository.');
    const resolved = resolveWorkspacePath(root, relativePath, { allowMissing: true });
    const gitPath = path.relative(gitRoot, resolved.absolutePath);
    const tracked = await execGit(['ls-files', '--error-unmatch', '--', gitPath], {
        cwd: gitRoot, encoding: 'utf8', timeout: 5000,
    }).then(() => true).catch(() => false);
    if (!tracked) {
        throw workspaceError('WORKSPACE_DISCARD_UNTRACKED', 'Untracked files must be deleted explicitly; discard only restores tracked files.');
    }
    await execGit(['restore', '--worktree', '--', gitPath], {
        cwd: gitRoot, encoding: 'utf8', timeout: 10000, maxBuffer: GIT_MAX_BUFFER,
    });
    return workspaceGitStatus(root);
}

function runGitWithInput(args, cwd, input) {
    return new Promise((resolve, reject) => {
        const child = spawn('git', args, { cwd, windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'] });
        const stdout = [];
        const stderr = [];
        child.stdout.on('data', (chunk) => stdout.push(chunk));
        child.stderr.on('data', (chunk) => stderr.push(chunk));
        child.once('error', reject);
        child.once('close', (code) => {
            const output = Buffer.concat(stdout).toString('utf8');
            const errorOutput = Buffer.concat(stderr).toString('utf8');
            if (code === 0) {
                resolve(output);
                return;
            }
            reject(workspaceError('WORKSPACE_GIT_APPLY_FAILED', errorOutput.trim() || `git exited with ${code}`, { exitCode: code }));
        });
        child.stdin.on('error', () => undefined);
        child.stdin.end(input, 'utf8');
    });
}

function parseUnifiedDiffHunks(diff, relativePath, staged) {
    if (typeof diff !== 'string' || !diff.trim()) return [];
    const lines = diff.split('\n');
    const firstHunk = lines.findIndex((line) => line.startsWith('@@ '));
    if (firstHunk < 0) return [];
    const header = lines.slice(0, firstHunk).join('\n');
    const hunks = [];
    let index = firstHunk;
    while (index < lines.length) {
        if (!lines[index].startsWith('@@ ')) {
            index += 1;
            continue;
        }
        const hunkLines = [lines[index]];
        const headerMatch = lines[index].match(/^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@(.*)$/);
        index += 1;
        while (index < lines.length && !lines[index].startsWith('@@ ')) {
            hunkLines.push(lines[index]);
            index += 1;
        }
        while (hunkLines.length > 1 && hunkLines[hunkLines.length - 1] === '') hunkLines.pop();
        const patch = `${header}\n${hunkLines.join('\n')}\n`;
        const body = hunkLines.slice(1);
        const additions = body.filter((line) => line.startsWith('+') && !line.startsWith('+++')).length;
        const deletions = body.filter((line) => line.startsWith('-') && !line.startsWith('---')).length;
        hunks.push({
            id: sha256(Buffer.from(patch, 'utf8')),
            index: hunks.length,
            path: relativePath,
            staged: Boolean(staged),
            header: hunkLines[0],
            section: headerMatch?.[5]?.trim() || '',
            oldStart: Number(headerMatch?.[1] || 0),
            oldLines: Number(headerMatch?.[2] || 1),
            newStart: Number(headerMatch?.[3] || 0),
            newLines: Number(headerMatch?.[4] || 1),
            additions,
            deletions,
            preview: body.slice(0, 12).join('\n'),
            patch,
        });
    }
    return hunks;
}

async function workspaceGitHunks(rootPath, relativePath, options = {}) {
    const diff = await workspaceGitDiff(rootPath, relativePath, { staged: options.staged === true });
    return parseUnifiedDiffHunks(diff, relativePath, options.staged === true);
}

async function workspaceGitApplyHunk(rootPath, relativePath, hunkId, action) {
    const root = canonicalWorkspaceRoot(rootPath);
    const gitRoot = await resolveGitRoot(root);
    if (!gitRoot) throw workspaceError('WORKSPACE_NOT_GIT_REPOSITORY', 'Workspace is not inside a Git repository.');
    resolveWorkspacePath(root, relativePath, { allowMissing: true });
    if (!['stage', 'unstage', 'discard'].includes(action)) {
        throw workspaceError('WORKSPACE_HUNK_ACTION_INVALID', `Unsupported hunk action: ${action}`);
    }
    const staged = action === 'unstage';
    const hunks = await workspaceGitHunks(root, relativePath, { staged });
    const hunk = hunks.find((candidate) => candidate.id === hunkId);
    if (!hunk) {
        throw workspaceError('WORKSPACE_HUNK_STALE', 'The selected hunk no longer matches the current Git diff. Refresh and try again.');
    }
    const args = ['apply', '--recount', '--whitespace=nowarn'];
    if (action === 'stage' || action === 'unstage') args.push('--cached');
    if (action === 'unstage' || action === 'discard') args.push('--reverse');
    args.push('-');
    await runGitWithInput(args, gitRoot, hunk.patch);
    return {
        status: await workspaceGitStatus(root),
        hunks: await workspaceGitHunks(root, relativePath, { staged }),
    };
}

function gitPathFromWorkspace(gitRoot, absolutePath) {
    return path.relative(gitRoot, absolutePath).split(path.sep).join('/');
}

async function gitStdout(args, cwd) {
    const { stdout } = await execGit(args, {
        cwd, encoding: 'utf8', timeout: 10000, maxBuffer: GIT_MAX_BUFFER,
    }).catch((error) => ({ stdout: typeof error?.stdout === 'string' ? error.stdout : '' }));
    return stdout;
}

async function workspaceGitShow(rootPath, relativePath, revision = 'HEAD') {
    const root = canonicalWorkspaceRoot(rootPath);
    const gitRoot = await resolveGitRoot(root);
    if (!gitRoot || !relativePath) return '';
    const resolved = resolveWorkspacePath(root, relativePath, { allowMissing: true });
    return gitStdout(['show', `${revision}:${gitPathFromWorkspace(gitRoot, resolved.absolutePath)}`], gitRoot);
}

async function workspaceGitDiff(rootPath, relativePath = null, options = {}) {
    const root = canonicalWorkspaceRoot(rootPath);
    const gitRoot = await resolveGitRoot(root);
    if (!gitRoot) return '';
    const args = ['diff', '--no-ext-diff', '--no-color'];
    if (options.againstHead === true || options.againstHead === true) args.push('HEAD');
    else if (options.staged === true) args.push('--cached');
    if (relativePath) {
        const resolved = resolveWorkspacePath(root, relativePath, { allowMissing: true });
        args.push('--', gitPathFromWorkspace(gitRoot, resolved.absolutePath));
    }
    let stdout = await gitStdout(args, gitRoot);
    if (!stdout && relativePath && (options.includeUntracked === true || options.includeUntracked === true)) {
        const statuses = await workspaceGitStatus(root);
        const resolved = resolveWorkspacePath(root, relativePath, { allowMissing: true });
        const relative = gitPathFromWorkspace(root, resolved.absolutePath);
        const match = statuses.find((entry) => {
            const statusPath = String(entry.path || '').split(path.sep).join('/');
            return statusPath === relative || statusPath.endsWith(`/${relative}`) || relative.endsWith(`/${statusPath}`);
        });
        if (match && (match.worktreeStatus === '?' || match.indexStatus === '?')) {
            stdout = await gitStdout(
                ['diff', '--no-index', '--no-ext-diff', '--no-color', '--', '/dev/null', resolved.absolutePath],
                gitRoot,
            );
        }
    }
    return stdout;
}

module.exports = {
    runGitWithInput,
    canonicalWorkspaceRoot,
    createWorkspaceDirectory,
    deleteWorkspacePath,
    isWithinRoot,
    listWorkspaceDirectory,
    openWorkspace,
    readWorkspaceFile,
    renameWorkspacePath,
    resolveWorkspacePath,
    searchWorkspace,
    statWorkspaceFile,
    sha256,
    parseGitWorktreeList,
    parseUnifiedDiffHunks,
    workspaceGitApplyHunk,
    workspaceGitBranches,
    workspaceGitCheckout,
    workspaceGitRemote,
    workspaceGitAction,
    workspaceGitList,
    workspaceGitClone,
    workspaceGitOutput,
    gitActionSteps,
    changesRepository,
    workspaceGitCommit,
    workspaceGitCommitDetail,
    workspaceGitConflicts,
    workspaceGitCreateWorktree,
    workspaceGitDiff,
    workspaceGitDiscard,
    workspaceGitShow,
    workspaceGitHunks,
    workspaceGitHistory,
    workspaceGitIntegrateWorktree,
    workspaceGitListWorktrees,
    workspaceGitOverview,
    workspaceGitRemoveWorktree,
    workspaceGitReviewWorktree,
    workspaceGitStage,
    workspaceGitStageMany,
    workspaceGitStatus,
    workspaceGitUnstage,
    workspaceGitUnstageMany,
    writeWorkspaceFile,
};
