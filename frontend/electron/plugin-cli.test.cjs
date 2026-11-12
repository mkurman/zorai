const test = require('node:test');
const assert = require('node:assert/strict');
const { pluginSourceSpec, runPluginCli } = require('./main/plugin-cli.cjs');

test('updates reinstall from the source the plugin was installed from', () => {
    assert.deepEqual(pluginSourceSpec('npm:zorai-plugin-github'), { spec: 'zorai-plugin-github' });
    assert.deepEqual(pluginSourceSpec('github:mkurman/zorai-plugins'), { spec: 'github:mkurman/zorai-plugins' });
    assert.deepEqual(pluginSourceSpec('local:/home/me/plugins/jira', () => true), { spec: '/home/me/plugins/jira' });
});

test('plugins without a usable source explain why they cannot be updated instead of failing later', () => {
    assert.match(pluginSourceSpec('local').reason, /no record of where it was installed from/);
    assert.match(pluginSourceSpec('').reason, /no record/);
    assert.match(pluginSourceSpec('local:/gone/cmux-next/huggingface', () => false).reason, /no longer exists/);
    assert.match(pluginSourceSpec('npm:').reason, /missing/);
});

test('the CLI runs with an argument list, never a shell string, and reports its output', async () => {
    const calls = [];
    const execFileImpl = (file, args, options, callback) => { calls.push([file, args]); callback(null, 'Plugin installed\n', ''); };
    assert.deepEqual(await runPluginCli(execFileImpl, '/bin/zorai', ['plugin', 'add', 'pkg; rm -rf ~']), { ok: true, output: 'Plugin installed' });
    assert.deepEqual(calls, [['/bin/zorai', ['plugin', 'add', 'pkg; rm -rf ~']]]);
    const failing = (file, args, options, callback) => callback(new Error('exit 1'), '', 'npm ERR! 404');
    assert.deepEqual(await runPluginCli(failing, '/bin/zorai', ['plugin', 'add', 'nope']), { ok: false, output: 'npm ERR! 404' });
    assert.equal((await runPluginCli(execFileImpl, null, [])).ok, false);
});
