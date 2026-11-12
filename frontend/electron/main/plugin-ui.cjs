const fs = require('fs');
const path = require('path');
const { pathToFileURL } = require('url');

const PLUGIN_SCHEME = 'zorai-plugin';
const PLUGIN_PANEL_PERMISSIONS = ['composer', 'toast'];
const COMMAND_OUTPUTS = ['panel', 'composer', 'toast'];
const COMMAND_PLACEMENTS = ['palette', 'composer'];
const PANEL_CONTENT_SECURITY_POLICY = [
    "default-src 'none'",
    `script-src ${PLUGIN_SCHEME}: 'unsafe-inline'`,
    `style-src ${PLUGIN_SCHEME}: 'unsafe-inline'`,
    `img-src ${PLUGIN_SCHEME}: data: blob:`,
    `font-src ${PLUGIN_SCHEME}: data:`,
    `media-src ${PLUGIN_SCHEME}: data: blob:`,
    "connect-src 'none'",
    "frame-src 'none'",
    "form-action 'none'",
    "base-uri 'none'",
].join('; ');
const PANEL_MIME_TYPES = {
    '.html': 'text/html; charset=utf-8',
    '.htm': 'text/html; charset=utf-8',
    '.js': 'text/javascript; charset=utf-8',
    '.mjs': 'text/javascript; charset=utf-8',
    '.css': 'text/css; charset=utf-8',
    '.json': 'application/json; charset=utf-8',
    '.svg': 'image/svg+xml',
    '.png': 'image/png',
    '.jpg': 'image/jpeg',
    '.jpeg': 'image/jpeg',
    '.gif': 'image/gif',
    '.webp': 'image/webp',
    '.woff': 'font/woff',
    '.woff2': 'font/woff2',
};

const isId = (value) => typeof value === 'string' && /^[a-z0-9][a-z0-9_-]{0,63}$/i.test(value);
const text = (value, fallback = '') => (typeof value === 'string' && value.trim() ? value.trim() : fallback);

function insideDir(dir, relative) {
    if (typeof relative !== 'string' || !relative.trim() || path.isAbsolute(relative)) return null;
    const resolved = path.resolve(dir, relative);
    return resolved.startsWith(`${path.resolve(dir)}${path.sep}`) ? resolved : null;
}

function normalizeInputs(inputs) {
    if (!Array.isArray(inputs)) return [];
    return inputs
        .filter((input) => input && isId(input.key))
        .map((input) => ({ key: input.key, label: text(input.label, input.key), placeholder: text(input.placeholder), default: text(input.default) }));
}

function normalizePluginUi(manifest, dir, dirName) {
    const problems = [];
    const endpoints = Object.keys(manifest?.api?.endpoints ?? {});
    const ui = manifest?.ui;
    if (!ui || typeof ui !== 'object') return null;
    const endpointOk = (owner, endpoint) => {
        if (endpoints.includes(endpoint)) return true;
        problems.push(`${owner} uses endpoint "${endpoint}", which this plugin does not define.`);
        return false;
    };
    const commands = (Array.isArray(ui.commands) ? ui.commands : []).filter((command) => {
        if (!command || !isId(command.id) || !text(command.title)) {
            problems.push('A command needs an id (letters, digits, - or _) and a title.');
            return false;
        }
        if (command.endpoint === undefined && text(command.prompt)) return true;
        return endpointOk(`Command "${command.id}"`, command.endpoint);
    }).map((command) => {
        const prompt = command.endpoint === undefined ? text(command.prompt) : '';
        return {
            id: command.id,
            title: command.title.trim(),
            endpoint: prompt ? '' : command.endpoint,
            prompt: prompt || null,
            params: command.params && typeof command.params === 'object' && !Array.isArray(command.params) ? command.params : {},
            inputs: normalizeInputs(command.inputs),
            output: prompt ? 'composer' : COMMAND_OUTPUTS.includes(command.output) ? command.output : 'panel',
            placement: (Array.isArray(command.placement) ? command.placement : ['palette']).filter((place) => COMMAND_PLACEMENTS.includes(place)),
        };
    });
    const panels = (Array.isArray(ui.panels) ? ui.panels : []).filter((panel) => {
        if (!panel || !isId(panel.id) || !text(panel.title)) {
            problems.push('A panel needs an id (letters, digits, - or _) and a title.');
            return false;
        }
        if (panel.web !== undefined) {
            if (!insideDir(dir, panel.web) || !fs.existsSync(insideDir(dir, panel.web))) {
                problems.push(`Panel "${panel.id}" points to "${panel.web}", which is not a file inside the plugin folder.`);
                return false;
            }
            return true;
        }
        return endpointOk(`Panel "${panel.id}"`, panel.endpoint);
    }).map((panel) => panel.web !== undefined
        ? {
            id: panel.id,
            title: panel.title.trim(),
            kind: 'web',
            url: `${PLUGIN_SCHEME}://${encodeURIComponent(dirName)}/${panel.web.split(/[\\/]/).map(encodeURIComponent).join('/')}`,
            permissions: (Array.isArray(panel.permissions) ? panel.permissions : []).filter((permission) => PLUGIN_PANEL_PERMISSIONS.includes(permission)),
        }
        : {
            id: panel.id,
            title: panel.title.trim(),
            kind: 'markdown',
            endpoint: panel.endpoint,
            params: panel.params && typeof panel.params === 'object' && !Array.isArray(panel.params) ? panel.params : {},
            inputs: normalizeInputs(panel.inputs),
        });
    return { commands, panels, problems, icon: normalizeIcon(ui.icon, dir, dirName, problems) };
}

