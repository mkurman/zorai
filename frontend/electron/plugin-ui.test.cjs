const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('fs');
const os = require('os');
const path = require('path');
const { PANEL_CONTENT_SECURITY_POLICY, approveLocalRun, normalizePluginUi, pluginSchemePrivileges, localEndpointTimeoutSecs, localRunStatus, pluginFileFromUrl, readPluginManifestByName, readPluginUiManifests, revokeLocalRun } = require('./main/plugin-ui.cjs');

function pluginsRoot(plugins) {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'zorai-plugin-ui-'));
    for (const [dir, files] of Object.entries(plugins)) {
        fs.mkdirSync(path.join(root, dir), { recursive: true });
        for (const [file, content] of Object.entries(files)) {
            fs.mkdirSync(path.dirname(path.join(root, dir, file)), { recursive: true });
            fs.writeFileSync(path.join(root, dir, file), typeof content === 'string' ? content : JSON.stringify(content));
        }
    }
    return root;
}

const api = { endpoints: { get_repo: {}, list_issues: {} } };

test('plugins without a ui section add nothing, and declared commands and panels are normalised', () => {
    const root = pluginsRoot({
        plain: { 'plugin.json': { name: 'plain', version: '1', api } },
        github: {
            'plugin.json': { name: 'github', version: '1.2.0', api, ui: {
                commands: [{ id: 'repo', title: 'Repo info', endpoint: 'get_repo', inputs: [{ key: 'owner' }, { key: 'bad key' }], output: 'weird', placement: ['palette', 'composer', 'rail'] }],
                panels: [
                    { id: 'issues', title: 'Open issues', endpoint: 'list_issues', params: { state: 'open' } },
                    { id: 'board', title: 'Board', web: 'panel/index.html', permissions: ['composer', 'filesystem', 'terminal'] },
                ],
            } },
            'panel/index.html': '<p>hi</p>',
        },
    });
    const [github] = readPluginUiManifests(root);
    assert.equal(readPluginUiManifests(root).length, 1);
    assert.deepEqual(github.problems, []);
    assert.deepEqual(github.commands[0], { id: 'repo', title: 'Repo info', endpoint: 'get_repo', prompt: null, params: {}, inputs: [{ key: 'owner', label: 'owner', placeholder: '', default: '' }], output: 'panel', placement: ['palette', 'composer'] });
    assert.deepEqual(github.panels[0], { id: 'issues', title: 'Open issues', kind: 'markdown', endpoint: 'list_issues', params: { state: 'open' }, inputs: [] });
    assert.deepEqual(github.panels[1], { id: 'board', title: 'Board', kind: 'web', url: 'zorai-plugin://github/panel/index.html', permissions: ['composer', 'terminal'] });
});

test('python plugins without endpoints can offer prompt commands, which always fill the message box', () => {
    const root = pluginsRoot({ pubmed: { 'plugin.json': { name: 'pubmed-database', version: '1', ui: { commands: [
        { id: 'ask', title: 'Ask with PubMed', prompt: 'Use the pubmed-database plugin to: {{request}}', inputs: [{ key: 'request', label: 'Request' }], output: 'toast' },
        { id: 'empty', title: 'Empty', prompt: '   ' },
    ] } } } });
    const [pubmed] = readPluginUiManifests(root);
    assert.equal(pubmed.commands.length, 1);
    assert.deepEqual({ endpoint: pubmed.commands[0].endpoint, prompt: pubmed.commands[0].prompt, output: pubmed.commands[0].output }, { endpoint: '', prompt: 'Use the pubmed-database plugin to: {{request}}', output: 'composer' });
    assert.equal(pubmed.problems.length, 1);
});

