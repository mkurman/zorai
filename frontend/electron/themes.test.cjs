const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('fs');
const os = require('os');
const path = require('path');
const { createThemeStore, seeThroughWindowOptions } = require('./main/themes.cjs');

function tempStore() {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'zorai-themes-'));
    return { root, store: createThemeStore({ rootDir: () => root }) };
}

const theme = (name, extra = {}) => ({ name, appearance: 'dark', base: 'Nord', ...extra });

test('themes dropped into ~/.zorai/themes are discovered as folders or loose json files, and broken ones are reported instead of hidden', () => {
    const { root, store } = tempStore();
    const themes = path.join(root, 'themes');
    fs.mkdirSync(path.join(themes, 'ocean'), { recursive: true });
    fs.writeFileSync(path.join(themes, 'ocean', 'theme.json'), JSON.stringify(theme('Ocean', { background: { image: 'bg.png', blur: 8 } })));
    fs.writeFileSync(path.join(themes, 'ocean', 'bg.png'), 'png');
    fs.writeFileSync(path.join(themes, 'ocean', 'theme.css'), '.x { color: red; }');
    fs.writeFileSync(path.join(themes, 'loose.json'), JSON.stringify(theme('Loose')));
    fs.writeFileSync(path.join(themes, 'broken.json'), '{ nope');
    fs.writeFileSync(path.join(themes, 'notes.txt'), 'ignored');

    const records = Object.fromEntries(store.list().map((record) => [record.id, record]));
    assert.deepEqual(Object.keys(records).sort(), ['broken.json', 'loose.json', 'ocean']);
    assert.equal(records.ocean.backgroundPath, path.join(themes, 'ocean', 'bg.png'));
    assert.equal(records.ocean.css, '.x { color: red; }');
    assert.equal(records['loose.json'].theme.name, 'Loose');
    assert.match(records['broken.json'].error, /Could not read/);
});

test('a background path in theme.json cannot reach outside the theme folder', () => {
    const { root, store } = tempStore();
    fs.mkdirSync(path.join(root, 'themes', 'sneaky'), { recursive: true });
    fs.writeFileSync(path.join(root, 'themes', 'sneaky', 'theme.json'), JSON.stringify(theme('Sneaky', { background: { image: '../../secret.png' } })));
    fs.writeFileSync(path.join(root, 'secret.png'), 'x');
    const [record] = store.list();
    assert.equal(record.backgroundPath, null);
    assert.match(record.error, /not found/);
});

test('saving copies the chosen background into the theme folder so the theme stays portable', () => {
    const { root, store } = tempStore();
    const image = path.join(root, 'wallpaper.JPG');
    fs.writeFileSync(image, 'jpg');
    const id = store.save({ theme: theme('My Night!'), css: 'body{}', backgroundSource: image });
    assert.equal(id, 'my-night');
    const dir = path.join(root, 'themes', id);
    assert.equal(fs.readFileSync(path.join(dir, 'background.jpg'), 'utf8'), 'jpg');
    assert.equal(JSON.parse(fs.readFileSync(path.join(dir, 'theme.json'), 'utf8')).background.image, 'background.jpg');
    assert.equal(store.save({ theme: theme('My Night!') }), 'my-night-2');

    store.save({ id, theme: { ...theme('My Night!'), background: { image: 'background.jpg', blur: 4 } }, css: '', backgroundSource: null });
    const saved = JSON.parse(fs.readFileSync(path.join(dir, 'theme.json'), 'utf8'));
    assert.equal(saved.background.image, undefined);
    assert.equal(saved.background.blur, 4);
    assert.equal(fs.existsSync(path.join(dir, 'theme.css')), false);
});

test('editing a loose json theme turns it into a folder and removes the old file, so it is not listed twice', () => {
    const { root, store } = tempStore();
    fs.mkdirSync(path.join(root, 'themes'), { recursive: true });
    fs.writeFileSync(path.join(root, 'themes', 'loose.json'), JSON.stringify(theme('Loose')));
    store.save({ id: 'loose.json', theme: theme('Loose', { surfaceOpacity: 0.8 }) });
    assert.deepEqual(store.list().map((record) => record.id), ['loose']);
});

test('saving refuses ids that escape the themes folder', () => {
    const { store } = tempStore();
    assert.throws(() => store.save({ id: '../outside', theme: theme('X') }), /Unknown theme/);
});

