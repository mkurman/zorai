const test = require('node:test');
const assert = require('node:assert/strict');
const os = require('node:os');
const path = require('node:path');
const { mediaPathFromUrl, isServableMediaPath } = require('./main/media-protocol.cjs');

test('media URLs decode back to the file on disk, including spaces, ~ and #', () => {
    assert.equal(mediaPathFromUrl('zorai-media://local/home/me/Downloads/Gabby%20-%20Soft%20(1).mp3'), '/home/me/Downloads/Gabby - Soft (1).mp3');
    assert.equal(mediaPathFromUrl('zorai-media://local/~/Downloads/promo.mp4'), path.join(os.homedir(), 'Downloads/promo.mp4'));
    assert.equal(mediaPathFromUrl('zorai-media://local/tmp/clip%23take2.webm'), '/tmp/clip#take2.webm');
    assert.equal(mediaPathFromUrl('https://example.com/a.mp4'), null);
});

test('only media files are served, so the scheme cannot be used to read arbitrary files', () => {
    assert.equal(isServableMediaPath('/tmp/a.mp4'), true);
    assert.equal(isServableMediaPath('/tmp/a.MP3'), true);
    assert.equal(isServableMediaPath('/home/me/.ssh/id_ed25519'), false);
    assert.equal(isServableMediaPath('/etc/passwd'), false);
});
