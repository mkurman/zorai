const test = require("node:test");
const assert = require("node:assert/strict");
const { EventEmitter } = require("node:events");

const { createComputerControlOverlay, STOP_URL } = require("./main/computer-control-overlay.cjs");

function fakeElectron(displays) {
    const created = [];
    class FakeWindow extends EventEmitter {
        constructor(options) {
            super();
            this.options = options;
            this.destroyed = false;
            this.ignoresMouse = false;
            this.contentProtected = false;
            this.webContents = new EventEmitter();
            this.webContents.setWindowOpenHandler = () => {};
            created.push(this);
        }
        setAlwaysOnTop() {}
        setVisibleOnAllWorkspaces() {}
        setContentProtection(value) { this.contentProtected = value; }
        setIgnoreMouseEvents(value) { this.ignoresMouse = value; }
        loadURL() { return Promise.resolve(); }
        showInactive() {}
        isDestroyed() { return this.destroyed; }
        destroy() { this.destroyed = true; }
    }
    const screen = {
        getAllDisplays: () => displays,
        getPrimaryDisplay: () => displays[0],
    };
    return { BrowserWindow: FakeWindow, screen, created };
}

const DISPLAYS = [
    { bounds: { x: 0, y: 0, width: 1920, height: 1080 }, workArea: { x: 0, y: 0, width: 1920, height: 1040 } },
    { bounds: { x: 1920, y: 0, width: 2560, height: 1440 }, workArea: { x: 1920, y: 0, width: 2560, height: 1400 } },
];

function setup() {
    const electron = fakeElectron(DISPLAYS);
    const mainWindow = new EventEmitter();
    mainWindow.isDestroyed = () => false;
    mainWindow.focused = 0;
    mainWindow.restored = 0;
    mainWindow.isMinimized = () => true;
    mainWindow.restore = () => { mainWindow.restored += 1; };
    mainWindow.show = () => {};
    mainWindow.setAlwaysOnTop = () => {};
    mainWindow.focus = () => { mainWindow.focused += 1; };
    let stops = 0;
    const overlay = createComputerControlOverlay({
        BrowserWindow: electron.BrowserWindow,
        screen: electron.screen,
        getMainWindow: () => mainWindow,
        onStop: () => { stops += 1; },
        logToFile: () => {},
    });
    return { overlay, electron, mainWindow, stops: () => stops };
}

test("the operator sees the control frame on every display, not just inside the Zorai window", () => {
    const { overlay, electron } = setup();
    overlay.apply({ active: true });

    const frames = electron.created.filter((win) => win.ignoresMouse);
    assert.deepEqual(frames.map((win) => [win.options.x, win.options.width]), [[0, 1920], [1920, 2560]]);
});

test("the frame never swallows the agent's clicks and stays out of its screenshots", () => {
    const { overlay, electron } = setup();
    overlay.apply({ active: true });

    const frames = electron.created.filter((win) => win.ignoresMouse);
    assert.ok(frames.every((win) => win.options.focusable === false && win.contentProtected));
});

test("the Stop button outside the app actually stops the agent", () => {
    const { overlay, electron, stops } = setup();
    overlay.apply({ active: true });

    const banner = electron.created.find((win) => !win.ignoresMouse);
    let prevented = false;
    banner.webContents.emit("will-navigate", { preventDefault: () => { prevented = true; } }, STOP_URL);

    assert.equal(prevented, true);
    assert.equal(stops(), 1);
});

test("the overlay disappears when control ends, the bridge drops, or the app window closes", () => {
    const { overlay, electron, mainWindow } = setup();

    overlay.apply({ active: true });
    overlay.apply({ active: true });
    assert.equal(electron.created.length, 3);
    overlay.apply({ active: false });
    assert.ok(electron.created.every((win) => win.destroyed));

    overlay.apply({ active: true });
    mainWindow.emit("close");
    assert.ok(electron.created.every((win) => win.destroyed));
    assert.equal(mainWindow.listenerCount("close"), 0);
});

test("when the agent finishes its turn, Zorai comes back to the front so the operator sees the answer", () => {
    const { overlay, mainWindow } = setup();

    overlay.apply({ active: true });
    overlay.apply({ active: false, reason: "released after 2 minutes without actions" });
    assert.equal(mainWindow.focused, 0);

    overlay.apply({ active: true });
    overlay.apply({ active: false, return_to_app: true });
    assert.equal(mainWindow.restored, 1);
    assert.equal(mainWindow.focused, 1);
});
