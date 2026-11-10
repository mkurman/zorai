function shouldOpenDevTools({ isPackaged, env = process.env } = {}) {
    if (isPackaged) return false;
    const flag = String(env.ZORAI_DEVTOOLS ?? "").trim().toLowerCase();
    return flag === "1" || flag === "true";
}

function devToolsAccelerator(platform = process.platform) {
    return platform === "darwin" ? "Alt+Command+I" : "Ctrl+Shift+I";
}

function toggleWindowDevTools(webContents) {
    if (!webContents) return;
    if (webContents.isDevToolsOpened()) {
        webContents.closeDevTools();
        return;
    }
    webContents.openDevTools({ mode: "detach" });
}

function screenshotFileName(date = new Date()) {
    const pad = (value) => String(value).padStart(2, "0");
    return `zorai-${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}_${pad(date.getHours())}-${pad(date.getMinutes())}-${pad(date.getSeconds())}.png`;
}

function contextMenuRequest(params) {
    return {
        x: params.x,
        y: params.y,
        linkURL: /^https?:\/\//i.test(params.linkURL ?? '') ? params.linkURL : '',
        isEditable: Boolean(params.isEditable),
        canCut: Boolean(params.editFlags?.canCut),
        canCopy: Boolean(params.editFlags?.canCopy),
        canPaste: Boolean(params.editFlags?.canPaste),
    };
}

function resolveScreenshotDirectory(settingsFile, fallbackDirectory, fs) {
    try {
        const configured = JSON.parse(fs.readFileSync(settingsFile, 'utf8'))?.settings?.screenshotDirectory;
        return typeof configured === 'string' && configured.trim() ? configured.trim() : fallbackDirectory;
    } catch {
        return fallbackDirectory;
    }
}

async function saveWindowScreenshot(webContents, { directory, clipboard, fs, path, date = new Date() }) {
    const image = await webContents.capturePage();
    clipboard.writeImage(image);
    fs.mkdirSync(directory, { recursive: true });
    const filePath = path.join(directory, screenshotFileName(date));
    fs.writeFileSync(filePath, image.toPNG());
    return filePath;
}

function resolveWindowFrameOptions() {
    return {
        frame: false,
        titleBarStyle: 'hidden',
    };
}

function loadWindowIcon(options) {
    const iconPath = resolveWindowIcon(options);
    const icon = options.nativeImage.createFromPath(iconPath);
    if (icon.isEmpty()) {
        throw new Error(`Failed to load Electron window icon: ${iconPath}`);
    }
    return icon;
}

function resolveWindowIcon(options) {
    const {
        electronDir,
        path,
        platform = process.platform,
    } = options;

    const fileName = platform === 'win32' ? 'icon.ico' : 'icon.png';
    return path.join(electronDir, '..', 'assets', fileName);
}

