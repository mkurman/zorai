const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");

const { devToolsAccelerator, loadWindowIcon, resolveWindowFrameOptions, resolveWindowIcon, shouldOpenDevTools, toggleWindowDevTools } = require("./main/window-runtime.cjs");

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

test("Linux windows use native window-manager chrome", () => {
    assert.deepEqual(resolveWindowFrameOptions("linux"), {
        frame: true,
        titleBarStyle: "default",
    });
});

test("Windows keeps its native window frame", () => {
    assert.deepEqual(resolveWindowFrameOptions("win32"), {
        frame: true,
        titleBarStyle: "default",
    });
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

test("macOS preserves its existing hidden title bar", () => {
    assert.deepEqual(resolveWindowFrameOptions("darwin"), {
        frame: false,
        titleBarStyle: "hidden",
    });
});
