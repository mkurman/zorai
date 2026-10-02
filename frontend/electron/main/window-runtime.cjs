function shouldOpenDevTools({ isPackaged, env = process.env } = {}) {
    if (isPackaged) return false;
    const flag = String(env.ZORAI_DEVTOOLS ?? "").trim().toLowerCase();
    return flag === "1" || flag === "true";
}

function resolveWindowFrameOptions(platform = process.platform) {
    const useNativeFrame = platform === 'win32' || platform === 'linux';
    return {
        frame: useNativeFrame,
        titleBarStyle: useNativeFrame ? 'default' : 'hidden',
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
        Menu,
        nativeImage,
        getMainWindow,
        logToFile,
        path,
        screen,
        setMainWindow,
        shell,
        stopAllTerminalBridges,
    } = options;
    const { resolveRendererLoadTarget } = require("./load-target.cjs");
    const appName = 'Zorai';

    function sendAppCommand(command) {
        getMainWindow()?.webContents.send('app-command', command);
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
                { type: 'separator' }, { role: 'reload' }, { role: 'forceReload' }, { role: 'toggleDevTools' },
                { type: 'separator' }, { role: 'resetZoom' }, { role: 'zoomIn' }, { role: 'zoomOut' }, { role: 'togglefullscreen' },
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

    return { createWindow, setWindowOpacity };
}

module.exports = {
    createWindowRuntime,
    loadWindowIcon,
    resolveWindowFrameOptions,
    resolveWindowIcon,
    shouldOpenDevTools,
};