function createWindowRuntime(options) {
    const {
        app,
        BrowserWindow,
        clipboard,
        Menu,
        nativeImage,
        Notification,
        getDataDir,
        getMainWindow,
        logToFile,
        path,
        screen,
        setMainWindow,
        shell,
        stopAllTerminalBridges,
    } = options;
    const { resolveRendererLoadTarget } = require("./load-target.cjs");
    const fs = require("fs");
    const appName = 'Zorai';

    function sendAppCommand(command) {
        getMainWindow()?.webContents.send('app-command', command);
    }

    function defaultScreenshotDirectory() {
        return path.join(app.getPath('pictures'), appName);
    }

    async function takeScreenshot(window) {
        try {
            await new Promise((resolve) => setTimeout(resolve, 150));
            const filePath = await saveWindowScreenshot(window.webContents, {
                directory: resolveScreenshotDirectory(
                    path.join(getDataDir(), 'settings.json'),
                    defaultScreenshotDirectory(),
                    fs,
                ),
                clipboard,
                fs,
                path,
            });
            logToFile('info', 'screenshot saved', { filePath });
            if (Notification?.isSupported?.()) {
                const notification = new Notification({ title: 'Screenshot saved', body: `Copied to clipboard and saved to ${filePath}` });
                notification.on('click', () => shell.showItemInFolder(filePath));
                notification.show();
            }
        } catch (error) {
            logToFile('error', 'screenshot failed', { error: error?.message || String(error) });
        }
    }

    function runContextMenuAction(webContents, action, payload) {
        const window = BrowserWindow.fromWebContents(webContents);
        if (action === 'cut' || action === 'copy' || action === 'paste' || action === 'selectAll') {
            webContents[action]();
            return true;
        }
        if (action === 'openLink' && /^https?:\/\//i.test(payload ?? '')) {
            void shell.openExternal(payload);
            return true;
        }
        if (action === 'copyLink' && typeof payload === 'string') {
            clipboard.writeText(payload);
            return true;
        }
        if (action === 'screenshot' && window) {
            void takeScreenshot(window);
            return true;
        }
        return false;
    }

    function buildAppMenu() {
        return Menu.buildFromTemplate([
            { label: 'File', submenu: [
                { label: 'New Window', accelerator: 'Ctrl+Alt+N', click: () => createWindow() },
                { label: 'New Workspace', accelerator: 'Ctrl+Shift+N', click: () => sendAppCommand('new-workspace') },
                { label: 'New Surface', accelerator: 'Ctrl+T', click: () => sendAppCommand('new-surface') },
                { type: 'separator' },
                { label: 'Settings', accelerator: 'Ctrl+,', click: () => sendAppCommand('toggle-settings') },
                { type: 'separator' },
                { role: 'quit', label: 'Exit' },
            ] },
            { label: 'Edit', submenu: [
                { role: 'undo' }, { role: 'redo' }, { type: 'separator' }, { role: 'cut' },
                { label: 'Copy', accelerator: 'Ctrl+C', click: () => { getMainWindow()?.webContents.copy(); sendAppCommand('copy'); } },
                { label: 'Paste', accelerator: 'Ctrl+V', click: () => { getMainWindow()?.webContents.paste(); sendAppCommand('paste'); } },
                { label: 'Select All', accelerator: 'Ctrl+A', click: () => { getMainWindow()?.webContents.selectAll(); sendAppCommand('select-all'); } },
            ] },
            { label: 'View', submenu: [
                { label: 'Command Palette', accelerator: 'Ctrl+Shift+P', click: () => sendAppCommand('toggle-command-palette') },
                { label: 'Search', accelerator: 'Ctrl+Shift+F', click: () => sendAppCommand('toggle-search') },
                { label: 'File Manager', accelerator: 'Ctrl+Shift+E', click: () => sendAppCommand('toggle-file-manager') },
                { label: 'Toggle Sidebar', accelerator: 'Ctrl+B', click: () => sendAppCommand('toggle-sidebar') },
                { type: 'separator' }, { role: 'reload' }, { role: 'forceReload' },
                { label: 'Toggle Developer Tools', accelerator: devToolsAccelerator(), click: () => toggleWindowDevTools(getMainWindow()?.webContents) },
                { type: 'separator' }, { role: 'resetZoom' }, { role: 'zoomIn' }, { role: 'zoomOut' }, { role: 'togglefullscreen' },
                { type: 'separator' },
                { label: 'Take Screenshot', click: () => { const window = getMainWindow(); if (window) void takeScreenshot(window); } },
            ] },
            { label: 'Features', submenu: [
                { label: 'Mission Console', click: () => sendAppCommand('toggle-mission') },
                { label: 'Command History', click: () => sendAppCommand('toggle-command-history') },
                { label: 'Command Log', click: () => sendAppCommand('toggle-command-log') },
                { label: 'Session Vault', click: () => sendAppCommand('toggle-session-vault') },
                { label: 'System Monitor', click: () => sendAppCommand('toggle-system-monitor') },
                { label: 'Execution Canvas', click: () => sendAppCommand('toggle-canvas') },
                { label: 'Time Travel Snapshots', click: () => sendAppCommand('toggle-time-travel') },
            ] },
            { label: 'Window', submenu: [
                { label: 'Split Right', accelerator: 'Ctrl+D', click: () => sendAppCommand('split-right') },
                { label: 'Split Down', accelerator: 'Ctrl+Shift+D', click: () => sendAppCommand('split-down') },
                { label: 'Zoom Pane', accelerator: 'Ctrl+Shift+Z', click: () => sendAppCommand('toggle-zoom') },
                { type: 'separator' }, { role: 'minimize' }, { role: 'close' },
            ] },
            { label: 'Help', submenu: [{ label: 'About', click: () => sendAppCommand('about') }] },
        ]);
    }

    function setWindowOpacity(opacity) {
        const normalized = Number.isFinite(opacity) ? Math.min(1, Math.max(0.35, Number(opacity))) : 1;
        if (getMainWindow() && typeof getMainWindow().setOpacity === 'function') {
            getMainWindow().setOpacity(normalized);
        }
        return normalized;
    }

    function createWindow() {
        const { width: screenW, height: screenH } = screen.getPrimaryDisplay().workAreaSize;
        const frameOptions = resolveWindowFrameOptions();
        const openWindows = BrowserWindow.getAllWindows().length;
        const mainWindow = new BrowserWindow({
            width: Math.min(1400, screenW),
            height: Math.min(900, screenH),
            minWidth: 600,
            minHeight: 400,
            ...frameOptions,
            autoHideMenuBar: false,
            webPreferences: {
                preload: path.join(options.electronDir, 'preload.cjs'),
                nodeIntegration: false,
                contextIsolation: true,
                webviewTag: true,
            },
            title: appName,
            icon: loadWindowIcon({
                electronDir: options.electronDir,
                nativeImage,
                path,
            }),
            backgroundColor: '#1e1e2e',
            show: false,
            opacity: 1,
        });
        if (openWindows > 0) {
            const [currentX, currentY] = mainWindow.getPosition();
            mainWindow.setPosition(currentX + 32, currentY + 32);
        }
        setMainWindow(mainWindow);
        const rendererLoadTarget = resolveRendererLoadTarget({
            app,
            electronDir: options.electronDir,
            env: process.env,
            path,
        });
        if (rendererLoadTarget.kind === "url") {
            mainWindow.loadURL(rendererLoadTarget.value);
        } else {
            mainWindow.loadFile(rendererLoadTarget.value);
        }
        mainWindow.webContents.setWindowOpenHandler(({ url }) => /^https?:\/\//i.test(url) ? (void shell.openExternal(url), { action: 'deny' }) : { action: 'allow' });
        mainWindow.webContents.on('will-navigate', (event, url) => {
            const currentUrl = mainWindow.webContents.getURL();
            if (url !== currentUrl && /^https?:\/\//i.test(url)) {
                event.preventDefault();
                void shell.openExternal(url);
            }
        });
        mainWindow.webContents.on('context-menu', (_event, params) => {
            mainWindow.webContents.send('context-menu-request', contextMenuRequest(params));
        });
        Menu.setApplicationMenu(buildAppMenu());
        mainWindow.once('ready-to-show', () => mainWindow.show());
        if (shouldOpenDevTools({ isPackaged: app.isPackaged })) {
            mainWindow.webContents.openDevTools({ mode: "detach" });
        }
        mainWindow.on('focus', () => setMainWindow(mainWindow));
        mainWindow.on('maximize', () => mainWindow.webContents.send('window-state', 'maximized'));
        mainWindow.on('unmaximize', () => mainWindow.webContents.send('window-state', 'normal'));
        mainWindow.on('closed', () => {
            logToFile('info', 'window closed');
            const remaining = BrowserWindow.getAllWindows();
            if (getMainWindow() === mainWindow) {
                setMainWindow(remaining[0] ?? null);
            }
            if (remaining.length === 0) {
                stopAllTerminalBridges(true, true);
            }
        });
    }

    return { createWindow, defaultScreenshotDirectory, runContextMenuAction, setWindowOpacity };
}

module.exports = {
    contextMenuRequest,
    createWindowRuntime,
    loadWindowIcon,
    resolveWindowFrameOptions,
    resolveWindowIcon,
    resolveScreenshotDirectory,
    saveWindowScreenshot,
    screenshotFileName,
    devToolsAccelerator,
    shouldOpenDevTools,
    toggleWindowDevTools,
};
