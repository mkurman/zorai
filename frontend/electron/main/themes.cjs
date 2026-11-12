const fs = require('fs');
const path = require('path');

const THEME_FILE = 'theme.json';
const THEME_CSS_FILE = 'theme.css';
const WINDOW_APPEARANCE_FILE = 'window-appearance.json';
const CUSTOM_CSS_FILE = 'custom.css';
const IMAGE_EXTENSIONS = ['.png', '.jpg', '.jpeg', '.gif', '.webp', '.bmp', '.svg'];
const VIDEO_EXTENSIONS = ['.mp4', '.webm'];
const BACKGROUND_EXTENSIONS = [...IMAGE_EXTENSIONS, ...VIDEO_EXTENSIONS];

function themeSlug(name) {
    const slug = String(name ?? '').toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '');
    return slug || 'theme';
}

function uniqueChildName(parent, base) {
    let candidate = base;
    for (let index = 2; fs.existsSync(path.join(parent, candidate)); index += 1) candidate = `${base}-${index}`;
    return candidate;
}

function validateTheme(theme) {
    if (!theme || typeof theme !== 'object' || Array.isArray(theme)) return 'theme.json must contain a JSON object.';
    if (typeof theme.name !== 'string' || !theme.name.trim()) return 'theme.json needs a "name".';
    if (theme.appearance !== 'dark' && theme.appearance !== 'light') return 'theme.json "appearance" must be "dark" or "light".';
    for (const key of ['colors', 'appShell', 'fonts', 'background']) {
        if (theme[key] !== undefined && (typeof theme[key] !== 'object' || theme[key] === null || Array.isArray(theme[key]))) return `theme.json "${key}" must be an object.`;
    }
    return null;
}

function resolveInside(dir, relative) {
    if (typeof relative !== 'string' || !relative.trim()) return null;
    const resolved = path.resolve(dir, relative);
    return resolved.startsWith(`${path.resolve(dir)}${path.sep}`) ? resolved : null;
}

function readThemeRecord(id, dir, jsonPath, cssPath) {
    const record = { id, dir, theme: null, css: null, backgroundPath: null, error: null };
    try {
        const theme = JSON.parse(fs.readFileSync(jsonPath, 'utf8'));
        record.error = validateTheme(theme);
        if (record.error) return record;
        record.theme = theme;
        if (cssPath && fs.existsSync(cssPath)) record.css = fs.readFileSync(cssPath, 'utf8');
        const image = resolveInside(dir, theme.background?.image);
        if (image && fs.existsSync(image)) record.backgroundPath = image;
        else if (theme.background?.image) record.error = `Background image "${theme.background.image}" was not found next to the theme.`;
    } catch (error) {
        record.error = `Could not read ${path.basename(jsonPath)}: ${error?.message ?? error}`;
    }
    return record;
}

const REMOTE_LIMITS = { json: 1024 * 1024, css: 512 * 1024, image: 25 * 1024 * 1024, video: 100 * 1024 * 1024 };
const REMOTE_TIMEOUT_MS = 20000;

function normalizeThemeUrl(input) {
    let url;
    try {
        url = new URL(String(input ?? '').trim());
    } catch {
        throw new Error('That is not a valid web address.');
    }
    if (url.protocol !== 'https:') throw new Error('Only https:// addresses are supported.');
    if (url.hostname === 'github.com') {
        const parts = url.pathname.split('/').filter(Boolean);
        if (parts.length >= 4 && (parts[2] === 'blob' || parts[2] === 'tree')) {
            const [owner, repo, kind, branch, ...rest] = parts;
            const file = kind === 'tree' ? [...rest, THEME_FILE] : rest;
            return `https://raw.githubusercontent.com/${owner}/${repo}/${branch}/${file.join('/')}`;
        }
        throw new Error('Link to a theme.json file or a theme folder on GitHub.');
    }
    if (!url.pathname.toLowerCase().endsWith('.json')) url.pathname = `${url.pathname.replace(/\/$/, '')}/${THEME_FILE}`;
    return url.toString();
}

