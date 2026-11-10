const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");

const { contextMenuRequest, devToolsAccelerator, loadWindowIcon, resolveWindowFrameOptions, resolveWindowIcon, resolveScreenshotDirectory, saveWindowScreenshot, screenshotFileName, screenshotSoundCommands, playScreenshotSound, shouldOpenDevTools, toggleWindowDevTools } = require("./main/window-runtime.cjs");

test("Linux windows use the PNG app icon", () => {
    assert.equal(
        resolveWindowIcon({ electronDir: "/repo/frontend/electron", path, platform: "linux" }),
        "/repo/frontend/assets/icon.png",
    );
});

test("Windows windows use the ICO app icon", () => {
    assert.equal(
        resolveWindowIcon({ electronDir: "/repo/frontend/electron", path, platform: "win32" }),
        "/repo/frontend/assets/icon.ico",
    );
});

test("runtime icon is loaded into an Electron NativeImage", () => {
    const loadedImage = { isEmpty: () => false };
    let loadedPath = null;
    const nativeImage = {
        createFromPath(iconPath) {
            loadedPath = iconPath;
            return loadedImage;
        },
    };

    assert.equal(
        loadWindowIcon({
            electronDir: "/repo/frontend/electron",
            nativeImage,
            path,
            platform: "linux",
        }),
        loadedImage,
    );
    assert.equal(loadedPath, "/repo/frontend/assets/icon.png");
});

test("runtime icon loading fails loudly for an empty image", () => {
    assert.throws(
        () => loadWindowIcon({
            electronDir: "/repo/frontend/electron",
            nativeImage: { createFromPath: () => ({ isEmpty: () => true }) },
            path,
            platform: "linux",
        }),
        /Failed to load Electron window icon.*icon\.png/,
    );
});

test("every platform drops the native title bar so the in-app title bar owns the chrome", () => {
    for (const platform of ["linux", "win32", "darwin"]) {
        assert.deepEqual(resolveWindowFrameOptions(platform), {
            frame: false,
            titleBarStyle: "hidden",
        });
    }
});

test("File menu opens another Zorai window from the first item", () => {
    const source = fs.readFileSync(path.join(__dirname, "main", "window-runtime.cjs"), "utf8");
    const fileMenu = source.slice(source.indexOf("label: 'File'"), source.indexOf("label: 'Edit'"));
    const newWindow = fileMenu.indexOf("label: 'New Window'");
    const newWorkspace = fileMenu.indexOf("label: 'New Workspace'");
    assert.ok(newWindow >= 0);
    assert.ok(newWindow < newWorkspace);
    assert.match(fileMenu, /label: 'New Window'[\s\S]*click: \(\) => createWindow\(\)/);
});

test("developer tools toggle opens a detached window so Linux can inspect the page", () => {
    assert.equal(devToolsAccelerator("linux"), "Ctrl+Shift+I");
    assert.equal(devToolsAccelerator("win32"), "Ctrl+Shift+I");
    assert.equal(devToolsAccelerator("darwin"), "Alt+Command+I");

    const opened = [];
    const contents = {
        isDevToolsOpened: () => opened.length > 0,
        openDevTools(options) { opened.push(options); },
        closeDevTools() { opened.length = 0; },
    };

    toggleWindowDevTools(contents);
    assert.deepEqual(opened, [{ mode: "detach" }]);
    toggleWindowDevTools(contents);
    assert.deepEqual(opened, []);
    toggleWindowDevTools(null);
});

test("dev builds do not auto-open DevTools unless ZORAI_DEVTOOLS is set", () => {
    assert.equal(shouldOpenDevTools({ isPackaged: false, env: {} }), false);
    assert.equal(shouldOpenDevTools({ isPackaged: false, env: { ZORAI_DEVTOOLS: "1" } }), true);
    assert.equal(shouldOpenDevTools({ isPackaged: true, env: { ZORAI_DEVTOOLS: "1" } }), false);
});