test('a ui entry calling an endpoint the plugin does not define, or a web panel outside its folder, is dropped and reported', () => {
    const root = pluginsRoot({
        sneaky: { 'plugin.json': { name: 'sneaky', version: '1', api, ui: {
            commands: [{ id: 'x', title: 'X', endpoint: 'delete_everything' }],
            panels: [{ id: 'p', title: 'P', web: '../other/index.html' }, { id: 'q', title: 'Q', web: 'missing.html' }],
        } } },
        broken: { 'plugin.json': '{ nope' },
    });
    const byName = Object.fromEntries(readPluginUiManifests(root).map((plugin) => [plugin.name, plugin]));
    assert.deepEqual(byName.sneaky.commands, []);
    assert.deepEqual(byName.sneaky.panels, []);
    assert.equal(byName.sneaky.problems.length, 3);
    assert.match(byName.broken.problems[0], /Could not read plugin.json/);
});

test('the plugin file protocol only serves known file types from inside that plugin folder', () => {
    const root = pluginsRoot({ github: { 'plugin.json': '{}', 'panel/index.html': 'x', 'panel/app.js': 'x', 'secret.env': 'x' } });
    assert.equal(pluginFileFromUrl(root, 'zorai-plugin://github/panel/index.html'), path.join(root, 'github', 'panel', 'index.html'));
    assert.equal(pluginFileFromUrl(root, 'zorai-plugin://github/panel/app.js'), path.join(root, 'github', 'panel', 'app.js'));
    assert.equal(pluginFileFromUrl(root, 'zorai-plugin://github/secret.env'), null);
    for (const url of ['zorai-plugin://github/%2e%2e/%2e%2e/etc/passwd.html', 'zorai-plugin://github/..%2f..%2fetc/passwd.html', 'zorai-plugin://github/a/../../../x.html']) {
        const file = pluginFileFromUrl(root, url);
        assert.ok(file === null || file.startsWith(path.join(root, 'github') + path.sep), `${url} resolved outside the plugin: ${file}`);
    }
    assert.equal(pluginFileFromUrl(root, 'zorai-plugin://%2e%2e/github/panel/index.html'), null);
    assert.equal(pluginFileFromUrl(root, 'zorai-media://github/panel/index.html'), null);
});

test('web panels may not reach the network or embed other pages; data only flows through zorai', () => {
    assert.match(PANEL_CONTENT_SECURITY_POLICY, /connect-src 'none'/);
    assert.match(PANEL_CONTENT_SECURITY_POLICY, /default-src 'none'/);
    assert.match(PANEL_CONTENT_SECURITY_POLICY, /frame-src 'none'/);
    assert.doesNotMatch(PANEL_CONTENT_SECURITY_POLICY, /https?:/);
});

test('a plugin icon is an image file from its own folder or a short glyph; anything else is reported', () => {
    const root = pluginsRoot({
        pics: { 'plugin.json': { name: 'pics', version: '1', ui: { icon: 'assets/icon.svg' } }, 'assets/icon.svg': '<svg/>' },
        emoji: { 'plugin.json': { name: 'emoji', version: '1', ui: { icon: '🧬' } } },
        bad: { 'plugin.json': { name: 'bad', version: '1', ui: { icon: '../../evil.svg' } } },
        long: { 'plugin.json': { name: 'long', version: '1', ui: { icon: 'not an icon' } } },
    });
    const byName = Object.fromEntries(readPluginUiManifests(root).map((plugin) => [plugin.name, plugin]));
    assert.deepEqual(byName.pics.icon, { kind: 'image', url: 'zorai-plugin://pics/assets/icon.svg' });
    assert.deepEqual(byName.emoji.icon, { kind: 'glyph', text: '🧬' });
    assert.equal(byName.bad.icon, null);
    assert.match(byName.bad.problems[0], /not a file inside the plugin folder/);
    assert.equal(byName.long.icon, null);
    assert.equal(byName.long.problems.length, 1);
});

