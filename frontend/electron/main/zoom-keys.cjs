const ZOOM_STEP = 0.5;
const MIN_ZOOM_LEVEL = -3;
const MAX_ZOOM_LEVEL = 5;

function zoomActionForInput(input) {
    if (input?.type !== 'keyDown' || !(input.control || input.meta) || input.alt) return null;
    const { key, code } = input;
    if (key === '+' || key === '=' || code === 'NumpadAdd') return 'in';
    if (key === '-' || key === '_' || code === 'NumpadSubtract') return 'out';
    if (key === '0' || code === 'Numpad0') return 'reset';
    return null;
}

function nextZoomLevel(current, action) {
    if (action === 'reset') return 0;
    const delta = action === 'in' ? ZOOM_STEP : -ZOOM_STEP;
    return Math.min(MAX_ZOOM_LEVEL, Math.max(MIN_ZOOM_LEVEL, current + delta));
}

function attachZoomKeys(webContents, ipcMain = require('electron').ipcMain) {
    let yieldToRenderer = false;
    ipcMain.on('zoom-keys-yield', (event, value) => {
        if (event.sender === webContents) yieldToRenderer = Boolean(value);
    });
    webContents.on('before-input-event', (event, input) => {
        const action = zoomActionForInput(input);
        if (!action || yieldToRenderer) return;
        event.preventDefault();
        webContents.setZoomLevel(nextZoomLevel(webContents.getZoomLevel(), action));
    });
}

module.exports = { attachZoomKeys, nextZoomLevel, zoomActionForInput };
