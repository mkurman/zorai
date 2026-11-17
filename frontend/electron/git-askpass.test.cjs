const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { execFile } = require('node:child_process');
const { once } = require('node:events');
const { startGitAskpass, gitAskpassEnv } = require('./main/git-askpass.cjs');

function runHelper(env, prompt) {
    return new Promise((resolve) => {
        execFile(env.GIT_ASKPASS, [prompt], { env: { ...process.env, ...env } }, (error, stdout) => {
            resolve({ code: error ? error.code ?? 1 : 0, stdout });
        });
    });
}

test('git and ssh prompts are answered by the app instead of failing for lack of a terminal', { skip: process.platform === 'win32' }, async () => {
    const baseDir = fs.mkdtempSync(path.join(os.tmpdir(), 'askpass-test-'));
    const prompts = [];
    const askpass = startGitAskpass({
        baseDir,
        runtimePath: process.execPath,
        requestPrompt: async (prompt) => {
            prompts.push(prompt);
            return prompt.startsWith('Enter passphrase') ? 'correct horse' : null;
        },
    });
    try {
        if (!askpass.server.listening) await once(askpass.server, 'listening');
        const env = gitAskpassEnv();
        assert.equal(env.SSH_ASKPASS, env.GIT_ASKPASS);
        assert.equal(env.SSH_ASKPASS_REQUIRE, 'force');

        const answered = await runHelper(env, "Enter passphrase for key '/home/me/.ssh/id_ed25519': ");
        assert.equal(answered.code, 0);
        assert.equal(answered.stdout, 'correct horse\n');

        const cancelled = await runHelper(env, 'Username for https://example.com: ');
        assert.notEqual(cancelled.code, 0, 'a cancelled prompt must fail the git command, not send an empty secret');

        const forged = await runHelper({ ...env, ZORAI_ASKPASS_TOKEN: 'wrong' }, "Enter passphrase for key 'x': ");
        assert.notEqual(forged.code, 0, 'callers without the token must not reach the prompt');
        assert.equal(prompts.length, 2);
    } finally {
        askpass.stop();
        fs.rmSync(baseDir, { recursive: true, force: true });
    }
    assert.deepEqual(gitAskpassEnv(), {});
});