async function fetchLimited(fetchImpl, url, limit) {
    const response = await fetchImpl(url, { redirect: 'follow', signal: AbortSignal.timeout(REMOTE_TIMEOUT_MS) });
    if (response.url && !response.url.startsWith('https:')) throw new Error(`${url} redirected to a non-https address.`);
    if (!response.ok) {
        const error = new Error(`${url} returned ${response.status}.`);
        error.status = response.status;
        throw error;
    }
    const declared = Number(response.headers?.get?.('content-length') ?? 0);
    if (declared > limit) throw new Error(`${url} is larger than the ${Math.round(limit / 1024 / 1024) || 1} MB limit.`);
    const buffer = Buffer.from(await response.arrayBuffer());
    if (buffer.length > limit) throw new Error(`${url} is larger than the ${Math.round(limit / 1024 / 1024) || 1} MB limit.`);
    return buffer;
}

function relativeAssetPath(value) {
    if (typeof value !== 'string' || !value.trim()) return null;
    if (/^[a-z][a-z0-9+.-]*:/i.test(value) || value.startsWith('/') || value.split(/[\\/]/).includes('..')) {
        throw new Error('The theme background must be a file next to theme.json.');
    }
    return value;
}

async function stageRemoteTheme({ url, fetchImpl, stagingRoot }) {
    const themeUrl = normalizeThemeUrl(url);
    const theme = JSON.parse((await fetchLimited(fetchImpl, themeUrl, REMOTE_LIMITS.json)).toString('utf8'));
    const problem = validateTheme(theme);
    if (problem) throw new Error(problem);
    const dir = fs.mkdtempSync(path.join(stagingRoot, 'zorai-theme-'));
    try {
        const image = relativeAssetPath(theme.background?.image);
        if (image) {
            const extension = path.extname(image).toLowerCase();
            if (!BACKGROUND_EXTENSIONS.includes(extension)) throw new Error('The background must be an image (PNG, JPEG, GIF, WebP, BMP, SVG) or a video (MP4, WebM).');
            const limit = VIDEO_EXTENSIONS.includes(extension) ? REMOTE_LIMITS.video : REMOTE_LIMITS.image;
            const target = path.join(dir, image);
            fs.mkdirSync(path.dirname(target), { recursive: true });
            fs.writeFileSync(target, await fetchLimited(fetchImpl, new URL(image, themeUrl).toString(), limit));
        }
        try {
            fs.writeFileSync(path.join(dir, THEME_CSS_FILE), await fetchLimited(fetchImpl, new URL(THEME_CSS_FILE, themeUrl).toString(), REMOTE_LIMITS.css));
        } catch (error) {
            if (error?.status !== 404) throw error;
        }
        fs.writeFileSync(path.join(dir, THEME_FILE), `${JSON.stringify(theme, null, 2)}\n`);
        return { dir, sourceUrl: themeUrl, record: readThemeRecord('', dir, path.join(dir, THEME_FILE), path.join(dir, THEME_CSS_FILE)) };
    } catch (error) {
        fs.rmSync(dir, { recursive: true, force: true });
        throw error;
    }
}

function normalizeGalleryIndex(payload, indexUrl) {
    const entries = Array.isArray(payload) ? payload : Array.isArray(payload?.themes) ? payload.themes : null;
    if (!entries) throw new Error('The gallery index must be a JSON list of themes, or an object with a "themes" list.');
    return entries
        .filter((entry) => entry && typeof entry.name === 'string' && typeof entry.url === 'string')
        .map((entry) => ({
            name: entry.name.trim(),
            url: new URL(entry.url, indexUrl).toString(),
            author: typeof entry.author === 'string' ? entry.author : '',
            description: typeof entry.description === 'string' ? entry.description : '',
            appearance: entry.appearance === 'light' ? 'light' : entry.appearance === 'dark' ? 'dark' : null,
        }));
}

