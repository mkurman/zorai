const os = require('os');
const path = require('path');
const { pathToFileURL } = require('url');

const MEDIA_SCHEME = 'zorai-media';
const MEDIA_EXTENSIONS = new Set([
    '.png', '.jpg', '.jpeg', '.gif', '.webp', '.bmp', '.svg', '.ico',
    '.mp4', '.webm', '.mov', '.m4v',
    '.mp3', '.wav', '.ogg', '.oga', '.opus', '.m4a', '.aac', '.flac', '.weba',
]);

function mediaPathFromUrl(url) {
    const parsed = new URL(url);
    if (parsed.protocol !== `${MEDIA_SCHEME}:`) return null;
    const decoded = decodeURIComponent(parsed.pathname);
    const expanded = decoded.startsWith('/~/') ? path.join(os.homedir(), decoded.slice(3)) : decoded;
    return path.resolve(expanded);
}

function isServableMediaPath(filePath) {
    return typeof filePath === 'string' && MEDIA_EXTENSIONS.has(path.extname(filePath).toLowerCase());
}

function registerMediaSchemePrivileges(protocol, extraSchemes = []) {
    protocol.registerSchemesAsPrivileged([
        { scheme: MEDIA_SCHEME, privileges: { standard: true, secure: true, stream: true, supportFetchAPI: true } },
        ...extraSchemes,
    ]);
}

function registerMediaProtocol(protocol, net) {
    protocol.handle(MEDIA_SCHEME, (request) => {
        const filePath = mediaPathFromUrl(request.url);
        if (!filePath || !isServableMediaPath(filePath)) return new Response('Not a media file', { status: 403 });
        return net.fetch(pathToFileURL(filePath).toString(), { headers: request.headers });
    });
}

module.exports = { MEDIA_SCHEME, mediaPathFromUrl, isServableMediaPath, registerMediaSchemePrivileges, registerMediaProtocol };
