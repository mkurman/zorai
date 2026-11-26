const { spawn, spawnSync } = require('child_process');

const IGNORED_DIRECTORIES = ['.git', 'node_modules', 'target', 'dist', 'build', '.venv', '__pycache__', '.next', '.cache'];
let rgCommand;

function ripgrepCommand() {
    if (rgCommand !== undefined) return rgCommand;
    const probe = spawnSync('rg', ['--version'], { encoding: 'utf8', timeout: 3000, windowsHide: true });
    rgCommand = probe.status === 0 ? 'rg' : null;
    return rgCommand;
}

function globList(patterns) {
    return String(patterns ?? '').split(',').map((glob) => glob.trim().replace(/\\/g, '/').replace(/^\.\//, '')).filter(Boolean);
}

function ripgrepArgs(query, options = {}) {
    const args = ['--json', '--max-filesize', '1M', '--max-columns', '1000', '--no-messages'];
    if (options.regex !== true) args.push('--fixed-strings');
    if (options.caseSensitive !== true) args.push('--ignore-case');
    if (options.wholeWord === true) args.push('--word-regexp');
    if (/\n|\\n/.test(query)) args.push('--multiline');
    if (options.useIgnoreFiles === false) args.push('--no-ignore', '--hidden');
    for (const name of IGNORED_DIRECTORIES) args.push('--glob', `!**/${name}/**`);
    for (const glob of globList(options.include)) args.push('--glob', glob);
    for (const glob of globList(options.exclude)) args.push('--glob', `!${glob}`);
    const pattern = options.regex === true ? query : query.replace(/\r?\n/g, '\n');
    args.push('--', pattern, '.');
    return args;
}

function utf16Column(text, byteOffset) {
    return Buffer.from(text, 'utf8').subarray(0, byteOffset).toString('utf8').length;
}

function ripgrepMatchesFromEvent(event) {
    if (event?.type !== 'match') return [];
    const data = event.data;
    const relativePath = String(data.path?.text ?? '').replace(/^\.\//, '');
    const fullText = String(data.lines?.text ?? '');
    const firstLine = fullText.split(/\r?\n/)[0];
    return (data.submatches ?? []).map((submatch) => {
        const column = utf16Column(fullText, submatch.start);
        const matchText = String(submatch.match?.text ?? '');
        const indent = firstLine.length - firstLine.trimStart().length;
        const previewStart = column - indent > 80 ? column - 40 : Math.min(indent, column);
        const preview = firstLine.slice(previewStart, previewStart + 240).trimEnd();
        const matchStart = Math.min(column - previewStart, preview.length);
        const lineOffset = fullText.slice(0, column).split('\n').length - 1;
        return {
            path: relativePath,
            line: data.line_number + lineOffset,
            column: column - (fullText.lastIndexOf('\n', column - 1) + 1) + 1,
            preview,
            matchStart,
            matchLength: Math.max(0, Math.min(matchText.split(/\r?\n/)[0].length, preview.length - matchStart)),
            ...(matchText.includes('\n') ? { endLine: data.line_number + lineOffset + matchText.split('\n').length - 1 } : {}),
        };
    });
}

function ripgrepSearch(root, query, options = {}, onBatch = null) {
    const command = ripgrepCommand();
    if (!command) return Promise.resolve(null);
    const maxResults = Math.max(1, Math.min(Number(options.maxResults) || 100, 5000));
    return new Promise((resolve) => {
        const child = spawn(command, ripgrepArgs(query, options), { cwd: root, windowsHide: true });
        const results = [];
        let buffer = '';
        let failed = false;
        let pendingBatch = [];
        const flushBatch = () => {
            if (onBatch && pendingBatch.length > 0) onBatch(pendingBatch);
            pendingBatch = [];
        };
        child.stdout.on('data', (chunk) => {
            buffer += chunk.toString('utf8');
            let newline = buffer.indexOf('\n');
            while (newline !== -1) {
                const line = buffer.slice(0, newline);
                buffer = buffer.slice(newline + 1);
                newline = buffer.indexOf('\n');
                let event;
                try { event = JSON.parse(line); } catch { continue; }
                for (const match of ripgrepMatchesFromEvent(event)) {
                    if (results.length >= maxResults) break;
                    results.push(match);
                    pendingBatch.push(match);
                }
                if (pendingBatch.length >= 100) flushBatch();
                if (results.length >= maxResults) {
                    child.kill();
                    return;
                }
            }
        });
        child.once('error', () => { failed = true; });
        child.once('close', (code) => {
            flushBatch();
            resolve(failed || (code !== 0 && code !== 1 && code !== null && results.length === 0) ? null : results);
        });
    });
}

module.exports = { ripgrepArgs, ripgrepCommand, ripgrepMatchesFromEvent, ripgrepSearch };