test('import copies the whole theme folder including image and css, export copies it back out', () => {
    const { root, store } = tempStore();
    const source = path.join(root, 'download', 'sunset');
    fs.mkdirSync(source, { recursive: true });
    fs.writeFileSync(path.join(source, 'theme.json'), JSON.stringify(theme('Sunset', { background: { image: 'bg.webp' } })));
    fs.writeFileSync(path.join(source, 'bg.webp'), 'w');
    fs.writeFileSync(path.join(source, 'theme.css'), 'a{}');
    const imported = store.importFrom(path.join(source, 'theme.json'));
    assert.equal(imported.id, 'sunset');
    assert.equal(imported.css, 'a{}');
    assert.ok(imported.backgroundPath.endsWith(path.join('themes', 'sunset', 'bg.webp')));

    const out = path.join(root, 'out');
    fs.mkdirSync(out);
    const exported = store.exportTo('sunset', out);
    assert.equal(fs.readFileSync(path.join(exported, 'bg.webp'), 'utf8'), 'w');
    assert.throws(() => store.importFrom(path.join(root, 'nothing.json')));
});

test('the see-through window choice persists for the next launch and uses each platform\'s native material', () => {
    const { store } = tempStore();
    assert.deepEqual(store.readWindowAppearance(), { seeThrough: false });
    store.writeWindowAppearance({ seeThrough: true });
    assert.deepEqual(store.readWindowAppearance(), { seeThrough: true });
    assert.equal(seeThroughWindowOptions('darwin').vibrancy, 'under-window');
    assert.equal(seeThroughWindowOptions('win32').backgroundMaterial, 'acrylic');
    assert.equal(seeThroughWindowOptions('linux').transparent, true);
});

test('the personal custom.css is optional and created empty only when the user asks to edit it', () => {
    const { root, store } = tempStore();
    assert.equal(store.readCustomCss(), null);
    assert.equal(store.ensureCustomCss(), path.join(root, 'custom.css'));
    assert.equal(store.readCustomCss(), '');
    fs.writeFileSync(path.join(root, 'custom.css'), '.zorai-shell { gap: 0; }');
    store.ensureCustomCss();
    assert.equal(store.readCustomCss(), '.zorai-shell { gap: 0; }');
});

test('a looping video can be the background, and switching back to an image removes the old video file', () => {
    const { root, store } = tempStore();
    const video = path.join(root, 'loop.WEBM');
    const image = path.join(root, 'still.png');
    fs.writeFileSync(video, 'v');
    fs.writeFileSync(image, 'i');
    const id = store.save({ theme: theme('Motion'), backgroundSource: video });
    const dir = path.join(root, 'themes', id);
    assert.equal(fs.existsSync(path.join(dir, 'background.webm')), true);
    store.save({ id, theme: theme('Motion'), backgroundSource: image });
    assert.equal(fs.existsSync(path.join(dir, 'background.webm')), false);
    assert.equal(JSON.parse(fs.readFileSync(path.join(dir, 'theme.json'), 'utf8')).background.image, 'background.png');
    fs.writeFileSync(path.join(root, 'clip.avi'), 'x');
    assert.throws(() => store.save({ id, theme: theme('Motion'), backgroundSource: path.join(root, 'clip.avi') }), /video \(MP4, WebM\)/);
});

const { normalizeGalleryIndex, normalizeThemeUrl, stageRemoteTheme } = require('./main/themes.cjs');

function fakeFetch(files) {
    const calls = [];
    const impl = async (url) => {
        calls.push(url);
        const body = files[url];
        if (body === undefined) return { ok: false, status: 404, url, headers: new Map() };
        const buffer = Buffer.from(body);
        return { ok: true, status: 200, url, headers: new Map([['content-length', String(buffer.length)]]), arrayBuffer: async () => buffer };
    };
    impl.calls = calls;
    return impl;
}

test('GitHub file and folder links become raw downloads, and plain http is refused', () => {
    assert.equal(normalizeThemeUrl('https://github.com/me/themes/blob/main/ocean/theme.json'), 'https://raw.githubusercontent.com/me/themes/main/ocean/theme.json');
    assert.equal(normalizeThemeUrl('https://github.com/me/themes/tree/main/ocean'), 'https://raw.githubusercontent.com/me/themes/main/ocean/theme.json');
    assert.equal(normalizeThemeUrl('https://example.com/themes/ocean/'), 'https://example.com/themes/ocean/theme.json');
    assert.throws(() => normalizeThemeUrl('http://example.com/theme.json'), /https/);
    assert.throws(() => normalizeThemeUrl('not a url'), /valid web address/);
});

