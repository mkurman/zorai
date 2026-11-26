const test = require('node:test');
const assert = require('node:assert/strict');
const { nextZoomLevel, zoomActionForInput } = require('./main/zoom-keys.cjs');

const key = (overrides) => ({ type: 'keyDown', control: true, meta: false, alt: false, shift: false, key: '', code: '', ...overrides });

test('every common zoom-in chord works on Linux, not only the shifted "+" that the default menu accelerator needs', () => {
    assert.equal(zoomActionForInput(key({ key: '=', code: 'Equal' })), 'in');
    assert.equal(zoomActionForInput(key({ key: '+', code: 'Equal', shift: true })), 'in');
    assert.equal(zoomActionForInput(key({ key: '+', code: 'NumpadAdd' })), 'in');
    assert.equal(zoomActionForInput(key({ key: '-', code: 'Minus' })), 'out');
    assert.equal(zoomActionForInput(key({ key: '0', code: 'Digit0' })), 'reset');
    assert.equal(zoomActionForInput(key({ key: 'Insert', code: 'Numpad0' })), 'reset');
});

test('plain typing, Alt chords and key releases never change the zoom', () => {
    assert.equal(zoomActionForInput(key({ control: false, key: '=' })), null);
    assert.equal(zoomActionForInput(key({ alt: true, key: '=' })), null);
    assert.equal(zoomActionForInput(key({ type: 'keyUp', key: '=' })), null);
    assert.equal(zoomActionForInput(key({ key: 'a' })), null);
});

test('zoom is clamped so the UI can never shrink to unreadable or be lost, and reset returns to 100%', () => {
    assert.equal(nextZoomLevel(-3, 'out'), -3);
    assert.equal(nextZoomLevel(5, 'in'), 5);
    assert.equal(nextZoomLevel(-8, 'in'), -3);
    assert.equal(nextZoomLevel(-8, 'reset'), 0);
});

test('while Code is open the zoom chords go to the editor font commands instead of the app zoom', () => {
    const { attachZoomKeys } = require('./main/zoom-keys.cjs');
    const listeners = {};
    const ipcMain = { on: (channel, handler) => { listeners[channel] = handler; } };
    let level = 0;
    const webContents = {
        on: (channel, handler) => { listeners[channel] = handler; },
        getZoomLevel: () => level,
        setZoomLevel: (next) => { level = next; },
    };
    attachZoomKeys(webContents, ipcMain);
    const press = () => {
        let prevented = false;
        listeners['before-input-event']({ preventDefault: () => { prevented = true; } }, key({ key: '=' }));
        return prevented;
    };
    assert.equal(press(), true);
    assert.equal(level, 0.5);
    listeners['zoom-keys-yield']({ sender: webContents }, true);
    assert.equal(press(), false);
    assert.equal(level, 0.5);
    listeners['zoom-keys-yield']({ sender: {} }, false);
    assert.equal(press(), false);
    listeners['zoom-keys-yield']({ sender: webContents }, false);
    assert.equal(press(), true);
    assert.equal(level, 1);
});
