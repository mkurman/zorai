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

const FREEDESKTOP_SOUNDS = '/usr/share/sounds/freedesktop/stereo';
const MAC_GRAB_SOUND = '/System/Library/Components/CoreAudio.component/Contents/SharedSupport/SystemSounds/system/Grab.aif';

function screenshotSoundCommands(platform, exists) {
    if (platform === 'darwin') return exists(MAC_GRAB_SOUND) ? [['afplay', [MAC_GRAB_SOUND]]] : [];
    if (platform !== 'linux') return [];
    const commands = [['canberra-gtk-play', ['-i', 'screen-capture', '-d', 'Zorai screenshot']]];
    for (const name of ['screen-capture.oga', 'camera-shutter.oga']) {
        const file = `${FREEDESKTOP_SOUNDS}/${name}`;
        if (exists(file)) commands.push(['pw-play', [file]], ['paplay', [file]]);
    }
    return commands;
}

function playScreenshotSound({ spawn, platform = process.platform, exists }) {
    const commands = screenshotSoundCommands(platform, exists);
    const tryNext = (index) => {
        if (index >= commands.length) return;
        const [command, args] = commands[index];
        let child;
        try {
            child = spawn(command, args, { stdio: 'ignore' });
        } catch {
            tryNext(index + 1);
            return;
        }
        child.on('error', () => tryNext(index + 1));
        child.on('exit', (code) => { if (code !== 0) tryNext(index + 1); });
    };
    tryNext(0);
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
        windowAppearanceOptions = {},
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
            playScreenshotSound({ spawn: require('child_process').spawn, exists: (file) => fs.existsSync(file) });
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

    let menuAcceleratorOverrides = {};

    function menuAccelerator(command, fallback) {
        const override = menuAcceleratorOverrides[command];
        if (typeof override !== 'string') return fallback;
        return override || undefined;
    }

    function setMenuAccelerators(overrides) {
        const previous = menuAcceleratorOverrides;
        menuAcceleratorOverrides = overrides && typeof overrides === 'object' ? overrides : {};
        try {
            Menu.setApplicationMenu(buildAppMenu());
            return true;
        } catch (error) {
            menuAcceleratorOverrides = previous;
            Menu.setApplicationMenu(buildAppMenu());
            logToFile?.('warn', 'menu accelerators rejected', { message: error?.message });
            return false;
        }
    }

    let menuState = { railOpen: true, contextOpen: false };

    function setMenuState(state) {
        menuState = { ...menuState, ...(state && typeof state === 'object' ? state : {}) };
        Menu.setApplicationMenu(buildAppMenu());
        return true;
    }

    function commandItem(label, command, fallbackAccelerator) {
        return { label, accelerator: menuAccelerator(command, fallbackAccelerator), click: () => sendAppCommand(command) };
    }

    function buildAppMenu() {
        return Menu.buildFromTemplate([
            { label: 'File', submenu: [
                commandItem('New Thread', 'new-surface', 'Ctrl+T'),
                commandItem('New Workspace', 'new-workspace', 'Ctrl+Shift+N'),
                { type: 'separator' },
                commandItem('Close Thread Tab', 'close-thread-tab'),
                commandItem('Reopen Closed Thread', 'reopen-closed-thread', 'Ctrl+Shift+T'),
                { type: 'separator' },
                commandItem('Settings', 'toggle-settings', 'Ctrl+,'),
                { type: 'separator' },
                { role: 'quit', label: 'Quit' },
            ] },
            { label: 'Edit', submenu: [
                { role: 'undo' }, { role: 'redo' }, { type: 'separator' }, { role: 'cut' },
                { label: 'Copy', accelerator: 'Ctrl+C', click: () => { getMainWindow()?.webContents.copy(); sendAppCommand('copy'); } },
                { label: 'Paste', accelerator: 'Ctrl+V', click: () => { getMainWindow()?.webContents.paste(); sendAppCommand('paste'); } },
                { label: 'Select All', accelerator: 'Ctrl+A', click: () => { getMainWindow()?.webContents.selectAll(); sendAppCommand('select-all'); } },
            ] },
            { label: 'View', submenu: [
                commandItem('Command Palette…', 'toggle-command-palette', 'Ctrl+Shift+P'),
                { type: 'separator' },
                { label: 'Sidebar', type: 'checkbox', checked: menuState.railOpen !== false, accelerator: menuAccelerator('toggle-rail'), click: () => sendAppCommand('toggle-rail') },
                { label: 'Context Panel', type: 'checkbox', checked: menuState.contextOpen === true, accelerator: menuAccelerator('toggle-sidebar', 'Ctrl+B'), click: () => sendAppCommand('toggle-sidebar') },
                commandItem('Files', 'toggle-file-manager', 'Ctrl+Shift+E'),
                { label: 'Thread Panels', submenu: [
                    commandItem('Participants', 'thread-panel-participants'),
                    commandItem('Handoff', 'thread-panel-handoff'),
                    commandItem('Gateway', 'thread-panel-gateway'),
                    commandItem('Operations', 'thread-panel-operations'),
                ] },
                { label: 'Tools', submenu: [
                    commandItem('Command History', 'toggle-command-history'),
                    commandItem('Session Vault', 'toggle-session-vault'),
                    commandItem('System Monitor', 'toggle-system-monitor'),
                    commandItem('Execution Canvas', 'toggle-canvas'),
                    commandItem('Time Travel Snapshots', 'toggle-time-travel'),
                ] },
                { type: 'separator' },
                { label: 'Split View', submenu: [
                    commandItem('Split Right', 'split-right'),
                    commandItem('Split Down', 'split-down'),
                    { type: 'separator' },
                    commandItem('Close Pane', 'split-close-pane'),
                    commandItem('Close Split View', 'split-close'),
                ] },
                { type: 'separator' }, { role: 'resetZoom', label: 'Actual Size' }, { role: 'zoomIn' }, { role: 'zoomOut' },
                { type: 'separator' }, { role: 'togglefullscreen', label: 'Full Screen' },
                { type: 'separator' }, { role: 'reload' }, { role: 'forceReload' },
                { label: 'Developer Tools', accelerator: devToolsAccelerator(), click: () => toggleWindowDevTools(getMainWindow()?.webContents) },
                { label: 'Take Screenshot', click: () => { const window = getMainWindow(); if (window) void takeScreenshot(window); } },
            ] },
            { label: 'Go', submenu: [
                commandItem('Back', 'thread-back'),
                { type: 'separator' },
                commandItem('Threads', 'go-threads'),
                commandItem('Code', 'go-code'),
                commandItem('Goals', 'go-goals'),
                commandItem('Workspaces', 'go-workspaces'),
                commandItem('Database', 'go-database'),
                commandItem('Tools', 'go-tools'),
                commandItem('Skills', 'go-skills'),
                commandItem('Inbox', 'go-activity'),
                commandItem('Usage', 'go-usage'),
                commandItem('Plugins', 'go-plugins'),
                { type: 'separator' },
                commandItem('Previous Thread', 'previous-thread', 'Ctrl+Shift+PageUp'),
                commandItem('Next Thread', 'next-thread', 'Ctrl+Shift+PageDown'),
                { type: 'separator' },
                commandItem('Search Threads…', 'toggle-search', 'Ctrl+Shift+F'),
            ] },
            { label: 'Window', submenu: [
                { label: 'New Window', accelerator: 'Ctrl+Alt+N', click: () => createWindow() },
                { type: 'separator' }, { role: 'minimize' },
                { label: 'Maximize', click: (_item, window) => { if (window?.isMaximized()) window.unmaximize(); else window?.maximize(); } },
                { role: 'close' },
            ] },
            { label: 'Help', submenu: [
                commandItem('Keyboard Shortcuts', 'keyboard-shortcuts'),
                commandItem('Developer Settings', 'developer-tools'),
                { type: 'separator' },
                commandItem('Check for Updates…', 'check-updates'),
                commandItem('About Zorai', 'about'),
            ] },
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
            ...windowAppearanceOptions,
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
        mainWindow.webContents.on('console-message', (_event, _level, message) => {
            if (typeof message === 'string' && message.startsWith('[code-timing]')) logToFile('info', message);
        });
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

    return { createWindow, defaultScreenshotDirectory, runContextMenuAction, setMenuAccelerators, setMenuState, setWindowOpacity };
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
    screenshotSoundCommands,
    playScreenshotSound,
    devToolsAccelerator,
    shouldOpenDevTools,
    toggleWindowDevTools,
};