const ICON_EXTENSIONS = ['.svg', '.png', '.jpg', '.jpeg', '.webp', '.gif'];

function normalizeIcon(icon, dir, dirName, problems) {
    if (typeof icon !== 'string' || !icon.trim()) return null;
    const value = icon.trim();
    if (ICON_EXTENSIONS.includes(path.extname(value).toLowerCase())) {
        const file = insideDir(dir, value);
        if (!file || !fs.existsSync(file)) {
            problems.push(`The icon "${value}" is not a file inside the plugin folder.`);
            return null;
        }
        return { kind: 'image', url: `${PLUGIN_SCHEME}://${encodeURIComponent(dirName)}/${value.split(/[\\/]/).map(encodeURIComponent).join('/')}` };
    }
    if ([...value].length <= 3) return { kind: 'glyph', text: value };
    problems.push('The icon must be an image file (SVG, PNG, JPEG, WebP, GIF) or up to 3 characters such as an emoji.');
    return null;
}

function readPluginUiManifests(pluginsRoot) {
    if (!fs.existsSync(pluginsRoot)) return [];
    const results = [];
    for (const entry of fs.readdirSync(pluginsRoot, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) {
        if (!entry.isDirectory()) continue;
        const dir = path.join(pluginsRoot, entry.name);
        const manifestPath = path.join(dir, 'plugin.json');
        if (!fs.existsSync(manifestPath)) continue;
        try {
            const manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
            const ui = normalizePluginUi(manifest, dir, entry.name);
            if (!ui) continue;
            results.push({
                name: text(manifest.name, entry.name),
                version: text(manifest.version, '0'),
                description: text(manifest.description),
                dirName: entry.name,
                ...ui,
            });
        } catch (error) {
            results.push({ name: entry.name, version: '0', description: '', dirName: entry.name, commands: [], panels: [], problems: [`Could not read plugin.json: ${error?.message ?? error}`] });
        }
    }
    return results;
}

function pluginFileFromUrl(pluginsRoot, url) {
    const parsed = new URL(url);
    if (parsed.protocol !== `${PLUGIN_SCHEME}:`) return null;
    const dirName = decodeURIComponent(parsed.hostname);
    if (!dirName || dirName.includes('/') || dirName.includes('\\') || dirName === '..') return null;
    const dir = path.join(pluginsRoot, dirName);
    const relative = decodeURIComponent(parsed.pathname).replace(/^\/+/, '');
    const file = insideDir(dir, relative);
    if (!file || !PANEL_MIME_TYPES[path.extname(file).toLowerCase()]) return null;
    return file;
}

function pluginSchemePrivileges() {
    return { scheme: PLUGIN_SCHEME, privileges: { standard: true, secure: true } };
}

function registerPluginProtocol(protocol, net, getPluginsRoot) {
    protocol.handle(PLUGIN_SCHEME, async (request) => {
        const file = pluginFileFromUrl(getPluginsRoot(), request.url);
        if (!file || !fs.existsSync(file)) return new Response('Not found', { status: 404 });
        const response = await net.fetch(pathToFileURL(file).toString());
        return new Response(response.body, {
            status: 200,
            headers: {
                'Content-Type': PANEL_MIME_TYPES[path.extname(file).toLowerCase()],
                'Content-Security-Policy': PANEL_CONTENT_SECURITY_POLICY,
                'X-Content-Type-Options': 'nosniff',
            },
        });
    });
}

module.exports = {
    PANEL_CONTENT_SECURITY_POLICY,
    PLUGIN_SCHEME,
    normalizePluginUi,
    pluginFileFromUrl,
    pluginSchemePrivileges,
    readPluginUiManifests,
    registerPluginProtocol,
};