test('approving local programs writes the exact file and version the daemon checks, and revoking removes it', () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'zorai-local-run-'));
    const pluginsRoot = path.join(root, 'plugins');
    fs.mkdirSync(path.join(pluginsRoot, 'voice-lab'), { recursive: true });
    fs.writeFileSync(path.join(pluginsRoot, 'voice-lab', 'plugin.json'), JSON.stringify({
        name: 'voice-lab', version: '0.2.0', schema_version: 1,
        api: { endpoints: { render: { run: { command: 'python r.py', timeout_secs: 900 } }, quick: { run: { command: 'python q.py' } }, web: { method: 'GET', path: '/x' } } },
    }));

    assert.deepEqual(localRunStatus(pluginsRoot, 'voice-lab'), { local: true, version: '0.2.0', approvedVersion: null });
    approveLocalRun(pluginsRoot, 'voice-lab');
    assert.equal(fs.readFileSync(path.join(root, 'plugin-data', 'voice-lab', 'local-run-approved'), 'utf8').trim(), '0.2.0');
    assert.equal(revokeLocalRun(pluginsRoot, 'voice-lab').approvedVersion, null);

    const manifest = readPluginManifestByName(pluginsRoot, 'voice-lab');
    assert.equal(localEndpointTimeoutSecs(manifest, 'render'), 900);
    assert.equal(localEndpointTimeoutSecs(manifest, 'quick'), 120);
    assert.equal(localEndpointTimeoutSecs(manifest, 'web'), null);
});

test('a panel can play files from its own plugin data folder and nothing outside it', () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'zorai-plugin-data-'));
    const pluginsRoot = path.join(root, 'plugins');
    fs.mkdirSync(path.join(pluginsRoot, 'zorai-plugin-voice-lab'), { recursive: true });
    fs.writeFileSync(path.join(pluginsRoot, 'zorai-plugin-voice-lab', 'plugin.json'), JSON.stringify({ name: 'voice-lab', version: '1', schema_version: 1 }));
    const own = path.join(root, 'plugin-data', 'voice-lab', 'projects', 'demo', 'mix.mp3');
    fs.mkdirSync(path.dirname(own), { recursive: true });
    fs.writeFileSync(own, 'x');

    assert.equal(pluginFileFromUrl(pluginsRoot, 'zorai-plugin://zorai-plugin-voice-lab/.data/projects/demo/mix.mp3'), own);
    const allowedRoots = [path.join(pluginsRoot, 'zorai-plugin-voice-lab') + path.sep, path.join(root, 'plugin-data', 'voice-lab') + path.sep];
    for (const url of [
        'zorai-plugin://zorai-plugin-voice-lab/.data/../other/secret.mp3',
        'zorai-plugin://zorai-plugin-voice-lab/.data/%2E%2E/other/secret.mp3',
        'zorai-plugin://zorai-plugin-voice-lab/.data/%2E%2E%2F%2E%2E%2Fgithub/notes.txt',
    ]) {
        const resolved = pluginFileFromUrl(pluginsRoot, url);
        assert.ok(resolved === null || allowedRoots.some((allowed) => resolved.startsWith(allowed)), `${url} resolved to ${resolved}`);
    }
    assert.equal(pluginFileFromUrl(pluginsRoot, 'zorai-plugin://zorai-plugin-voice-lab/.data/models/kokoro.onnx'), null);
});

test('the plugin scheme may stream media, so panels can play the audio and video they render', () => {
    assert.equal(pluginSchemePrivileges().privileges.stream, true);
});

test('endpoints that change data are listed so panels need the write permission to use them', () => {
    const ui = normalizePluginUi({
        api: { endpoints: { list: { method: 'GET', path: '/a' }, trash: { method: 'POST', path: '/b' }, remove: { method: 'delete', path: '/c' }, render: { run: { command: 'python r.py' } } } },
        ui: { commands: [] },
    }, os.tmpdir(), 'x');
    assert.deepEqual(ui.writeEndpoints.sort(), ['remove', 'trash']);
});