function listPresetBackgrounds(presetDirs) {
    const dir = presetDirs.find((candidate) => fs.existsSync(candidate));
    if (!dir) return [];
    return fs.readdirSync(dir)
        .filter((file) => BACKGROUND_EXTENSIONS.includes(path.extname(file).toLowerCase()))
        .map((file) => {
            const stem = path.basename(file, path.extname(file));
            const favorite = stem.startsWith('zorai-');
            const label = favorite ? stem.slice('zorai-'.length) : stem;
            return { name: label.replace(/[-_]+/g, ' ').replace(/\b\w/g, (char) => char.toUpperCase()), path: path.join(dir, file), favorite };
        })
        .sort((a, b) => Number(b.favorite) - Number(a.favorite) || path.basename(a.path).localeCompare(path.basename(b.path)));
}

function createThemeStore({ rootDir }) {
    const themesDir = () => {
        const dir = path.join(rootDir(), 'themes');
        fs.mkdirSync(dir, { recursive: true });
        return dir;
    };

    const themeDirFor = (id) => {
        const dir = resolveInside(themesDir(), id);
        if (!dir || path.dirname(dir) !== themesDir()) throw new Error(`Unknown theme "${id}".`);
        return dir;
    };

    function list() {
        const dir = themesDir();
        const records = [];
        for (const entry of fs.readdirSync(dir, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) {
            const entryPath = path.join(dir, entry.name);
            if (entry.isDirectory() && fs.existsSync(path.join(entryPath, THEME_FILE))) {
                records.push(readThemeRecord(entry.name, entryPath, path.join(entryPath, THEME_FILE), path.join(entryPath, THEME_CSS_FILE)));
            } else if (entry.isFile() && entry.name.toLowerCase().endsWith('.json')) {
                records.push(readThemeRecord(entry.name, dir, entryPath, null));
            }
        }
        return records;
    }

    function save({ id, theme, css, backgroundSource }) {
        const error = validateTheme(theme);
        if (error) throw new Error(error);
        const existing = id ? themeDirFor(id) : null;
        const singleFile = Boolean(existing && fs.statSync(existing).isFile());
        const dir = existing && !singleFile ? existing : path.join(themesDir(), uniqueChildName(themesDir(), themeSlug(theme.name)));
        fs.mkdirSync(dir, { recursive: true });
        const next = { ...theme, background: theme.background ? { ...theme.background } : undefined };
        if (backgroundSource === null) {
            if (next.background) delete next.background.image;
        } else if (typeof backgroundSource === 'string' && backgroundSource) {
            const extension = path.extname(backgroundSource).toLowerCase();
            if (!BACKGROUND_EXTENSIONS.includes(extension)) throw new Error('The background must be an image (PNG, JPEG, GIF, WebP, BMP, SVG) or a video (MP4, WebM).');
            const target = path.join(dir, `background${extension}`);
            if (path.resolve(backgroundSource) !== target) {
                for (const old of BACKGROUND_EXTENSIONS) fs.rmSync(path.join(dir, `background${old}`), { force: true });
                fs.copyFileSync(backgroundSource, target);
            }
            next.background = { ...next.background, image: `background${extension}` };
        }
        fs.writeFileSync(path.join(dir, THEME_FILE), `${JSON.stringify(next, null, 2)}\n`);
        if (typeof css === 'string' && css.trim()) fs.writeFileSync(path.join(dir, THEME_CSS_FILE), css);
        else fs.rmSync(path.join(dir, THEME_CSS_FILE), { force: true });
        if (singleFile) fs.rmSync(existing, { force: true });
        return path.basename(dir);
    }

    function importFrom(sourceFile) {
        const sourceDir = path.dirname(sourceFile);
        const record = readThemeRecord('', sourceDir, sourceFile, path.basename(sourceFile) === THEME_FILE ? path.join(sourceDir, THEME_CSS_FILE) : null);
        if (!record.theme) throw new Error(record.error ?? 'Not a theme file.');
        const target = path.join(themesDir(), uniqueChildName(themesDir(), themeSlug(record.theme.name)));
        if (path.basename(sourceFile) === THEME_FILE) {
            fs.cpSync(sourceDir, target, { recursive: true });
        } else {
            fs.mkdirSync(target, { recursive: true });
            fs.copyFileSync(sourceFile, path.join(target, THEME_FILE));
        }
        return readThemeRecord(path.basename(target), target, path.join(target, THEME_FILE), path.join(target, THEME_CSS_FILE));
    }

    function installStaged(stagingDir) {
        const record = readThemeRecord('', stagingDir, path.join(stagingDir, THEME_FILE), path.join(stagingDir, THEME_CSS_FILE));
        if (!record.theme) throw new Error(record.error ?? 'Not a theme.');
        const target = path.join(themesDir(), uniqueChildName(themesDir(), themeSlug(record.theme.name)));
        fs.cpSync(stagingDir, target, { recursive: true });
        return readThemeRecord(path.basename(target), target, path.join(target, THEME_FILE), path.join(target, THEME_CSS_FILE));
    }

    function exportTo(id, destinationDir) {
        const source = themeDirFor(id);
        const target = path.join(destinationDir, uniqueChildName(destinationDir, id));
        fs.cpSync(source, target, { recursive: true });
        return target;
    }

    const customCssPath = () => path.join(rootDir(), CUSTOM_CSS_FILE);

    function readCustomCss() {
        try {
            return fs.readFileSync(customCssPath(), 'utf8');
        } catch {
            return null;
        }
    }

    function ensureCustomCss() {
        if (!fs.existsSync(customCssPath())) fs.writeFileSync(customCssPath(), '');
        return customCssPath();
    }

    function readWindowAppearance() {
        try {
            const parsed = JSON.parse(fs.readFileSync(path.join(rootDir(), WINDOW_APPEARANCE_FILE), 'utf8'));
            return { seeThrough: parsed?.seeThrough === true };
        } catch {
            return { seeThrough: false };
        }
    }

    function writeWindowAppearance(appearance) {
        fs.writeFileSync(path.join(rootDir(), WINDOW_APPEARANCE_FILE), `${JSON.stringify({ seeThrough: appearance?.seeThrough === true }, null, 2)}\n`);
    }

    return { themesDir, themeDirFor, list, save, importFrom, installStaged, exportTo, customCssPath, readCustomCss, ensureCustomCss, readWindowAppearance, writeWindowAppearance };
}

function seeThroughWindowOptions(platform) {
    if (platform === 'darwin') return { vibrancy: 'under-window', visualEffectState: 'active', backgroundColor: '#00000000' };
    if (platform === 'win32') return { backgroundMaterial: 'acrylic', backgroundColor: '#00000000' };
    return { transparent: true, backgroundColor: '#00000000' };
}

function registerThemeIpcHandlers(ipcMain, { store, dialog, shell, BrowserWindow, seeThroughAtLaunch, fetchImpl = fetch, stagingRoot = require('os').tmpdir(), presetDirs = [] }) {
    const staged = new Map();
    let stageCounter = 0;
    const discardStaged = (token) => {
        const entry = staged.get(token);
        if (entry) fs.rmSync(entry.dir, { recursive: true, force: true });
        staged.delete(token);
    };
    let watcher = null;
    let timer = null;
    const broadcast = () => {
        clearTimeout(timer);
        timer = setTimeout(() => {
            for (const window of BrowserWindow.getAllWindows()) window.webContents.send('themes-changed');
        }, 250);
    };
    try {
        watcher = fs.watch(store.themesDir(), { recursive: true }, broadcast);
    } catch {
        watcher = fs.watch(store.themesDir(), broadcast);
    }
    fs.watchFile(store.customCssPath(), { interval: 1000 }, broadcast);

    ipcMain.handle('themes-list', () => ({ dir: store.themesDir(), themes: store.list(), customCss: { path: store.customCssPath(), css: store.readCustomCss() } }));
    ipcMain.handle('themes-open-custom-css', () => shell.openPath(store.ensureCustomCss()));
    ipcMain.handle('themes-reveal-custom-css', () => shell.showItemInFolder(store.ensureCustomCss()));
    ipcMain.handle('themes-save', (_event, payload) => store.save(payload ?? {}));
    ipcMain.handle('themes-delete', async (_event, id) => {
        await shell.trashItem(store.themeDirFor(id));
        return true;
    });
    ipcMain.handle('themes-import', async () => {
        const selection = await dialog.showOpenDialog({
            title: 'Import Theme',
            properties: ['openFile'],
            filters: [{ name: 'Zorai theme', extensions: ['json'] }],
        });
        if (selection.canceled || !selection.filePaths?.[0]) return null;
        return store.importFrom(selection.filePaths[0]);
    });
    ipcMain.handle('themes-export', async (_event, id) => {
        const selection = await dialog.showOpenDialog({ title: 'Export Theme To…', properties: ['openDirectory', 'createDirectory'] });
        if (selection.canceled || !selection.filePaths?.[0]) return null;
        const target = store.exportTo(id, selection.filePaths[0]);
        shell.showItemInFolder(target);
        return target;
    });
    ipcMain.handle('themes-open-folder', () => shell.openPath(store.themesDir()));
    ipcMain.handle('themes-preset-backgrounds', () => listPresetBackgrounds(presetDirs));
    ipcMain.handle('themes-fetch-url', async (_event, url) => {
        const result = await stageRemoteTheme({ url, fetchImpl, stagingRoot });
        stageCounter += 1;
        const token = `stage-${Date.now()}-${stageCounter}`;
        staged.set(token, result);
        return { token, sourceUrl: result.sourceUrl, record: result.record };
    });
    ipcMain.handle('themes-install-staged', (_event, token) => {
        const entry = staged.get(token);
        if (!entry) throw new Error('This download expired. Fetch the theme again.');
        try {
            return store.installStaged(entry.dir);
        } finally {
            discardStaged(token);
        }
    });
    ipcMain.handle('themes-discard-staged', (_event, token) => {
        discardStaged(token);
        return true;
    });
    ipcMain.handle('themes-fetch-gallery', async (_event, indexUrl) => {
        const url = new URL(String(indexUrl ?? '').trim());
        if (url.protocol !== 'https:') throw new Error('Only https:// gallery addresses are supported.');
        const payload = JSON.parse((await fetchLimited(fetchImpl, url.toString(), REMOTE_LIMITS.json)).toString('utf8'));
        return normalizeGalleryIndex(payload, url.toString());
    });
    ipcMain.handle('themes-pick-background', async () => {
        const selection = await dialog.showOpenDialog({
            title: 'Background Image or Video',
            properties: ['openFile'],
            filters: [
                { name: 'Images and videos', extensions: BACKGROUND_EXTENSIONS.map((extension) => extension.slice(1)) },
                { name: 'Images', extensions: IMAGE_EXTENSIONS.map((extension) => extension.slice(1)) },
                { name: 'Videos', extensions: VIDEO_EXTENSIONS.map((extension) => extension.slice(1)) },
            ],
        });
        return selection.canceled || !selection.filePaths?.[0] ? null : selection.filePaths[0];
    });
    ipcMain.handle('themes-window-appearance', () => ({ seeThroughAtLaunch, seeThrough: store.readWindowAppearance().seeThrough }));
    ipcMain.handle('themes-set-see-through', (_event, seeThrough) => {
        store.writeWindowAppearance({ seeThrough: seeThrough === true });
        return { seeThroughAtLaunch, seeThrough: seeThrough === true };
    });
    return () => {
        clearTimeout(timer);
        watcher?.close();
        fs.unwatchFile(store.customCssPath(), broadcast);
        for (const token of [...staged.keys()]) discardStaged(token);
    };
}

module.exports = { createThemeStore, listPresetBackgrounds, normalizeGalleryIndex, normalizeThemeUrl, registerThemeIpcHandlers, seeThroughWindowOptions, stageRemoteTheme, themeSlug, validateTheme };