test("a context-menu screenshot captures the whole window, copies it, and keeps a sortable PNG", async () => {
    const directory = fs.mkdtempSync(path.join(require("node:os").tmpdir(), "zorai-shot-"));
    const png = Buffer.from("png-bytes");
    const copied = [];
    const image = { toPNG: () => png };
    const filePath = await saveWindowScreenshot({ capturePage: async () => image }, {
        directory: path.join(directory, "Zorai"),
        clipboard: { writeImage: (value) => copied.push(value) },
        fs,
        path,
        date: new Date(2026, 10, 9, 7, 5, 3),
    });

    assert.equal(path.basename(filePath), "zorai-2026-11-09_07-05-03.png");
    assert.deepEqual(fs.readFileSync(filePath), png);
    assert.deepEqual(copied, [image]);
    assert.equal(screenshotFileName(new Date(2026, 0, 2, 3, 4, 5)), "zorai-2026-01-02_03-04-05.png");
    fs.rmSync(directory, { recursive: true, force: true });
});

test("screenshots go to the folder chosen in settings, and fall back to Pictures/Zorai when unset or unreadable", () => {
    const directory = fs.mkdtempSync(path.join(require("node:os").tmpdir(), "zorai-shot-settings-"));
    const settingsFile = path.join(directory, "settings.json");
    const fallback = "/home/user/Pictures/Zorai";

    fs.writeFileSync(settingsFile, JSON.stringify({ settings: { screenshotDirectory: " /data/shots " } }));
    assert.equal(resolveScreenshotDirectory(settingsFile, fallback, fs), "/data/shots");
    fs.writeFileSync(settingsFile, JSON.stringify({ settings: { screenshotDirectory: "" } }));
    assert.equal(resolveScreenshotDirectory(settingsFile, fallback, fs), fallback);
    fs.writeFileSync(settingsFile, "{not json");
    assert.equal(resolveScreenshotDirectory(settingsFile, fallback, fs), fallback);
    assert.equal(resolveScreenshotDirectory(path.join(directory, "missing.json"), fallback, fs), fallback);
    fs.rmSync(directory, { recursive: true, force: true });
});

test("the right-click request only offers to open web links, so file: or javascript: targets never reach shell.openExternal", () => {
    const base = { x: 4, y: 8, isEditable: false, editFlags: { canCut: false, canCopy: true, canPaste: false } };
    assert.equal(contextMenuRequest({ ...base, linkURL: "https://zorai.dev" }).linkURL, "https://zorai.dev");
    assert.equal(contextMenuRequest({ ...base, linkURL: "javascript:alert(1)" }).linkURL, "");
    assert.equal(contextMenuRequest({ ...base, linkURL: "file:///etc/passwd" }).linkURL, "");
    assert.deepEqual(contextMenuRequest({ ...base, linkURL: "" }), { x: 4, y: 8, linkURL: "", isEditable: false, canCut: false, canCopy: true, canPaste: false });
});

test("screenshots play the desktop's own capture sound, falling back to the sound files", () => {
    const linux = screenshotSoundCommands("linux", () => true);
    assert.deepEqual(linux[0], ["canberra-gtk-play", ["-i", "screen-capture", "-d", "Zorai screenshot"]]);
    assert.deepEqual(linux[1], ["pw-play", ["/usr/share/sounds/freedesktop/stereo/screen-capture.oga"]]);
    assert.equal(screenshotSoundCommands("linux", () => false).length, 1);
    assert.equal(screenshotSoundCommands("darwin", () => true)[0][0], "afplay");
    assert.deepEqual(screenshotSoundCommands("win32", () => true), []);
});

test("a missing or failing sound player falls through to the next one instead of breaking the screenshot", () => {
    const { EventEmitter } = require("node:events");
    const tried = [];
    const spawn = (command) => {
        tried.push(command);
        const child = new EventEmitter();
        setImmediate(() => (command === "canberra-gtk-play" ? child.emit("error", new Error("ENOENT")) : command === "pw-play" ? child.emit("exit", 1) : child.emit("exit", 0)));
        return child;
    };
    playScreenshotSound({ spawn, platform: "linux", exists: () => true });
    return new Promise((resolve) => setTimeout(() => {
        assert.deepEqual(tried, ["canberra-gtk-play", "pw-play", "paplay"]);
        resolve();
    }, 20));
});