test('a remote theme is staged with its background and css, then installed only on request', async () => {
    const { root, store } = tempStore();
    const base = 'https://example.com/ocean/';
    const fetchImpl = fakeFetch({
        [`${base}theme.json`]: JSON.stringify(theme('Ocean', { background: { image: 'bg.png', blur: 4 } })),
        [`${base}bg.png`]: 'png',
        [`${base}theme.css`]: '.x{}',
    });
    const staging = fs.mkdtempSync(path.join(root, 'staging-'));
    const result = await stageRemoteTheme({ url: base, fetchImpl, stagingRoot: staging });
    assert.equal(result.record.theme.name, 'Ocean');
    assert.equal(result.record.css, '.x{}');
    assert.ok(result.record.backgroundPath.endsWith('bg.png'));
    assert.deepEqual(store.list(), []);
    const installed = store.installStaged(result.dir);
    assert.equal(installed.id, 'ocean');
    assert.equal(fs.readFileSync(path.join(root, 'themes', 'ocean', 'bg.png'), 'utf8'), 'png');
});

test('a missing theme.css is fine, but a theme pointing its background outside its folder is rejected and leaves nothing behind', async () => {
    const { root } = tempStore();
    const staging = fs.mkdtempSync(path.join(root, 'staging-'));
    const ok = await stageRemoteTheme({ url: 'https://example.com/plain/theme.json', fetchImpl: fakeFetch({ 'https://example.com/plain/theme.json': JSON.stringify(theme('Plain')) }), stagingRoot: staging });
    assert.equal(ok.record.css, null);
    const sneaky = fakeFetch({ 'https://example.com/s/theme.json': JSON.stringify(theme('Sneaky', { background: { image: '../../etc/x.png' } })) });
    await assert.rejects(stageRemoteTheme({ url: 'https://example.com/s/theme.json', fetchImpl: sneaky, stagingRoot: staging }), /next to theme.json/);
    assert.equal(fs.readdirSync(staging).length, 1);
});

test('oversized downloads and invalid themes are refused before anything is written', async () => {
    const { root } = tempStore();
    const staging = fs.mkdtempSync(path.join(root, 'staging-'));
    const huge = fakeFetch({ 'https://example.com/h/theme.json': 'x'.repeat(1024 * 1024 + 1) });
    await assert.rejects(stageRemoteTheme({ url: 'https://example.com/h/theme.json', fetchImpl: huge, stagingRoot: staging }), /limit/);
    const bad = fakeFetch({ 'https://example.com/b/theme.json': JSON.stringify({ name: 'No mode' }) });
    await assert.rejects(stageRemoteTheme({ url: 'https://example.com/b/theme.json', fetchImpl: bad, stagingRoot: staging }), /appearance/);
    assert.equal(fs.readdirSync(staging).length, 0);
});

test('a gallery index lists themes with links resolved against the index address and skips malformed entries', () => {
    const entries = normalizeGalleryIndex({ themes: [
        { name: 'Ocean', url: 'ocean/theme.json', author: 'me', appearance: 'dark' },
        { name: 'Broken' },
        { name: 'Sun', url: 'https://cdn.example.com/sun/theme.json', appearance: 'weird' },
    ] }, 'https://example.com/gallery/index.json');
    assert.deepEqual(entries.map((entry) => [entry.name, entry.url, entry.appearance]), [
        ['Ocean', 'https://example.com/gallery/ocean/theme.json', 'dark'],
        ['Sun', 'https://cdn.example.com/sun/theme.json', null],
    ]);
    assert.throws(() => normalizeGalleryIndex({ nope: true }, 'https://example.com/index.json'), /list of themes/);
});

test('bundled background presets are listed from the first existing folder and can be saved into a theme as SVG', () => {
    const { root, store } = tempStore();
    const { listPresetBackgrounds } = require('./main/themes.cjs');
    const presets = path.join(root, 'presets');
    fs.mkdirSync(presets);
    fs.writeFileSync(path.join(presets, 'deep-sea.svg'), '<svg xmlns="http://www.w3.org/2000/svg"/>');
    fs.writeFileSync(path.join(presets, 'notes.txt'), 'x');
    const listed = listPresetBackgrounds([path.join(root, 'missing'), presets]);
    assert.deepEqual(listed, [{ name: 'Deep Sea', path: path.join(presets, 'deep-sea.svg') }]);
    assert.deepEqual(listPresetBackgrounds([path.join(root, 'missing')]), []);
    const id = store.save({ theme: theme('Sea'), backgroundSource: listed[0].path });
    assert.equal(JSON.parse(fs.readFileSync(path.join(root, 'themes', id, 'theme.json'), 'utf8')).background.image, 'background.svg');
    assert.equal(fs.existsSync(path.join(presets, 'deep-sea.svg')), true);
});
