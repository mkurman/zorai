const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { readImageFileAsDataUrl, AUDIO_FILE_MIME_TYPES } = require('./main/agent-ipc-handlers.cjs');

test('audio files are read as playable data URLs, and non-audio files are refused', async () => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'zorai-audio-'));
    const mp3 = path.join(dir, 'speech.mp3');
    fs.writeFileSync(mp3, Buffer.from('ID3fake'));
    assert.equal(await readImageFileAsDataUrl(mp3, AUDIO_FILE_MIME_TYPES), `data:audio/mpeg;base64,${Buffer.from('ID3fake').toString('base64')}`);
    const text = path.join(dir, 'notes.txt');
    fs.writeFileSync(text, 'hi');
    assert.equal(await readImageFileAsDataUrl(text, AUDIO_FILE_MIME_TYPES), null);
    assert.equal(await readImageFileAsDataUrl(mp3), null);
});
