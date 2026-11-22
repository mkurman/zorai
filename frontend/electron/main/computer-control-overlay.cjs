const STOP_URL = 'zorai-computer-control://stop';
const BANNER_WIDTH = 380;
const BANNER_HEIGHT = 52;

const FRAME_HTML = `<!doctype html><html><head><meta charset="utf-8"><style>
html,body{margin:0;width:100%;height:100%;overflow:hidden;background:transparent}
div{position:fixed;inset:0;border:3px solid #7c9cff;box-shadow:inset 0 0 28px rgba(124,156,255,.45);animation:g 2.4s ease-in-out infinite}
@keyframes g{50%{box-shadow:inset 0 0 44px rgba(124,156,255,.65)}}
@media (prefers-reduced-motion:reduce){div{animation:none}}
</style></head><body><div></div></body></html>`;

const BANNER_HTML = `<!doctype html><html><head><meta charset="utf-8"><style>
html,body{margin:0;height:100%;overflow:hidden;background:transparent;font:13px system-ui,-apple-system,"Segoe UI",sans-serif}
.b{box-sizing:border-box;height:100%;display:flex;align-items:center;gap:12px;padding:0 8px 0 16px;border:1px solid rgba(124,156,255,.6);border-radius:999px;background:#151821;color:#e8ebf2;user-select:none;-webkit-app-region:drag}
i{width:9px;height:9px;border-radius:50%;background:#7c9cff;animation:p 1.6s ease-out infinite}
@keyframes p{0%{box-shadow:0 0 0 0 rgba(124,156,255,.6)}100%{box-shadow:0 0 0 10px transparent}}
strong{flex:1;font-weight:600}
a{-webkit-app-region:no-drag;padding:7px 16px;border-radius:999px;background:#7c9cff;color:#0b0d12;font-weight:600;text-decoration:none}
a:hover{background:#9ab2ff}
</style></head><body><div class="b"><i></i><strong>Zorai is controlling your computer</strong><a href="${STOP_URL}">Stop</a></div></body></html>`;

function htmlUrl(html) {
    return `data:text/html;charset=utf-8,${encodeURIComponent(html)}`;
}

function createComputerControlOverlay({ BrowserWindow, screen, getMainWindow, onStop, logToFile }) {
    let windows = [];
    let watchedWindow = null;

    function overlayWindow(bounds, interactive) {
        const win = new BrowserWindow({
            ...bounds,
            show: false,
            frame: false,
            transparent: true,
            resizable: false,
            movable: interactive,
            minimizable: false,
            maximizable: false,
            fullscreenable: false,
            focusable: false,
            skipTaskbar: true,
            hasShadow: false,
            alwaysOnTop: true,
            backgroundColor: '#00000000',
            webPreferences: { sandbox: true, contextIsolation: true, nodeIntegration: false, javascript: false },
        });
        win.setAlwaysOnTop(true, 'screen-saver');
        win.setVisibleOnAllWorkspaces(true, { visibleOnFullScreen: true });
        win.setContentProtection(true);
        if (!interactive) win.setIgnoreMouseEvents(true);
        win.webContents.on('will-navigate', (event, url) => {
            event.preventDefault();
            if (url === STOP_URL) onStop();
        });
        win.webContents.setWindowOpenHandler(() => ({ action: 'deny' }));
        win.once('ready-to-show', () => { if (!win.isDestroyed()) win.showInactive(); });
        return win;
    }

    function show() {
        if (windows.length) return;
        try {
            const mainWindow = getMainWindow();
            if (mainWindow && !mainWindow.isDestroyed()) {
                watchedWindow = mainWindow;
                mainWindow.once('close', hide);
            }
            for (const display of screen.getAllDisplays()) {
                const win = overlayWindow(display.bounds, false);
                void win.loadURL(htmlUrl(FRAME_HTML));
                windows.push(win);
            }
            const area = screen.getPrimaryDisplay().workArea;
            const banner = overlayWindow({
                x: Math.round(area.x + (area.width - BANNER_WIDTH) / 2),
                y: area.y + 12,
                width: BANNER_WIDTH,
                height: BANNER_HEIGHT,
            }, true);
            void banner.loadURL(htmlUrl(BANNER_HTML));
            windows.push(banner);
        } catch (error) {
            logToFile('warn', 'computer control overlay failed', { error: error?.message || String(error) });
            hide();
        }
    }

    function hide() {
        if (watchedWindow && !watchedWindow.isDestroyed()) watchedWindow.removeListener('close', hide);
        watchedWindow = null;
        for (const win of windows) {
            if (!win.isDestroyed()) win.destroy();
        }
        windows = [];
    }

    function returnToApp() {
        const mainWindow = getMainWindow();
        if (!mainWindow || mainWindow.isDestroyed()) return;
        if (mainWindow.isMinimized()) mainWindow.restore();
        mainWindow.show();
        mainWindow.setAlwaysOnTop(true);
        mainWindow.focus();
        mainWindow.setAlwaysOnTop(false);
    }

    function apply(event) {
        if (event?.active === true) {
            show();
            return;
        }
        hide();
        if (event?.return_to_app === true) returnToApp();
    }

    return { apply, hide };
}

module.exports = { createComputerControlOverlay, STOP_URL };
